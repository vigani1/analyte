//! Distinguishes receipt and transfer refusals from persistence failures.
use crate::StorageError;
/// Failures admitting raw receipts and transfer frames before acknowledgment.
#[derive(Debug, thiserror::Error)]
pub enum IntakeError {
    /// A persistence primitive failed; its recovery classification is retained.
    #[error(transparent)]
    Storage(#[from] StorageError),
    /// Retained evidence failed an application integrity check.
    #[error("stored evidence is corrupt")]
    Corruption,
    /// The worker could not establish completion of this operation.
    #[error("{0}")]
    Worker(#[from] crate::WorkerError),
    /// Receipt digests or protocol-supplied frame bounds do not agree.
    #[error("invalid receipt input")]
    InvalidReceiptInput,
    /// Transfer state or immutable provenance conflicts with retained evidence.
    #[error("identity or state constraint conflict")]
    Conflict,
}

impl From<rusqlite::Error> for IntakeError {
    fn from(error: rusqlite::Error) -> Self {
        StorageError::from(error).into()
    }
}
impl From<std::io::Error> for IntakeError {
    fn from(error: std::io::Error) -> Self {
        StorageError::from(error).into()
    }
}
impl From<serde_json::Error> for IntakeError {
    fn from(error: serde_json::Error) -> Self {
        StorageError::from(error).into()
    }
}
