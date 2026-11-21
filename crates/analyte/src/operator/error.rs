//! Distinguishes operator refusal, audit, storage and runtime dependency failures.
/// Failures running a bounded operator action or reading an operational snapshot.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The host could not represent the storage request deadline.
    #[error(transparent)]
    Deadline(#[from] crate::DeadlineError),
    /// Order command combined restore and order storage operations.
    #[error(transparent)]
    OrderCommand(#[from] crate::orders::CommandError),
    /// The operator action or snapshot cannot proceed.
    #[error("active alert snapshot exceeds 4096 rows")]
    AlertSnapshotTooLarge,
    /// The operator action or snapshot cannot proceed.
    #[error("alert retry endpoint is not configured")]
    AlertRetryEndpointIsNotConfigured,
    /// The operator action or snapshot cannot proceed.
    #[error("receipt machine is absent from current configuration")]
    ReceiptMachineMissing,
    /// The operator action or snapshot cannot proceed.
    #[error("receipt page lost cursor")]
    ReceiptPageLostCursor,
    /// Preserves the original operator dependency failure.
    #[error(transparent)]
    Runtime(#[from] crate::runtime::Error),
    /// Preserves the original operator dependency failure.
    #[error(transparent)]
    Store(#[from] analyte_store::OperatorError),
    /// Reading restored-store state failed.
    #[error(transparent)]
    Storage(#[from] analyte_store::StorageError),
    /// Retention or restored-store reconciliation failed.
    #[error(transparent)]
    Maintenance(#[from] analyte_store::MaintenanceError),
    /// Analyzer execution evidence could not be queried.
    #[error(transparent)]
    Reconciliation(#[from] analyte_store::ReconciliationError),
    /// A delivery recovery operation failed.
    #[error(transparent)]
    Delivery(#[from] analyte_store::DeliveryError),
    /// Worker admission, completion or shutdown failed.
    #[error(transparent)]
    Worker(#[from] analyte_store::WorkerError),
    /// The selected retained receipt does not exist.
    #[error("receipt does not exist")]
    UnknownReceipt,
    /// Preserves the original operator dependency failure.
    #[error(transparent)]
    Intake(#[from] crate::IntakeError),
    /// Preserves the original operator dependency failure.
    #[error(transparent)]
    Config(#[from] crate::installation::config::Error),
    /// Preserves the original operator dependency failure.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// Preserves the original operator dependency failure.
    #[error(transparent)]
    Text(#[from] analyte_model::TextError),
    /// Preserves the original operator dependency failure.
    #[error(transparent)]
    Uuid(#[from] uuid::Error),
    /// Preserves the original operator dependency failure.
    #[error(transparent)]
    Identity(#[from] analyte_model::InvalidUuid),
    /// Preserves the original operator dependency failure.
    #[error(transparent)]
    Integer(#[from] std::num::TryFromIntError),
    /// Preserves the original operator dependency failure.
    #[error(transparent)]
    ParseInteger(#[from] std::num::ParseIntError),
    /// Preserves the original operator dependency failure.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    /// Preserves the original operator dependency failure.
    #[error(transparent)]
    SinkDestinationError(#[from] analyte_sink::DestinationError),
    /// Preserves the owning sink API boundary.
    #[error(transparent)]
    SinkDeliveryError(#[from] analyte_sink::DeliveryError),
    /// Preserves the owning sink API boundary.
    #[error(transparent)]
    SinkPollError(#[from] analyte_sink::PollError),
    /// A page read for the command line, such as Cancelled by machine, failed.
    #[error(transparent)]
    Page(Box<crate::admin::Error>),
}
