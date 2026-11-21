//! Windows-only: creates and checks private access lists for protected storage (ADR 0017).
use super::caller::{Local, Os, account_sid, process_account, sid_text, wide};
use std::{io, path::Path, ptr};
use windows_sys::Win32::{
    Foundation::{ERROR_ALREADY_EXISTS, GetLastError},
    Security::{
        ACCESS_ALLOWED_ACE, ACE_HEADER, ACL, ACL_SIZE_INFORMATION, AclSizeInformation,
        Authorization::{
            ConvertStringSecurityDescriptorToSecurityDescriptorW, GetNamedSecurityInfoW,
            SDDL_REVISION_1, SE_FILE_OBJECT,
        },
        DACL_SECURITY_INFORMATION, GetAce, GetAclInformation, OWNER_SECURITY_INFORMATION,
        SECURITY_ATTRIBUTES,
    },
    Storage::FileSystem::CreateDirectoryW,
};

/// The installer's virtual service account (`packaging/windows/install.ps1`).
const SERVICE_ACCOUNT: &str = "NT SERVICE\\Analyte";
/// Well-known SID for SYSTEM.
const SYSTEM: &str = "S-1-5-18";
/// Well-known SIDs for SYSTEM and BUILTIN\Administrators.
const ALWAYS_ALLOWED: [&str; 2] = [SYSTEM, "S-1-5-32-544"];
/// ACE types from winnt.h; deny entries only remove access.
const ALLOWED_ACE: u8 = 0;
const DENIED_ACE: u8 = 1;

/// Who a protected path may grant access to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Readers {
    /// SYSTEM, Administrators, the process account and the installed service
    /// account: the data directory and the admin token.
    Operators,
    /// SYSTEM, the process account and the installed service account only: the
    /// locked credentials folder and file. Administrators may own it but get no grant.
    ServiceOnly,
}

/// Why a protected path's access list was refused; names principals by SID only.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// An allow entry, explicit or inherited, names another principal.
    #[error(
        "protected path grants access to {sid}; allow only SYSTEM, the Analyte account and, outside the credentials folder, Administrators"
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

impl From<Os> for Error {
    fn from(Os { call, source }: Os) -> Self {
        Self::Os { call, source }
    }
}

/// The installed service account's SID, or None when the service is not installed.
fn service_account() -> Result<Option<String>, Error> {
    Ok(account_sid(SERVICE_ACCOUNT)?.map(|(sid, _)| sid))
}

/// The principals an allow entry may name, and those that may own the path.
fn allowed(readers: Readers) -> Result<(Vec<String>, Vec<String>), Error> {
    let mut accounts = vec![process_account()?];
    accounts.extend(service_account()?);
    let owners: Vec<String> = ALWAYS_ALLOWED
        .iter()
        .map(|sid| (*sid).to_owned())
        .chain(accounts.iter().cloned())
        .collect();
    let grants = match readers {
        Readers::Operators => owners.clone(),
        Readers::ServiceOnly => [SYSTEM.to_owned()].into_iter().chain(accounts).collect(),
    };
    Ok((grants, owners))
}

/// Creates the final directory, if nothing exists there, with a protected access
/// list for `readers`; files created inside inherit it. An existing entry is left
/// for the caller to inspect.
/// # Errors
/// Reports the failed Windows call.
pub(crate) fn create_private(path: &Path, readers: Readers) -> Result<(), Error> {
    // Protected (P): nothing inherited from a broader parent; 0x1301bf is Modify.
    let me = process_account()?;
    let sddl = wide(match readers {
        Readers::Operators => {
            format!("D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;0x1301bf;;;{me})")
        }
        Readers::ServiceOnly => format!("D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;{me})"),
    })?;
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

/// Refuses an owner outside SYSTEM, Administrators, the process account and the
/// installed service account, and any allow entry, inherited or explicit, naming a
/// principal outside `readers`.
/// # Errors
/// Names the first offending principal, or the failed Windows call.
pub(crate) fn check(path: &Path, readers: Readers) -> Result<(), Error> {
    let (grants, owners) = allowed(readers)?;
    let judge = |allowed: &[String], sid: String, refuse: fn(String) -> Error| {
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
    judge(&owners, unsafe { sid_text(owner) }?, |sid| Error::Owner {
        sid,
    })?;
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
                judge(&grants, unsafe { sid_text(sid) }?, |sid| Error::Grant {
                    sid,
                })?;
            }
            other => return Err(Error::UnsupportedEntry(other)),
        }
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::{Error, Readers, check};
    use crate::runtime::service::caller::process_account;
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
        check(&data, Readers::Operators)?;
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
