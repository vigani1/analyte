//! Classifies app delivery claims, attempts, listings and audited resend failures.
use crate::StorageError;
/// Failures of durable app delivery operations; never authorize resending machine orders.
#[derive(Debug, thiserror::Error)]
pub enum DeliveryError {
    /// A persistence primitive failed; its recovery classification is retained.
    #[error(transparent)]
    Storage(#[from] StorageError),
    /// Retained evidence failed an application integrity check.
    #[error("stored evidence is corrupt")]
    Corruption,
    /// The worker could not establish completion of this operation.
    #[error("{0}")]
    Worker(#[from] crate::WorkerError),
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
        StorageError::from(error).into()
    }
}
impl From<std::io::Error> for DeliveryError {
    fn from(error: std::io::Error) -> Self {
        StorageError::from(error).into()
    }
}
impl From<serde_json::Error> for DeliveryError {
    fn from(error: serde_json::Error) -> Self {
        StorageError::from(error).into()
    }
}
