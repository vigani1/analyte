//! Keeps destination keys in a locked file that only the service account can read.
//!
//! The file is `<data_directory>/credentials/credentials.toml`. Its folder grants
//! only SYSTEM and the service account on Windows (ADR 0017) and is mode 0700 on
//! Unix; the file is mode 0600 and owned by the service user. The service writes
//! it itself: a staging file in the same folder, protected, flushed, then renamed
//! over the old one. Values never appear in errors, logs or `Debug` output.
use analyte_model::Identifier;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
};

/// Largest accepted credentials file.
const FILE_LIMIT: u64 = 65_536;
/// Most keys one file holds.
const KEY_LIMIT: usize = 32;
/// Longest accepted key value, matching the API key bound.
const VALUE_LIMIT: usize = 4096;

/// Why the locked credentials file could not be read or written; never holds a value.
#[derive(Debug, thiserror::Error)]
pub enum CredentialError {
    /// Other users could read the file or folder, or it is not owned by this account.
    #[error(
        "credentials {0} must be owned by the service account and readable by no one else (folder mode 0700, file mode 0600)"
    )]
    Exposed(PathBuf),
    /// The Windows access list grants someone other than SYSTEM and the service account.
    #[cfg(windows)]
    #[error("credentials {path}: {source}")]
    Access {
        /// Refused folder or file.
        path: PathBuf,
        /// The specific access-list refusal, naming the SID.
        source: crate::runtime::service::acl::Error,
    },
    /// The credentials path is a link, folder or device rather than a regular file.
    #[error("credentials file {0} is not a regular file")]
    NotRegularFile(PathBuf),
    /// The file exceeds the size or key-count bound.
    #[error("credentials file exceeds 64 KiB or 32 keys")]
    TooLarge,
    /// The file is not a version 1 credentials file; its text is never repeated.
    #[error(
        "credentials file is not a valid version 1 credentials file; replace the keys on the setup page"
    )]
    Malformed,
    /// A supplied value is empty, too long or not printable ASCII.
    #[error("credential {0} must be 1 to 4096 printable ASCII characters")]
    InvalidValue(String),
    /// A file operation failed; the OS cause is kept, the value is not.
    #[error("credentials {operation} failed: {source}")]
    Io {
        /// What was being done.
        operation: &'static str,
        /// Original operating-system error.
        source: std::io::Error,
    },
}

fn io(operation: &'static str) -> impl FnOnce(std::io::Error) -> CredentialError {
    move |source| CredentialError::Io { operation, source }
}

/// A key value checked once at the boundary; deliberately has no `Debug` or `Display`.
pub struct Secret(String);

impl Secret {
    /// Accepts 1 to 4096 printable ASCII characters; the error names only `name`.
    /// # Errors
    /// Refuses empty, overlong or non-printable values.
    pub fn new(name: &Identifier, value: String) -> Result<Self, CredentialError> {
        if value.is_empty()
            || value.len() > VALUE_LIMIT
            || !value.bytes().all(|b| (32..=126).contains(&b))
        {
            return Err(CredentialError::InvalidValue(name.as_str().into()));
        }
        Ok(Self(value))
    }

    /// Borrows the value for validation by its owner.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

#[derive(Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct Document {
    schema_version: u32,
    #[serde(default)]
    keys: BTreeMap<String, Entry>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    value: String,
    /// Unix second the value was stored.
    set_at: i64,
}

/// The keys read from the locked file. Deliberately has no `Debug`.
#[derive(Default)]
pub struct Stored(BTreeMap<String, Entry>);

impl Stored {
    /// The value stored under exactly the credential name `name`.
    #[must_use]
    pub fn value(&self, name: &str) -> Option<&str> {
        self.0.get(name).map(|entry| entry.value.as_str())
    }

    /// When the named credential was stored, as a Unix second; never its value.
    #[must_use]
    pub fn set_at(&self, name: &str) -> Option<i64> {
        self.0.get(name).map(|entry| entry.set_at)
    }
}

/// The locked credentials folder of one data directory.
pub struct Locked {
    folder: PathBuf,
}

impl Locked {
    /// Names `<data_directory>/credentials`; nothing is touched yet.
    #[must_use]
    pub fn new(data_directory: &Path) -> Self {
        Self {
            folder: data_directory.join("credentials"),
        }
    }

    fn file(&self) -> PathBuf {
        self.folder.join("credentials.toml")
    }

    /// Reads the stored keys; a missing folder or file holds none.
    /// # Errors
    /// Refuses an exposed, oversized or malformed file, and reports I/O failures.
    pub fn read(&self) -> Result<Stored, CredentialError> {
        match std::fs::symlink_metadata(&self.folder) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Stored::default());
            }
            Err(error) => return Err(io("inspect folder")(error)),
            Ok(metadata) if !metadata.is_dir() => {
                return Err(CredentialError::NotRegularFile(self.folder.clone()));
            }
            Ok(_) => guard(&self.folder, true)?,
        }
        let path = self.file();
        let metadata = match std::fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Stored::default());
            }
            other => other.map_err(io("inspect file"))?,
        };
        if !metadata.is_file() {
            return Err(CredentialError::NotRegularFile(path));
        }
        guard(&path, false)?;
        let mut bytes = Vec::new();
        std::fs::File::open(&path)
            .and_then(|file| file.take(FILE_LIMIT + 1).read_to_end(&mut bytes))
            .map_err(io("read file"))?;
        if bytes.len() as u64 > FILE_LIMIT {
            return Err(CredentialError::TooLarge);
        }
        let text = String::from_utf8(bytes).map_err(|_| CredentialError::Malformed)?;
        // The parser's message quotes the offending line, which may hold a key.
        let document: Document = toml::from_str(&text).map_err(|_| CredentialError::Malformed)?;
        if document.schema_version != 1 {
            return Err(CredentialError::Malformed);
        }
        if document.keys.len() > KEY_LIMIT {
            return Err(CredentialError::TooLarge);
        }
        Ok(Stored(document.keys))
    }

    /// Stores `updates` beside the keys already held, dated `now`, replacing the
    /// file atomically. Creates the locked folder when it is missing.
    /// # Errors
    /// Refuses an exposed folder or file, too many keys, and reports I/O failures;
    /// the previous file stays in place on any failure.
    pub fn replace(
        &self,
        updates: Vec<(Identifier, Secret)>,
        now: i64,
    ) -> Result<(), CredentialError> {
        create_folder(&self.folder)?;
        let mut keys = self.read()?.0;
        for (name, Secret(value)) in updates {
            keys.insert(name.as_str().into(), Entry { value, set_at: now });
        }
        if keys.len() > KEY_LIMIT {
            return Err(CredentialError::TooLarge);
        }
        let text = toml::to_string(&Document {
            schema_version: 1,
            keys,
        })
        .map_err(|_| CredentialError::Malformed)?;
        let staging = self.folder.join("credentials.toml.pending");
        // Only this program writes the staging file; a leftover one is an earlier
        // interrupted write of the same kind, never evidence.
        match std::fs::remove_file(&staging) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                return Err(io("remove stale staging file")(error));
            }
            Ok(()) | Err(_) => {}
        }
        let written = stage(&staging, text.as_bytes())
            .and_then(|()| guard(&staging, false))
            .and_then(|()| std::fs::rename(&staging, self.file()).map_err(io("install file")));
        if written.is_err() {
            let _ = std::fs::remove_file(&staging);
        }
        written?;
        #[cfg(unix)]
        std::fs::File::open(&self.folder)
            .and_then(|folder| folder.sync_all())
            .map_err(io("flush folder"))?;
        Ok(())
    }
}

/// Creates a staging file only the owner can open, writes and flushes it.
fn stage(path: &Path, bytes: &[u8]) -> Result<(), CredentialError> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    // On Windows the file inherits the folder's SYSTEM-and-service-only list.
    let mut file = options.open(path).map_err(io("create staging file"))?;
    file.write_all(bytes).map_err(io("write staging file"))?;
    file.sync_all().map_err(io("flush staging file"))
}

/// Creates the locked folder when missing; an existing one is only checked.
fn create_folder(folder: &Path) -> Result<(), CredentialError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt as _;
        match std::fs::DirBuilder::new().mode(0o700).create(folder) {
            Err(error) if error.kind() != std::io::ErrorKind::AlreadyExists => {
                return Err(io("create folder")(error));
            }
            Ok(()) | Err(_) => {}
        }
    }
    #[cfg(windows)]
    crate::runtime::service::acl::create_private(
        folder,
        crate::runtime::service::acl::Readers::ServiceOnly,
    )
    .map_err(|source| CredentialError::Access {
        path: folder.to_owned(),
        source,
    })?;
    guard(folder, true)
}

/// Refuses a folder or file that anyone but the service account could read.
fn guard(path: &Path, folder: bool) -> Result<(), CredentialError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
        let metadata = std::fs::symlink_metadata(path).map_err(io("inspect protection"))?;
        // `/proc/self` belongs to the effective user of this process.
        let me = std::fs::metadata("/proc/self")
            .map_err(io("identify the service account"))?
            .uid();
        let mode = metadata.permissions().mode() & 0o777;
        if metadata.uid() != me || mode != if folder { 0o700 } else { 0o600 } {
            return Err(CredentialError::Exposed(path.to_owned()));
        }
    }
    #[cfg(windows)]
    {
        let _ = folder;
        crate::runtime::service::acl::check(
            path,
            crate::runtime::service::acl::Readers::ServiceOnly,
        )
        .map_err(|source| CredentialError::Access {
            path: path.to_owned(),
            source,
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{CredentialError, Locked, Secret};
    use analyte_model::Identifier;

    const KEY: &str = "sensitive-synthetic-api-key-7Q2";

    fn store(directory: &std::path::Path) -> Result<Locked, crate::Error> {
        let locked = Locked::new(directory);
        let name = Identifier::new("demo-api")?;
        let secret = Secret::new(&name, KEY.into())?;
        locked.replace(vec![(name, secret)], 1_763_424_000)?;
        Ok(locked)
    }

    // Owner decision 2026-11-18 (locked file): a stored key reads back with its date.
    #[test]
    fn stored_keys_read_back_with_their_date() -> Result<(), crate::Error> {
        let directory = crate::admin::private_tempdir()?;
        let locked = store(directory.path())?;
        let signing = Identifier::new("demo-signing")?;
        let secret = Secret::new(&signing, analyte_fixtures::SECRET.into())?;
        locked.replace(vec![(signing, secret)], 1_763_500_000)?;
        let stored = locked.read()?;
        assert_eq!(stored.value("demo-api"), Some(KEY));
        assert_eq!(stored.set_at("demo-api"), Some(1_763_424_000));
        assert_eq!(stored.set_at("demo-signing"), Some(1_763_500_000));
        assert!(
            !directory
                .path()
                .join("credentials/credentials.toml.pending")
                .exists()
        );
        Ok(())
    }

    // Owner decision 2026-11-18 (locked file): Unix folder 0700, file 0600.
    #[cfg(unix)]
    #[test]
    fn unix_file_is_owner_only_and_readable_copy_is_refused() -> Result<(), crate::Error> {
        use std::os::unix::fs::PermissionsExt as _;
        let directory = crate::admin::private_tempdir()?;
        let locked = store(directory.path())?;
        let folder = directory.path().join("credentials");
        let file = folder.join("credentials.toml");
        assert_eq!(
            std::fs::metadata(&folder)?.permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(&file)?.permissions().mode() & 0o777,
            0o600
        );
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644))?;
        let error = locked.read().err().ok_or("readable file accepted")?;
        assert!(matches!(error, CredentialError::Exposed(_)), "{error}");
        assert!(!error.to_string().contains(KEY));
        Ok(())
    }

    // Owner decision 2026-11-18 (locked file) / ADR 0017: only SYSTEM and the
    // service account; an Administrators or Users grant is refused by SID.
    #[cfg(windows)]
    #[test]
    fn windows_file_grants_only_system_and_service() -> Result<(), crate::Error> {
        use crate::runtime::service::acl::{
            Error,
            tests::{USERS, icacls},
        };
        for sid in ["S-1-5-32-544", USERS] {
            let directory = crate::admin::private_tempdir()?;
            let locked = store(directory.path())?;
            let file = directory.path().join("credentials/credentials.toml");
            icacls(&file, &["/grant", &format!("*{sid}:R")])?;
            match locked.read() {
                Err(CredentialError::Access {
                    source: Error::Grant { sid: refused },
                    ..
                }) => assert_eq!(refused, sid),
                Err(other) => return Err(format!("unexpected refusal: {other}").into()),
                Ok(_) => return Err(format!("grant to {sid} accepted").into()),
            }
        }
        Ok(())
    }

    // AGENTS.md rule 10: a malformed file never repeats its text in the error.
    #[test]
    fn malformed_file_error_never_repeats_a_key() -> Result<(), crate::Error> {
        let directory = crate::admin::private_tempdir()?;
        let locked = store(directory.path())?;
        let file = directory.path().join("credentials/credentials.toml");
        std::fs::write(&file, format!("schema_version = 1\n{KEY} = broken\n"))?;
        let error = locked.read().err().ok_or("malformed file accepted")?;
        assert!(matches!(error, CredentialError::Malformed), "{error}");
        assert!(!format!("{error} {error:?}").contains(KEY));
        let name = Identifier::new("demo-api")?;
        let invalid = Secret::new(&name, format!("{KEY}\n"))
            .err()
            .ok_or("accepted")?;
        assert!(!format!("{invalid} {invalid:?}").contains(KEY));
        Ok(())
    }
}
