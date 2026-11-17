//! Classifies cached orders, machine reservations, lifecycle changes and poll custody failures.
use crate::StorageError;
/// Failures of order storage and host event construction; retries never authorize another send.
#[derive(Debug, thiserror::Error)]
pub enum OrderError {
    /// A persistence primitive failed; its recovery classification is retained.
    #[error(transparent)]
    Storage(#[from] StorageError),
    /// Retained evidence failed an application integrity check.
    #[error("stored evidence is corrupt")]
    Corruption,
    /// The worker could not establish completion of this operation.
    #[error("{0}")]
    Worker(#[from] crate::WorkerError),
    /// A host event builder supplied an invalid delivery identity.
    #[error(transparent)]
    Identity(#[from] analyte_model::InvalidUuid),
    /// The host invoked a single-use rendering callback more than once.
    #[error("order preflight callback already consumed")]
    PreflightConsumed,
    /// Reservation completed without the required rendered command.
    #[error("order preflight output missing")]
    PreflightMissing,
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
        StorageError::from(error).into()
    }
}
impl From<std::io::Error> for OrderError {
    fn from(error: std::io::Error) -> Self {
        StorageError::from(error).into()
    }
}
impl From<serde_json::Error> for OrderError {
    fn from(error: serde_json::Error) -> Self {
        StorageError::from(error).into()
    }
}
