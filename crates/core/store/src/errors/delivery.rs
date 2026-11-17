//! Classifies app delivery claims, attempts, listings and audited resend failures.
/// Failures of durable app delivery operations; never authorize resending machine orders.
#[derive(Debug, thiserror::Error)]
pub enum DeliveryError {
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
    /// An immutable identity or state transition conflicted.
    #[error("identity or state constraint conflict")]
    Conflict,
    /// Delivery construction or attempt evidence is inconsistent.
    #[error("invalid delivery input")]
    InvalidDeliveryInput,
    /// Purged evidence can no longer be replayed.
    #[error("item was purged by retention; reprocessing and resending are unavailable")]
    Purged,
    /// An operator action or target does not match the operation being authorized.
    #[error("audit action or target mismatch")]
    AuditMismatch,
    /// No delivery exists with the requested identity.
    #[error("delivery does not exist")]
    UnknownDelivery,
    /// Only completed, parked or setup-paused deliveries can be resent.
    #[error("delivery state does not permit resend")]
    ResendState,
    /// A newer observation revision exists; explicitly name the old delivery to resend it.
    #[error("delivery is superseded; explicit operator acknowledgement required")]
    SupersededDelivery,
}
impl From<rusqlite::Error> for DeliveryError {
    fn from(error: rusqlite::Error) -> Self {
        crate::StorageError::from(error).into()
    }
}
impl From<crate::StorageError> for DeliveryError {
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
impl From<DeliveryError> for crate::Error {
    fn from(error: DeliveryError) -> Self {
        match error {
            DeliveryError::StorageCorruption(cause) => Self::StorageCorruption(cause),
            DeliveryError::Capacity => Self::Capacity,
            DeliveryError::Corruption => Self::Corruption,
            DeliveryError::DiskFull(cause) => Self::DiskFull(cause),
            DeliveryError::LimitExceeded { what } => Self::LimitExceeded { what },
            DeliveryError::Busy(cause) => Self::Busy(cause),
            DeliveryError::ConstraintViolation(cause) => Self::ConstraintViolation(cause),
            DeliveryError::Text(cause) => Self::Text(cause),
            DeliveryError::Io(cause) => Self::Io(cause),
            DeliveryError::Sqlite(cause) => Self::Sqlite(cause),
            DeliveryError::Json(cause) => Self::Json(cause),
            DeliveryError::Conflict => Self::Conflict,
            DeliveryError::InvalidDeliveryInput => Self::InvalidDeliveryInput,
            DeliveryError::Purged => Self::Purged,
            DeliveryError::AuditMismatch => Self::AuditMismatch,
            DeliveryError::UnknownDelivery => Self::UnknownDelivery,
            DeliveryError::ResendState => Self::ResendState,
            DeliveryError::SupersededDelivery => Self::SupersededDelivery,
        }
    }
}
