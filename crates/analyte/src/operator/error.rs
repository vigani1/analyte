//! Distinguishes operator refusal, audit, storage and runtime dependency failures.
/// Failures running a bounded operator action or reading an operational snapshot.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Order command combined restore and order storage operations.
    #[error(transparent)]
    OrderCommand(#[from] crate::orders::CommandError),
    /// The operator action or snapshot cannot proceed.
    #[error("active alert snapshot exceeds 4096 rows")]
    ActiveAlertSnapshotExceeds4096Rows,
    /// The operator action or snapshot cannot proceed.
    #[error("alert retry endpoint is not configured")]
    AlertRetryEndpointIsNotConfigured,
    /// The operator action or snapshot cannot proceed.
    #[error("receipt machine is absent from current configuration")]
    ReceiptMachineIsAbsentFromCurrentConfiguration,
    /// The operator action or snapshot cannot proceed.
    #[error("receipt page lost cursor")]
    ReceiptPageLostCursor,
    /// Preserves the original operator dependency failure.
    #[error(transparent)]
    Runtime(#[from] crate::runtime::Error),
    /// Preserves the original operator dependency failure.
    #[error(transparent)]
    Store(#[from] analyte_store::Error),
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
}
#[cfg(all(test, feature = "test-hooks"))]
impl Error {
    pub(super) fn downcast_ref<T: std::error::Error + 'static>(&self) -> Option<&T> {
        if let Self::Store(cause) = self {
            let cause: &(dyn std::error::Error + 'static) = cause;
            return cause.downcast_ref();
        }
        None
    }
}

analyte_store::forward_command_errors!(Error);
