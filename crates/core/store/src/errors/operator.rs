//! Classifies operator queries, parked resolutions, alerts and notification custody failures.
use crate::StorageError;
/// Failures of protected operator operations and their audit evidence.
#[derive(Debug, thiserror::Error)]
pub enum OperatorError {
    /// A persistence primitive failed; its recovery classification is retained.
    #[error(transparent)]
    Storage(#[from] StorageError),
    /// Retained evidence failed an application integrity check.
    #[error("stored evidence is corrupt")]
    Corruption,
    /// The worker could not establish completion of this operation.
    #[error("{0}")]
    Worker(#[from] crate::WorkerError),
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
        StorageError::from(error).into()
    }
}
impl From<std::io::Error> for OperatorError {
    fn from(error: std::io::Error) -> Self {
        StorageError::from(error).into()
    }
}
impl From<serde_json::Error> for OperatorError {
    fn from(error: serde_json::Error) -> Self {
        StorageError::from(error).into()
    }
}
