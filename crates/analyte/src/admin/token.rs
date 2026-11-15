use base64::{Engine as _, engine::general_purpose::STANDARD};
use std::{fs::OpenOptions, io::Write, path::Path};

/// Specific failures provisioning or loading the host's admin credential.
#[derive(Debug, thiserror::Error)]
pub enum TokenError {
    /// The configured admin endpoint has no provisioned credential.
    #[error("admin token file is missing; run analyte admin token --rotate")]
    Missing,
    /// The credential is not one canonical base64-encoded 256-bit value.
    #[error("admin token file must contain one base64-encoded 256-bit token")]
    Invalid,
    /// The credential file can be read by other users, or is not a regular file.
    #[error("admin token file must be a protected regular file (Unix mode 0600)")]
    Permissions,
    /// The OS entropy source failed; no replacement credential was installed.
    #[error("admin token OS random source failed: {0}")]
    Random(getrandom::Error),
    /// A credential operation failed; its OS cause is retained without secret bytes.
    #[error("admin token {operation} failed: {source}")]
    Io {
        /// Operation that failed, never credential contents.
        operation: &'static str,
        /// Original operating-system error.
        source: std::io::Error,
    },
}

/// One validated host credential, deliberately without a Debug implementation.
pub struct Token(String);

impl Token {
    pub(super) fn form_value(&self) -> &str {
        &self.0
    }
    pub(super) fn session() -> Result<Self, TokenError> {
        generate(getrandom::fill).map(Self)
    }

    pub(super) fn cookie(&self, https: bool) -> String {
        format!(
            "analyte_session={}; Path=/; HttpOnly; SameSite=Strict{}",
            self.0,
            if https { "; Secure" } else { "" }
        )
    }
    /// Reads at most 45 bytes and refuses missing, malformed or unprotected files.
    /// # Errors
    /// Reports missing credentials, invalid format, permissions and OS read failures.
    pub fn load(directory: &Path) -> Result<Self, TokenError> {
        use std::io::Read as _;
        let path = directory.join("admin-token");
        let metadata = std::fs::symlink_metadata(&path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                TokenError::Missing
            } else {
                io("inspect credential", e)
            }
        })?;
        if !metadata.is_file() {
            return Err(TokenError::Permissions);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o777 != 0o600 {
                return Err(TokenError::Permissions);
            }
        }
        let mut bytes = Vec::new();
        std::fs::File::open(path)
            .and_then(|file| file.take(45).read_to_end(&mut bytes))
            .map_err(|e| io("read credential", e))?;
        let text = String::from_utf8(bytes).map_err(|_| TokenError::Invalid)?;
        let decoded = STANDARD.decode(&text).map_err(|_| TokenError::Invalid)?;
        if text.len() != 44 || decoded.len() != 32 || STANDARD.encode(decoded) != text {
            return Err(TokenError::Invalid);
        }
        Ok(Self(text))
    }

    /// Compares a supplied bearer credential in constant time for equal lengths.
    /// Length is public: every valid credential contains exactly 44 base64 bytes.
    #[must_use]
    pub fn matches(&self, supplied: &str) -> bool {
        use subtle::ConstantTimeEq as _;
        bool::from(self.0.as_bytes().ct_eq(supplied.as_bytes()))
    }
}

fn io(operation: &'static str, source: std::io::Error) -> TokenError {
    TokenError::Io { operation, source }
}

fn generate(
    fill: impl FnOnce(&mut [u8]) -> Result<(), getrandom::Error>,
) -> Result<String, TokenError> {
    let mut bytes = [0; 32];
    fill(&mut bytes).map_err(TokenError::Random)?;
    Ok(STANDARD.encode(bytes))
}

/// Atomically replaces the admin token with 256 bits from the OS random source.
/// Returns its base64 representation for the CLI to print once, never to log.
/// Unix files are created mode 0600; Windows inherits the protected directory ACL.
/// Restart the service after rotation to invalidate its loaded credential.
/// A crash may leave `admin-token.pending`; preserve and investigate it before
/// retrying. A failed installation retains the previous token when present.
/// # Errors
/// Reports protected-directory, entropy, staging, flush or rename failures.
pub fn rotate_token(directory: &Path) -> Result<String, crate::Error> {
    crate::service::create_data_directory(directory)?;
    let token = generate(getrandom::fill)?;
    let pending = directory.join("admin-token.pending");
    let target = directory.join("admin-token");
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&pending)
        .map_err(|e| io("create protected staging file", e))?;
    // Never publish a partial credential. A failed staging file deliberately
    // blocks another rotation until the operator investigates the failure.
    file.write_all(token.as_bytes())
        .map_err(|e| io("write staging file", e))?;
    file.sync_all().map_err(|e| io("flush staging file", e))?;
    drop(file);
    std::fs::rename(&pending, &target).map_err(|e| io("install credential", e))?;
    #[cfg(unix)]
    std::fs::File::open(directory)
        .and_then(|file| file.sync_all())
        .map_err(|e| io("flush credential directory", e))?;
    Ok(token)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Plan section 11: missing or malformed credentials cannot enable a listener.
    #[test]
    fn plan11_token_loading_refuses_missing_and_malformed_files() -> Result<(), crate::Error> {
        let directory = crate::admin::private_tempdir()?;
        assert!(matches!(
            Token::load(directory.path()),
            Err(TokenError::Missing)
        ));
        let token = rotate_token(directory.path())?;
        let loaded = Token::load(directory.path())?;
        assert!(loaded.matches(&token));
        assert!(!loaded.matches("wrong"));
        std::fs::write(directory.path().join("admin-token"), b"invalid")?;
        assert!(matches!(
            Token::load(directory.path()),
            Err(TokenError::Invalid)
        ));
        Ok(())
    }

    // Plan section 11: rotation installs a full random credential and replaces it.
    #[test]
    fn plan11_token_rotation_installs_256_bits() -> Result<(), crate::Error> {
        let directory = crate::admin::private_tempdir()?;
        let first = rotate_token(directory.path())?;
        assert_eq!(STANDARD.decode(&first)?.len(), 32);
        assert_eq!(
            std::fs::read_to_string(directory.path().join("admin-token"))?,
            first
        );
        let second = rotate_token(directory.path())?;
        assert_ne!(first, second);
        assert_eq!(
            std::fs::read_to_string(directory.path().join("admin-token"))?,
            second
        );
        assert!(!directory.path().join("admin-token.pending").exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(directory.path().join("admin-token"))?
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        Ok(())
    }

    // Plan section 12: an unavailable OS random source must never cause fallback.
    #[test]
    fn plan12_entropy_failure_is_specific_without_fallback() {
        let result = generate(|_| Err(getrandom::Error::UNSUPPORTED));
        assert!(matches!(result, Err(TokenError::Random(_))));
    }

    // Plan section 12: interrupted provisioning cannot overwrite retained evidence.
    #[test]
    fn plan12_pending_rotation_preserves_old_token() -> Result<(), crate::Error> {
        let directory = crate::admin::private_tempdir()?;
        let token = rotate_token(directory.path())?;
        std::fs::write(directory.path().join("admin-token.pending"), b"incomplete")?;
        assert!(rotate_token(directory.path()).is_err());
        assert_eq!(
            std::fs::read_to_string(directory.path().join("admin-token"))?,
            token
        );
        Ok(())
    }
}
