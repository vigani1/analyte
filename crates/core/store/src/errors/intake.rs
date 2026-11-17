//! Distinguishes receipt and transfer refusals from persistence failures.
storage_boundary! {
    /// Failures admitting raw receipts and transfer frames before acknowledgment.
    #[derive(Debug, thiserror::Error)]
    pub enum IntakeError {
        /// Receipt digests or protocol-supplied frame bounds do not agree.
        #[error("invalid receipt input")]
        InvalidReceiptInput,
        /// Transfer state or immutable provenance conflicts with retained evidence.
        #[error("identity or state constraint conflict")]
        Conflict,
    }
}
impl From<IntakeError> for crate::Error {
    fn from(error: IntakeError) -> Self {
        match error {
            IntakeError::Worker(cause) => Self::Worker(cause),
            IntakeError::StorageCorruption(cause) => Self::StorageCorruption(cause),
            IntakeError::Capacity => Self::Capacity,
            IntakeError::Corruption => Self::Corruption,
            IntakeError::DiskFull(cause) => Self::DiskFull(cause),
            IntakeError::LimitExceeded { what } => Self::LimitExceeded { what },
            IntakeError::Busy(cause) => Self::Busy(cause),
            IntakeError::ConstraintViolation(cause) => Self::ConstraintViolation(cause),
            IntakeError::Text(cause) => Self::Text(cause),
            IntakeError::Io(cause) => Self::Io(cause),
            IntakeError::Sqlite(cause) => Self::Sqlite(cause),
            IntakeError::Json(cause) => Self::Json(cause),
            IntakeError::InvalidReceiptInput => Self::InvalidReceiptInput,
            IntakeError::Conflict => Self::Conflict,
        }
    }
}
