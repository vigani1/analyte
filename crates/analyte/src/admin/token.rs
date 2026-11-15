use base64::{Engine as _, engine::general_purpose::STANDARD};
use std::{
    fs::OpenOptions,
    io::Write,
    path::Path,
};

/// Specific failures provisioning or loading the host's admin credential.
#[derive(Debug, thiserror::Error)]
pub enum TokenError {
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

    // Plan section 11: rotation installs a full random credential and replaces it.
    #[test]
    fn plan11_token_rotation_installs_256_bits() -> Result<(), crate::Error> {
        let directory = tempfile::tempdir()?;
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
        let directory = tempfile::tempdir()?;
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
