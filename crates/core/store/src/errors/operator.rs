//! Classifies operator queries, parked resolutions, alerts and notification custody failures.
/// Failures of protected operator operations and their audit evidence.
#[derive(Debug, thiserror::Error)]
pub enum OperatorError {
    /// SQLite corruption retains the original cause from shared primitives.
    #[error("stored evidence is corrupt: {0}")]
    StorageCorruption(#[source] rusqlite::Error),
    /// The configured storage reserve was reached.
    #[error("storage capacity reserve reached")]
    Capacity,
    /// SQLite or stored evidence failed an integrity check.
    #[error("stored evidence is corrupt")]
    Corruption,
    /// SQLite reported a full disk.
    #[error("disk full")]
    DiskFull(#[source] rusqlite::Error),
    /// A configured input bound was exceeded before storage.
    #[error("storage input exceeds {what}")]
    LimitExceeded {
        /// Configured bound that refused admission.
        what: &'static str,
    },
    /// SQLite refused execution because another connection holds a lock.
    /// This failure did not commit; retry with the original identity.
    #[error("SQLite is busy: {0}")]
    Busy(#[source] rusqlite::Error),
    /// SQLite refused a constraint; this is not an application identity conflict.
    #[error("SQLite constraint violation: {0}")]
    ConstraintViolation(#[source] rusqlite::Error),
    /// Bounded model text could not be constructed; the exact reason is retained.
    #[error("invalid bounded storage text: {0}")]
    Text(#[source] analyte_model::TextError),
    /// A filesystem operation failed.
    #[error("filesystem operation failed")]
    Io(#[from] std::io::Error),
    /// SQLite failed; its original cause is preserved.
    #[error("SQLite operation failed: {0}")]
    Sqlite(#[source] rusqlite::Error),
    /// Typed evidence could not be encoded or decoded.
    #[error("evidence encoding failed: {0}")]
    Json(#[from] serde_json::Error),
    /// Parked item does not exist, is already resolved or belongs to other evidence.
    #[error("parked item is missing, resolved or unrelated to this receipt")]
    ReprocessParked,
    /// Purged evidence can no longer be replayed.
    #[error("item was purged by retention; reprocessing and resending are unavailable")]
    Purged,
    /// An immutable identity or state transition conflicted.
    #[error("identity or state constraint conflict")]
    Conflict,
    /// Operator or notification input cannot authorize the requested operation.
    #[error("invalid operator input")]
    InvalidOperatorInput,
    /// An operator action or target does not match the operation being authorized.
    #[error("audit action or target mismatch")]
    AuditMismatch,
    /// A linked poll could not be reconciled with the operator resolution.
    #[error(transparent)]
    PollResolution(#[from] crate::OrderError),
}
impl From<rusqlite::Error> for OperatorError {
    fn from(error: rusqlite::Error) -> Self {
        crate::StorageError::from(error).into()
    }
}
impl From<crate::StorageError> for OperatorError {
    fn from(error: crate::StorageError) -> Self {
        match error {
            crate::StorageError::Corruption(cause) => Self::StorageCorruption(cause),
            crate::StorageError::Capacity => Self::Capacity,
            crate::StorageError::DiskFull(cause) => Self::DiskFull(cause),
            crate::StorageError::LimitExceeded { what } => Self::LimitExceeded { what },
            crate::StorageError::Busy(cause) => Self::Busy(cause),
            crate::StorageError::ConstraintViolation(cause) => Self::ConstraintViolation(cause),
            crate::StorageError::Text(cause) => Self::Text(cause),
            crate::StorageError::Io(cause) => Self::Io(cause),
            crate::StorageError::Sqlite(cause) => Self::Sqlite(cause),
            crate::StorageError::Json(cause) => Self::Json(cause),
        }
    }
}
impl From<OperatorError> for crate::Error {
    fn from(error: OperatorError) -> Self {
        match error {
            OperatorError::PollResolution(cause) => cause.into(),
            OperatorError::StorageCorruption(cause) => Self::StorageCorruption(cause),
            OperatorError::Capacity => Self::Capacity,
            OperatorError::Corruption => Self::Corruption,
            OperatorError::DiskFull(cause) => Self::DiskFull(cause),
            OperatorError::LimitExceeded { what } => Self::LimitExceeded { what },
            OperatorError::Busy(cause) => Self::Busy(cause),
            OperatorError::ConstraintViolation(cause) => Self::ConstraintViolation(cause),
            OperatorError::Text(cause) => Self::Text(cause),
            OperatorError::Io(cause) => Self::Io(cause),
            OperatorError::Sqlite(cause) => Self::Sqlite(cause),
            OperatorError::Json(cause) => Self::Json(cause),
            OperatorError::ReprocessParked => Self::ReprocessParked,
            OperatorError::Purged => Self::Purged,
            OperatorError::Conflict => Self::Conflict,
            OperatorError::InvalidOperatorInput => Self::InvalidOperatorInput,
            OperatorError::AuditMismatch => Self::AuditMismatch,
        }
    }
}
