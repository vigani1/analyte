//! Distinguishes audited command failures from service failures.
/// Failures of the parked-evidence command, including its host input refusals.
#[derive(Debug, thiserror::Error)]
pub enum ParkedCommandError {
    /// Protected evidence lookup or mutation failed.
    #[error(transparent)]
    Evidence(#[from] analyte_store::OperatorError),
    /// Recording the operator audit failed.
    #[error(transparent)]
    Audit(#[from] analyte_store::StorageError),
    /// The store worker could not establish completion.
    #[error(transparent)]
    Worker(#[from] analyte_store::WorkerError),
    /// Reprocessing was sent to the wrong command route.
    #[error("reprocessing requires the interpretation route")]
    ReprocessRequiresInterpretation,
    /// The next operator cursor cannot be represented.
    #[error("operator sequence cursor overflow")]
    OperatorCursorOverflow,
}

/// Boundaries combined by the audited delivery listing command.
#[derive(Debug, thiserror::Error)]
pub enum DeliveryCommandError {
    /// The delivery store operation failed.
    #[error(transparent)]
    Delivery(#[from] analyte_store::DeliveryError),
    /// Recording the operator audit failed.
    #[error(transparent)]
    Audit(#[from] analyte_store::StorageError),
    /// The store worker could not establish completion.
    #[error(transparent)]
    Worker(#[from] analyte_store::WorkerError),
}
