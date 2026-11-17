//! Classifies cached orders, machine reservations, lifecycle changes and poll custody failures.
/// Failures of order storage and host event construction; retries never authorize another send.
#[derive(Debug, thiserror::Error)]
pub enum OrderError {
    /// A host event builder supplied an invalid delivery identity.
    #[error(transparent)]
    Identity(#[from] analyte_model::InvalidUuid),
    /// The host invoked a single-use rendering callback more than once.
    #[error("order preflight callback already consumed")]
    PreflightConsumed,
    /// Reservation completed without the required rendered command.
    #[error("order preflight output missing")]
    PreflightMissing,

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
    /// Machine-order input violates its declared ownership or item structure.
    #[error("invalid machine-order input")]
    InvalidOrderInput,
    /// Purged evidence can no longer be replayed.
    #[error("item was purged by retention; reprocessing and resending are unavailable")]
    Purged,
    /// An operator action or target does not match the operation being authorized.
    #[error("audit action or target mismatch")]
    AuditMismatch,
    /// Analyzer cancellation requires visible reconciliation before any new cancel admission.
    #[error("AWOS is analyzer-cancelled; app cancellation intent requires reconciliation")]
    AnalyzerCancelled,
    /// The model rejected a machine-order lifecycle transition.
    #[error("machine-order lifecycle transition rejected: {0}")]
    OrderTransition(#[from] analyte_model::TransitionError),
    /// A previous poll may already have moved this machine's cursor.
    #[error("poll intent remains unresolved; reconcile before polling again")]
    PollIntentUnresolved,
    /// No retained poll has the requested identity.
    #[error("poll intent does not exist")]
    UnknownPollIntent,
    /// The generated local command identity exceeds the caller's byte budget.
    #[error("command identity exceeds caller byte limit")]
    ItemIdentityTooLong,
    /// A finite policy requires the original host credential.
    #[error("retention digest key is required")]
    RetentionKeyRequired,
    /// The host's transactional delivery builder refused construction; the cause is retained.
    #[error("delivery construction failed")]
    DeliveryBuild(#[source] Box<dyn std::error::Error + Send + Sync>),
    /// The keyed retention digest could not be constructed.
    #[error("retention digest key rejected")]
    DigestKey(#[from] hmac::digest::InvalidLength),
}
impl From<rusqlite::Error> for OrderError {
    fn from(error: rusqlite::Error) -> Self {
        crate::StorageError::from(error).into()
    }
}
impl From<crate::StorageError> for OrderError {
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
impl From<OrderError> for crate::Error {
    fn from(error: OrderError) -> Self {
        match error {
            OrderError::Identity(cause) => Self::DeliveryBuild(Box::new(cause)),
            OrderError::PreflightConsumed => Self::PreflightConsumed,
            OrderError::PreflightMissing => Self::PreflightMissing,
            OrderError::StorageCorruption(cause) => Self::StorageCorruption(cause),
            OrderError::Capacity => Self::Capacity,
            OrderError::Corruption => Self::Corruption,
            OrderError::DiskFull(cause) => Self::DiskFull(cause),
            OrderError::LimitExceeded { what } => Self::LimitExceeded { what },
            OrderError::Busy(cause) => Self::Busy(cause),
            OrderError::ConstraintViolation(cause) => Self::ConstraintViolation(cause),
            OrderError::Text(cause) => Self::Text(cause),
            OrderError::Io(cause) => Self::Io(cause),
            OrderError::Sqlite(cause) => Self::Sqlite(cause),
            OrderError::Json(cause) => Self::Json(cause),
            OrderError::Conflict => Self::Conflict,
            OrderError::InvalidOrderInput => Self::InvalidOrderInput,
            OrderError::Purged => Self::Purged,
            OrderError::AuditMismatch => Self::AuditMismatch,
            OrderError::AnalyzerCancelled => Self::AnalyzerCancelled,
            OrderError::OrderTransition(cause) => Self::OrderTransition(cause),
            OrderError::PollIntentUnresolved => Self::PollIntentUnresolved,
            OrderError::UnknownPollIntent => Self::UnknownPollIntent,
            OrderError::ItemIdentityTooLong => Self::ItemIdentityTooLong,
            OrderError::RetentionKeyRequired => Self::RetentionKeyRequired,
            OrderError::DeliveryBuild(cause) => Self::DeliveryBuild(cause),
            OrderError::DigestKey(cause) => Self::DigestKey(cause),
        }
    }
}
