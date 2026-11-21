//! Windows-only: accounts, tokens and SIDs, and who is asking on the sign-in pipe (ADR 0021; its helpers also serve ADR 0017's access lists).
use std::{
    ffi::{OsStr, c_void},
    io,
    os::windows::{
        ffi::OsStrExt as _,
        io::{AsHandle as _, AsRawHandle as _, OwnedHandle},
    },
    ptr,
};
use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
use windows_sys::Win32::{
    Foundation::{
        CloseHandle, ERROR_ACCESS_DENIED, ERROR_NONE_MAPPED, ERROR_PIPE_BUSY, GetLastError, HANDLE,
        LocalFree,
    },
    Security::{
        Authorization::{
            ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
            SDDL_REVISION_1,
        },
        GetTokenInformation, LookupAccountNameW, LookupAccountSidW, PSID, RevertToSelf,
        SECURITY_ATTRIBUTES, SECURITY_MAX_SID_SIZE, SID_AND_ATTRIBUTES, SID_NAME_USE, TOKEN_GROUPS,
        TOKEN_QUERY, TOKEN_USER, TokenGroups, TokenUser,
    },
    System::{
        Pipes::ImpersonateNamedPipeClient,
        Threading::{GetCurrentProcess, GetCurrentThread, OpenProcessToken, OpenThreadToken},
    },
};

/// Canonical SID strings are far shorter; bounds the terminator search.
const SID_TEXT_LIMIT: usize = 256;
/// Account and domain names are read into buffers of this many UTF-16 units each;
/// a longer name is not read (the caller is then shown by SID).
const NAME_LIMIT: u32 = 256;
/// A token's group list larger than this is refused rather than read: Windows
/// caps a token near 1,000 groups, far under 64 KiB.
const GROUPS_LIMIT: u32 = 64 * 1024;
/// The sign-in pipe's access list (ADR 0021): SYSTEM and Administrators full
/// control, interactive logons read and write; protected, so nothing is inherited.
const PIPE_SDDL: &str = "D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GRGW;;;IU)";

/// A failed Windows call, named by its API.
#[derive(Debug, thiserror::Error)]
#[error("Windows {call} failed")]
pub(crate) struct Os {
    /// The failed API.
    pub(super) call: &'static str,
    /// The OS cause.
    #[source]
    pub(super) source: io::Error,
}

fn os(call: &'static str) -> Os {
    Os {
        call,
        source: io::Error::last_os_error(),
    }
}

/// Why the sign-in pipe could not be made or its caller could not be read.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    /// A Windows call failed.
    #[error(transparent)]
    Os(#[from] Os),
    /// Another program already holds the pipe's name, or Windows refused the
    /// first instance.
    #[error("the sign-in pipe's name is already taken")]
    Taken(#[source] io::Error),
    /// The caller's token reports a group list larger than [`GROUPS_LIMIT`].
    #[error("the caller's token lists {0} bytes of groups, more than is read")]
    TooManyGroups(u32),
    /// The token's group list does not fit inside what Windows returned.
    #[error("the caller's token group list is malformed")]
    MalformedGroups,
    /// The impersonating thread could not stop impersonating; it ends without
    /// doing anything else, so the identity cannot reach other work.
    #[error("Windows RevertToSelf failed after reading the caller")]
    Revert(#[source] io::Error),
}

/// Frees memory the security APIs allocate with `LocalAlloc`.
pub(super) struct Local(pub(super) *mut c_void);
impl Drop for Local {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: the pointer came from an API documented to require LocalFree, once.
            unsafe { LocalFree(self.0) };
        }
    }
}

/// Closes a token handle once.
struct Token(HANDLE);
impl Drop for Token {
    fn drop(&mut self) {
        // SAFETY: the handle was opened by OpenProcessToken or OpenThreadToken and is closed once.
        unsafe { CloseHandle(self.0) };
    }
}

/// `text` as a NUL-terminated wide string; refuses an embedded NUL.
pub(super) fn wide(text: impl AsRef<OsStr>) -> Result<Vec<u16>, Os> {
    let text: Vec<u16> = text.as_ref().encode_wide().collect();
    if text.contains(&0) {
        let source = io::Error::from(io::ErrorKind::InvalidInput);
        return Err(Os {
            call: "path conversion",
            source,
        });
    }
    Ok(text.into_iter().chain([0]).collect())
}

/// # Safety
/// `sid` must point to a valid SID that stays alive for the call.
pub(super) unsafe fn sid_text(sid: PSID) -> Result<String, Os> {
    let mut text = Local(ptr::null_mut());
    // SAFETY: the caller guarantees `sid`; the out pointer is a valid local.
    if unsafe { ConvertSidToStringSidW(sid, ptr::from_mut(&mut text.0).cast()) } == 0 {
        return Err(os("ConvertSidToStringSidW"));
    }
    let start = text.0.cast::<u16>();
    // SAFETY: success returns a NUL-terminated string; reading stops at NUL or the bound.
    let len = (0..SID_TEXT_LIMIT).find(|&i| unsafe { start.add(i).read() } == 0);
    let len = len.ok_or(Os {
        call: "SID text",
        source: io::ErrorKind::InvalidData.into(),
    })?;
    // SAFETY: `len` units before the terminator were just read from this allocation.
    Ok(String::from_utf16_lossy(unsafe {
        std::slice::from_raw_parts(start, len)
    }))
}

/// A token's user, kept on the heap so its SID pointer stays valid after moves.
struct User(Box<[u64; 16]>);

impl User {
    fn read(token: &Token) -> Result<Self, Os> {
        // u64 storage aligns TOKEN_USER; 128 bytes exceeds TOKEN_USER plus the largest SID.
        let mut buffer = Box::new([0u64; 16]);
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
        Ok(Self(buffer))
    }

    /// The user's SID, pointing into this buffer.
    fn sid(&self) -> PSID {
        // SAFETY: `read` succeeded, so the buffer holds an aligned TOKEN_USER.
        unsafe { (*self.0.as_ptr().cast::<TOKEN_USER>()).User.Sid }
    }

    fn text(&self) -> Result<String, Os> {
        // SAFETY: the SID points into this live buffer, written by GetTokenInformation.
        unsafe { sid_text(self.sid()) }
    }
}

/// The SID of the account this process runs as.
pub(crate) fn process_account() -> Result<String, Os> {
    let mut handle = ptr::null_mut();
    // SAFETY: the pseudo-handle needs no closing; the out pointer is a valid local.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &raw mut handle) } == 0 {
        return Err(os("OpenProcessToken"));
    }
    User::read(&Token(handle))?.text()
}

/// The SID and kind of the account `name`, or None when no account has that name.
pub(crate) fn account_sid(name: &str) -> Result<Option<(String, SID_NAME_USE)>, Os> {
    let name = wide(name)?;
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
        return unsafe { sid_text(sid.as_mut_ptr().cast()) }.map(|sid| Some((sid, kind)));
    }
    // SAFETY: reads this thread's last error immediately after the failed call.
    if unsafe { GetLastError() } == ERROR_NONE_MAPPED {
        Ok(None)
    } else {
        Err(os("LookupAccountNameW"))
    }
}

/// Whose access the sign-in pipe's list grants beyond [`PIPE_SDDL`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Dacl {
    /// Exactly [`PIPE_SDDL`].
    Production,
    /// Also this process's account: test runners are often services, whose
    /// tokens lack the INTERACTIVE group.
    #[cfg(test)]
    AlsoProcessAccount,
}

/// Creates the only instance the pipe `name` may ever have: first instance,
/// one instance at most, remote clients refused, with the access list `dacl`.
/// # Errors
/// [`Error::Taken`] when the name is held already; otherwise the failed call.
pub(crate) fn create_pipe(name: &str, dacl: Dacl) -> Result<NamedPipeServer, Error> {
    let sddl = match dacl {
        Dacl::Production => PIPE_SDDL.to_owned(),
        #[cfg(test)]
        Dacl::AlsoProcessAccount => format!("{PIPE_SDDL}(A;;GRGW;;;{})", process_account()?),
    };
    let sddl = wide(sddl)?;
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
        return Err(os("ConvertStringSecurityDescriptorToSecurityDescriptorW").into());
    }
    let mut attributes = SECURITY_ATTRIBUTES {
        nLength: u32::try_from(size_of::<SECURITY_ATTRIBUTES>()).unwrap_or(u32::MAX),
        lpSecurityDescriptor: descriptor.0,
        bInheritHandle: 0,
    };
    // SAFETY: `attributes` is a valid SECURITY_ATTRIBUTES whose descriptor lives in
    // `descriptor` until after the call; the handle is not inheritable.
    let created = unsafe {
        ServerOptions::new()
            .first_pipe_instance(true)
            .max_instances(1)
            .reject_remote_clients(true)
            .create_with_security_attributes_raw(name, (&raw mut attributes).cast())
    };
    created.map_err(|source| {
        let taken = [ERROR_ACCESS_DENIED, ERROR_PIPE_BUSY]
            .iter()
            .any(|&code| i32::try_from(code).ok() == source.raw_os_error());
        if taken {
            Error::Taken(source)
        } else {
            Os {
                call: "CreateNamedPipeW",
                source,
            }
            .into()
        }
    })
}

/// What the caller's token says: the user's SID and each group's SID with its
/// attribute bits, as Windows wrote them.
pub(crate) struct Identity {
    /// The user's SID.
    pub(crate) user: String,
    /// Each group's SID and attributes (`SE_GROUP_*` bits).
    pub(crate) groups: Vec<(String, u32)>,
}

/// One identification in progress on its own thread: the identity comes first,
/// then the account name, which Windows may take longer to find.
pub(crate) struct Identifying {
    /// The token's user and groups, or why they could not be read.
    pub(crate) identity: tokio::sync::oneshot::Receiver<Result<Identity, Error>>,
    /// `DOMAIN\name` of the user, or why it was not found.
    pub(crate) name: tokio::sync::oneshot::Receiver<Result<String, Os>>,
    /// The thread; it has stopped impersonating before the name lookup starts.
    pub(crate) thread: std::thread::JoinHandle<()>,
}

/// Starts reading who is on the other end of `pipe`. The caller must already
/// have written to the pipe and the server read it: Windows impersonates the
/// sender of the last message read. The client's word is never used.
///
/// The work runs on a new thread of its own, never a runtime or pool thread:
/// it impersonates the client, opens the thread token as itself, reads the user
/// and groups, and reverts; only then does it look up the account name.
/// # Errors
/// The failed handle duplication or thread start.
pub(crate) fn identify(pipe: &NamedPipeServer) -> Result<Identifying, Os> {
    let handle = pipe.as_handle().try_clone_to_owned().map_err(|source| Os {
        call: "DuplicateHandle",
        source,
    })?;
    let (identity_sent, identity) = tokio::sync::oneshot::channel();
    let (name_sent, name) = tokio::sync::oneshot::channel();
    let thread = std::thread::Builder::new()
        .name("analyte-sign-in-caller".into())
        .spawn(move || {
            let read = impersonated(&handle);
            drop(handle);
            let (user, groups) = match read {
                Ok(read) => read,
                Err(error) => {
                    let _ = identity_sent.send(Err(error));
                    return;
                }
            };
            let identity = user
                .text()
                .map_err(Error::from)
                .map(|user| Identity { user, groups });
            let identified = identity.is_ok();
            if identity_sent.send(identity).is_ok() && identified {
                let _ = name_sent.send(account_name(&user));
            }
        })
        .map_err(|source| Os {
            call: "thread start",
            source,
        })?;
    Ok(Identifying {
        identity,
        name,
        thread,
    })
}

/// Impersonates the pipe's client, reads its token and reverts. Returns no
/// result until the revert has succeeded.
fn impersonated(pipe: &OwnedHandle) -> Result<(User, Vec<(String, u32)>), Error> {
    // SAFETY: `pipe` is an open handle to the server end, owned for the whole
    // call, and the server has read the client's request.
    if unsafe { ImpersonateNamedPipeClient(pipe.as_raw_handle()) } == 0 {
        // On failure Windows documents that the thread does not impersonate.
        return Err(os("ImpersonateNamedPipeClient").into());
    }
    let read = thread_token().and_then(|token| Ok((User::read(&token)?, groups(&token)?)));
    // SAFETY: no preconditions; it ends this thread's impersonation.
    if unsafe { RevertToSelf() } == 0 {
        return Err(Error::Revert(io::Error::last_os_error()));
    }
    read
}

/// The impersonated thread's token, opened with the process's own rights:
/// an identification-level token cannot open objects for itself.
fn thread_token() -> Result<Token, Error> {
    let mut handle = ptr::null_mut();
    // SAFETY: the pseudo-handle needs no closing; the out pointer is a valid local.
    if unsafe { OpenThreadToken(GetCurrentThread(), TOKEN_QUERY, 1, &raw mut handle) } == 0 {
        return Err(os("OpenThreadToken").into());
    }
    Ok(Token(handle))
}

/// Each group of `token`: its SID text and attribute bits.
fn groups(token: &Token) -> Result<Vec<(String, u32)>, Error> {
    let mut needed = 0;
    // SAFETY: a null buffer of length zero only asks for the size needed.
    unsafe { GetTokenInformation(token.0, TokenGroups, ptr::null_mut(), 0, &raw mut needed) };
    if needed == 0 {
        return Err(os("GetTokenInformation").into());
    }
    if needed > GROUPS_LIMIT {
        return Err(Error::TooManyGroups(needed));
    }
    // u64 storage aligns TOKEN_GROUPS; the length is bounded above.
    let mut buffer = vec![0u64; usize::try_from(needed.div_ceil(8)).unwrap_or(0)];
    let length = u32::try_from(buffer.len().saturating_mul(8)).unwrap_or(0);
    let mut used = 0;
    // SAFETY: the buffer is writable for `length` bytes and outlives the call.
    let ok = unsafe {
        GetTokenInformation(
            token.0,
            TokenGroups,
            buffer.as_mut_ptr().cast(),
            length,
            &raw mut used,
        )
    };
    if ok == 0 {
        return Err(os("GetTokenInformation").into());
    }
    let start = buffer.as_ptr().cast::<u8>();
    // SAFETY: success wrote a TOKEN_GROUPS at the start of the buffer.
    let count = unsafe { start.cast::<u32>().read_unaligned() };
    let first = std::mem::offset_of!(TOKEN_GROUPS, Groups);
    let entry = size_of::<SID_AND_ATTRIBUTES>();
    let end = usize::try_from(count)
        .ok()
        .and_then(|count| count.checked_mul(entry))
        .and_then(|groups| groups.checked_add(first));
    let written = usize::try_from(used.min(length)).unwrap_or(0);
    if end.is_none_or(|end| end > written) {
        return Err(Error::MalformedGroups);
    }
    (0..usize::try_from(count).unwrap_or(0))
        .map(|index| {
            let offset = first.saturating_add(index.saturating_mul(entry));
            // SAFETY: `offset + entry` is within the written length checked above.
            let group = unsafe {
                start
                    .add(offset)
                    .cast::<SID_AND_ATTRIBUTES>()
                    .read_unaligned()
            };
            // SAFETY: Windows points each group's SID into the same live buffer.
            let sid = unsafe { sid_text(group.Sid) }?;
            Ok((sid, group.Attributes))
        })
        .collect()
}

/// `DOMAIN\name` for the user's SID; looked up as this process, after reverting.
fn account_name(user: &User) -> Result<String, Os> {
    let mut name = [0u16; NAME_LIMIT as usize];
    let mut domain = [0u16; NAME_LIMIT as usize];
    let (mut name_len, mut domain_len, mut kind) =
        (NAME_LIMIT, NAME_LIMIT, SID_NAME_USE::default());
    // SAFETY: the SID points into the live user buffer; every out buffer is
    // writable for the length passed and outlives the call.
    let ok = unsafe {
        LookupAccountSidW(
            ptr::null(),
            user.sid(),
            name.as_mut_ptr(),
            &raw mut name_len,
            domain.as_mut_ptr(),
            &raw mut domain_len,
            &raw mut kind,
        )
    };
    if ok == 0 {
        return Err(os("LookupAccountSidW"));
    }
    // On success each length counts the units written, without the terminator.
    let text = |units: &[u16], len: u32| {
        units
            .get(..usize::try_from(len).unwrap_or(usize::MAX))
            .map(String::from_utf16)
            .and_then(Result::ok)
    };
    match (text(&domain, domain_len), text(&name, name_len)) {
        (Some(domain), Some(name)) => Ok(format!("{domain}\\{name}")),
        _ => Err(Os {
            call: "account name text",
            source: io::ErrorKind::InvalidData.into(),
        }),
    }
}
