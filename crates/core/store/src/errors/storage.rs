//! Preserves capacity, encoding, filesystem and SQLite failures shared by store boundaries.
/// Failures of bounded persistence primitives, independent of application operations.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    /// The worker could not establish completion of the persistence operation.
    #[error(transparent)]
    Worker(#[from] crate::WorkerError),
    /// The host reserve or database size limit refused admission.
    #[error("storage capacity reserve reached")]
    Capacity,
    /// A configured record bound refused admission.
    #[error("storage input exceeds {what}")]
    LimitExceeded {
        /// Name of the configured bound.
        what: &'static str,
    },
    /// SQLite refused a lock; this execution did not commit.
    #[error("SQLite is busy: {0}")]
    Busy(#[source] rusqlite::Error),
    /// SQLite detected damaged evidence; its diagnostic cause is retained.
    #[error("stored evidence is corrupt: {0}")]
    Corruption(#[source] rusqlite::Error),
    /// SQLite refused a constraint rather than an application identity rule.
    #[error("SQLite constraint violation: {0}")]
    ConstraintViolation(#[source] rusqlite::Error),
    /// SQLite reported a full disk.
    #[error("disk full")]
    DiskFull(#[source] rusqlite::Error),
    /// Other SQLite failure with its original cause.
    #[error("SQLite operation failed: {0}")]
    Sqlite(#[source] rusqlite::Error),
    /// The filesystem probe failed; admission is refused.
    #[error("filesystem operation failed")]
    Io(#[from] std::io::Error),
    /// Protected evidence could not be encoded.
    #[error("evidence encoding failed: {0}")]
    Json(#[from] serde_json::Error),
    /// Protected evidence could not fit its validated model type.
    #[error("invalid bounded storage text: {0}")]
    Text(#[source] analyte_model::TextError),
}
impl From<rusqlite::Error> for StorageError {
    fn from(error: rusqlite::Error) -> Self {
        if let rusqlite::Error::SqliteFailure(code, _) = &error {
            #[expect(
                clippy::wildcard_enum_match_arm,
                reason = "External SQLite codes retain their cause when no recovery classification is known"
            )]
            match code.code {
                rusqlite::ErrorCode::DiskFull => return Self::DiskFull(error),
                rusqlite::ErrorCode::DatabaseCorrupt | rusqlite::ErrorCode::NotADatabase => {
                    return Self::Corruption(error);
                }
                rusqlite::ErrorCode::ConstraintViolation => {
                    return Self::ConstraintViolation(error);
                }
                rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked => {
                    return Self::Busy(error);
                }
                _ => {}
            }
        }
        Self::Sqlite(error)
    }
}
// Compatibility for boundaries still being migrated. No cause is discarded.
impl From<StorageError> for crate::Error {
    fn from(error: StorageError) -> Self {
        match error {
            StorageError::Worker(cause) => Self::Worker(cause),
            StorageError::Capacity => Self::Capacity,
            StorageError::LimitExceeded { what } => Self::LimitExceeded { what },
            StorageError::Busy(cause) => Self::Busy(cause),
            StorageError::Corruption(cause) => Self::StorageCorruption(cause),
            StorageError::ConstraintViolation(cause) => Self::ConstraintViolation(cause),
            StorageError::DiskFull(cause) => Self::DiskFull(cause),
            StorageError::Sqlite(cause) => Self::Sqlite(cause),
            StorageError::Io(cause) => Self::Io(cause),
            StorageError::Json(cause) => Self::Json(cause),
            StorageError::Text(cause) => Self::Text(cause),
        }
    }
}
