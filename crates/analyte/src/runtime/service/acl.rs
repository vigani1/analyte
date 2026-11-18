//! Windows-only: creates and checks private access lists for protected storage (ADR 0017).
use std::{
    ffi::{OsStr, c_void},
    io,
    os::windows::ffi::OsStrExt as _,
    path::Path,
    ptr,
};
use windows_sys::Win32::{
    Foundation::{
        CloseHandle, ERROR_ALREADY_EXISTS, ERROR_NONE_MAPPED, GetLastError, HANDLE, LocalFree,
    },
    Security::{
        ACCESS_ALLOWED_ACE, ACE_HEADER, ACL, ACL_SIZE_INFORMATION, AclSizeInformation,
        Authorization::{
            ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
            GetNamedSecurityInfoW, SDDL_REVISION_1, SE_FILE_OBJECT,
        },
        DACL_SECURITY_INFORMATION, GetAce, GetAclInformation, GetTokenInformation,
        LookupAccountNameW, OWNER_SECURITY_INFORMATION, PSID, SECURITY_ATTRIBUTES,
        SECURITY_MAX_SID_SIZE, SID_NAME_USE, TOKEN_QUERY, TOKEN_USER, TokenUser,
    },
    Storage::FileSystem::CreateDirectoryW,
    System::Threading::{GetCurrentProcess, OpenProcessToken},
};

/// The installer's virtual service account (`packaging/windows/install.ps1`).
const SERVICE_ACCOUNT: &str = "NT SERVICE\\Analyte";
/// Well-known SIDs for SYSTEM and BUILTIN\Administrators.
const ALWAYS_ALLOWED: [&str; 2] = ["S-1-5-18", "S-1-5-32-544"];
/// ACE types from winnt.h; deny entries only remove access.
const ALLOWED_ACE: u8 = 0;
const DENIED_ACE: u8 = 1;
/// Canonical SID strings are far shorter; bounds the terminator search.
const SID_TEXT_LIMIT: usize = 256;

/// Why a protected path's access list was refused; names principals by SID only.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// An allow entry, explicit or inherited, names another principal.
    #[error(
        "protected path grants access to {sid}; allow only SYSTEM, Administrators and the Analyte account"
    )]
    Grant {
        /// The offending principal.
        sid: String,
    },
    /// The owner can rewrite the access list, so it must be an allowed principal.
    #[error(
        "protected path is owned by {sid}; the owner must be SYSTEM, Administrators or the Analyte account"
    )]
    Owner {
        /// The offending owner.
        sid: String,
    },
    /// A missing access list grants everyone full access.
    #[error("protected path has no access list, so every user has access")]
    NoAccessList,
    /// Object and conditional entries are not judged; refuse rather than guess.
    #[error("protected path has an unsupported access entry type {0}")]
    UnsupportedEntry(u8),
    /// A Windows security call failed.
    #[error("Windows {call} failed")]
    Os {
        /// The failed API.
        call: &'static str,
        /// The OS cause.
        #[source]
        source: io::Error,
    },
}

fn os(call: &'static str) -> Error {
    Error::Os {
        call,
        source: io::Error::last_os_error(),
    }
}

/// Frees memory the security APIs allocate with `LocalAlloc`.
struct Local(*mut c_void);
impl Drop for Local {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: the pointer came from an API documented to require LocalFree, once.
            unsafe { LocalFree(self.0) };
        }
    }
}

fn wide(text: impl AsRef<OsStr>) -> Result<Vec<u16>, Error> {
    let text: Vec<u16> = text.as_ref().encode_wide().collect();
    if text.contains(&0) {
        let source = io::Error::from(io::ErrorKind::InvalidInput);
        return Err(Error::Os {
            call: "path conversion",
            source,
        });
    }
    Ok(text.into_iter().chain([0]).collect())
}

/// # Safety
/// `sid` must point to a valid SID that stays alive for the call.
unsafe fn sid_text(sid: PSID) -> Result<String, Error> {
    let mut text = Local(ptr::null_mut());
    // SAFETY: the caller guarantees `sid`; the out pointer is a valid local.
    if unsafe { ConvertSidToStringSidW(sid, ptr::from_mut(&mut text.0).cast()) } == 0 {
        return Err(os("ConvertSidToStringSidW"));
    }
    let start = text.0.cast::<u16>();
    // SAFETY: success returns a NUL-terminated string; reading stops at NUL or the bound.
    let len = (0..SID_TEXT_LIMIT).find(|&i| unsafe { start.add(i).read() } == 0);
    let len = len.ok_or(Error::Os {
        call: "SID text",
        source: io::ErrorKind::InvalidData.into(),
    })?;
    // SAFETY: `len` units before the terminator were just read from this allocation.
    Ok(String::from_utf16_lossy(unsafe {
        std::slice::from_raw_parts(start, len)
    }))
}

/// The SID of the account this process runs as.
pub(crate) fn process_account() -> Result<String, Error> {
    struct Token(HANDLE);
    impl Drop for Token {
        fn drop(&mut self) {
            // SAFETY: the handle was opened by OpenProcessToken and is closed once.
            unsafe { CloseHandle(self.0) };
        }
    }
    let mut handle = ptr::null_mut();
    // SAFETY: the pseudo-handle needs no closing; the out pointer is a valid local.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &raw mut handle) } == 0 {
        return Err(os("OpenProcessToken"));
    }
    let token = Token(handle);
    // u64 storage aligns TOKEN_USER; 128 bytes exceeds TOKEN_USER plus the largest SID.
    let mut buffer = [0u64; 16];
    let mut used = 0;
    // SAFETY: the buffer is writable for the stated length and outlives the call.
    let ok = unsafe {
        GetTokenInformation(
            token.0,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            128,
            &raw mut used,
        )
    };
    if ok == 0 {
        return Err(os("GetTokenInformation"));
    }
    // SAFETY: success wrote an aligned TOKEN_USER whose SID points into `buffer`.
    unsafe { sid_text((*buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid) }
}

/// The installed service account's SID, or None when the service is not installed.
fn service_account() -> Result<Option<String>, Error> {
    let name = wide(SERVICE_ACCOUNT)?;
    let mut sid = [0u32; SECURITY_MAX_SID_SIZE as usize / 4];
    let mut domain = [0u16; 256];
    let (mut sid_len, mut domain_len, mut kind) =
        (SECURITY_MAX_SID_SIZE, 256, SID_NAME_USE::default());
    // SAFETY: every buffer is writable for the length passed and outlives the call.
    let ok = unsafe {
        LookupAccountNameW(
            ptr::null(),
            name.as_ptr(),
            sid.as_mut_ptr().cast(),
            &raw mut sid_len,
            domain.as_mut_ptr(),
            &raw mut domain_len,
            &raw mut kind,
        )
    };
    if ok != 0 {
        // SAFETY: success wrote a valid SID into `sid`.
        return unsafe { sid_text(sid.as_mut_ptr().cast()) }.map(Some);
    }
    // SAFETY: reads this thread's last error immediately after the failed call.
    if unsafe { GetLastError() } == ERROR_NONE_MAPPED {
        Ok(None)
    } else {
        Err(os("LookupAccountNameW"))
    }
}

fn allowed() -> Result<Vec<String>, Error> {
    let mut sids: Vec<String> = ALWAYS_ALLOWED.map(String::from).into();
    sids.push(process_account()?);
    sids.extend(service_account()?);
    Ok(sids)
}

/// Creates the final directory, if nothing exists there, with the installer's
/// protected access list. An existing entry is left for the caller to inspect.
/// # Errors
/// Reports the failed Windows call.
pub(crate) fn create_private(path: &Path) -> Result<(), Error> {
    // Protected (P): nothing inherited from a broader parent; 0x1301bf is Modify.
    let me = process_account()?;
    let sddl = wide(format!(
        "D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;0x1301bf;;;{me})"
    ))?;
    let mut descriptor = Local(ptr::null_mut());
    // SAFETY: `sddl` is NUL-terminated; the out pointer is a valid local freed by `Local`.
    let ok = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &raw mut descriptor.0,
            ptr::null_mut(),
        )
    };
    if ok == 0 {
        return Err(os("ConvertStringSecurityDescriptorToSecurityDescriptorW"));
    }
    let attributes = SECURITY_ATTRIBUTES {
        nLength: u32::try_from(size_of::<SECURITY_ATTRIBUTES>()).unwrap_or(u32::MAX),
        lpSecurityDescriptor: descriptor.0,
        bInheritHandle: 0,
    };
    let target = wide(path)?;
    // SAFETY: the path is NUL-terminated and the descriptor lives until after the call.
    if unsafe { CreateDirectoryW(target.as_ptr(), &raw const attributes) } == 0
        // SAFETY: reads this thread's last error immediately after the failed call.
        && unsafe { GetLastError() } != ERROR_ALREADY_EXISTS
    {
        return Err(os("CreateDirectoryW"));
    }
    Ok(())
}

/// Refuses an owner or any allow entry, inherited or explicit, outside SYSTEM,
/// Administrators, the process account and the installed service account.
/// # Errors
/// Names the first offending principal, or the failed Windows call.
pub(crate) fn check(path: &Path) -> Result<(), Error> {
    let allowed = allowed()?;
    let judge = |sid: String, refuse: fn(String) -> Error| {
        if allowed.contains(&sid) {
            Ok(())
        } else {
            Err(refuse(sid))
        }
    };
    let target = wide(path)?;
    let (mut owner, mut dacl, mut descriptor) = (
        ptr::null_mut(),
        ptr::null_mut::<ACL>(),
        Local(ptr::null_mut()),
    );
    // SAFETY: the path is NUL-terminated; out pointers are valid locals; `Local` frees the descriptor.
    let status = unsafe {
        GetNamedSecurityInfoW(
            target.as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &raw mut owner,
            ptr::null_mut(),
            &raw mut dacl,
            ptr::null_mut(),
            &raw mut descriptor.0,
        )
    };
    if status != 0 {
        let source = io::Error::from_raw_os_error(i32::try_from(status).unwrap_or(i32::MAX));
        return Err(Error::Os {
            call: "GetNamedSecurityInfoW",
            source,
        });
    }
    // SAFETY: `owner` points into the live descriptor.
    judge(unsafe { sid_text(owner) }?, |sid| Error::Owner { sid })?;
    if dacl.is_null() {
        return Err(Error::NoAccessList);
    }
    let mut size = ACL_SIZE_INFORMATION::default();
    let length = u32::try_from(size_of::<ACL_SIZE_INFORMATION>()).unwrap_or(u32::MAX);
    // SAFETY: `dacl` points into the live descriptor; `size` is writable for `length`.
    if unsafe { GetAclInformation(dacl, (&raw mut size).cast(), length, AclSizeInformation) } == 0 {
        return Err(os("GetAclInformation"));
    }
    for index in 0..size.AceCount {
        let mut ace = ptr::null_mut();
        // SAFETY: `index` is below the ACE count of the live access list.
        if unsafe { GetAce(dacl, index, &raw mut ace) } == 0 {
            return Err(os("GetAce"));
        }
        // SAFETY: GetAce returned a pointer to an ACE inside the live access list.
        let kind = unsafe { ace.cast::<ACE_HEADER>().read_unaligned() }.AceType;
        match kind {
            DENIED_ACE => {}
            ALLOWED_ACE => {
                let start = std::mem::offset_of!(ACCESS_ALLOWED_ACE, SidStart);
                let sid = ace.cast::<u8>().wrapping_add(start).cast();
                // SAFETY: an allow ACE stores its SID at `SidStart`, inside the live access list.
                judge(unsafe { sid_text(sid) }?, |sid| Error::Grant { sid })?;
            }
            other => return Err(Error::UnsupportedEntry(other)),
        }
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::{Error, check, process_account};
    use crate::runtime::{Error as Failure, service::create_data_directory};
    use std::path::Path;

    pub(crate) const USERS: &str = "S-1-5-32-545";

    /// Changes an access list with the stock tool, independently of the code under test.
    pub(crate) fn icacls(path: &Path, args: &[&str]) -> Result<(), crate::Error> {
        let status = std::process::Command::new("icacls")
            .arg(path)
            .args(args)
            .output()?
            .status;
        if status.success() {
            Ok(())
        } else {
            Err(format!("icacls {args:?}: {status}").into())
        }
    }

    fn refused_sid(result: Result<(), Failure>) -> Option<String> {
        match result {
            Err(Failure::InsecureWindowsDataDirectory(Error::Grant { sid })) => Some(sid),
            _ => None,
        }
    }

    // ADR 0017 / rule 10: a folder inheriting BUILTIN\Users read access is refused by SID.
    // ADR 0017
    #[test]
    fn inherited_users_read_is_refused() -> Result<(), crate::Error> {
        let root = tempfile::tempdir()?;
        icacls(root.path(), &["/grant", &format!("*{USERS}:(OI)(CI)RX")])?;
        let data = root.path().join("data");
        std::fs::create_dir(&data)?;
        assert_eq!(
            refused_sid(create_data_directory(&data)),
            Some(USERS.into())
        );
        Ok(())
    }

    // ADR 0017: a folder the program creates drops broad inherited grants and passes.
    // ADR 0017
    #[test]
    fn created_directory_is_private() -> Result<(), crate::Error> {
        let root = tempfile::tempdir()?;
        icacls(root.path(), &["/grant", &format!("*{USERS}:(OI)(CI)RX")])?;
        let data = root.path().join("nested").join("data");
        create_data_directory(&data)?;
        check(&data)?;
        Ok(())
    }

    // ADR 0017 / packaging/windows/install.ps1: the installer's protected shape passes.
    // ADR 0017
    #[test]
    fn installer_access_list_passes() -> Result<(), crate::Error> {
        let root = tempfile::tempdir()?;
        let data = root.path().join("data");
        std::fs::create_dir(&data)?;
        let account = format!("*{}:(OI)(CI)M", process_account()?);
        let grants = ["*S-1-5-18:(OI)(CI)F", "*S-1-5-32-544:(OI)(CI)F", &account];
        icacls(
            &data,
            &[&["/inheritance:r", "/grant:r"][..], &grants].concat(),
        )?;
        create_data_directory(&data)?;
        Ok(())
    }
}
