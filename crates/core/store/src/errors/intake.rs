//! Distinguishes receipt and transfer refusals from persistence failures.
use super::StorageError;
/// Failures admitting raw receipts and transfer frames before acknowledgment.
#[derive(Debug, thiserror::Error)]
pub enum IntakeError {
    /// The worker could not establish receipt custody completion.
    #[error(transparent)]
    Worker(#[from] crate::WorkerError),
    /// Bounded persistence failed; no positive acknowledgment is authorized.
    #[error(transparent)]
    Storage(#[from] StorageError),
    /// Receipt digests or protocol-supplied frame bounds do not agree.
    #[error("invalid receipt input")]
    InvalidReceiptInput,
    /// Transfer state or immutable provenance conflicts with retained evidence.
    #[error("identity or state constraint conflict")]
    Conflict,
}
impl From<rusqlite::Error> for IntakeError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Storage(error.into())
    }
}

// Compatibility for callers whose enclosing boundary has not yet been split.
impl From<IntakeError> for crate::Error {
    fn from(error: IntakeError) -> Self {
        match error {
            IntakeError::Worker(cause) => Self::Worker(cause),
            IntakeError::Storage(cause) => cause.into(),
            IntakeError::InvalidReceiptInput => Self::InvalidReceiptInput,
            IntakeError::Conflict => Self::Conflict,
        }
    }
}
