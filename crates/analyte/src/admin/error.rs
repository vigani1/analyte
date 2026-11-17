//! Distinguishes admin refusals, authentication, storage and operator-action failures.
/// Failures at the authenticated local administration boundary.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The host could not represent the storage request deadline.
    #[error(transparent)]
    Deadline(#[from] crate::DeadlineError),
    /// A blocking owner failed while loading the current installation.
    #[error(transparent)]
    Join(#[from] tokio::task::JoinError),
    /// The enabled departure declaration could not be rendered.
    #[error(transparent)]
    MachineConfig(#[from] analyte_config::Error),
    /// The requested administration operation is refused.
    #[error("AWOS item not retained")]
    AwosItemNotRetained,
    /// The requested administration operation is refused.
    #[error("admin session capacity reached; restart or wait for expiry")]
    AdminSessionCapacityReachedRestartOrWaitForExpiry,
    /// The requested administration operation is refused.
    #[error("admin session lock poisoned")]
    AdminSessionLockPoisoned,
    /// The requested administration operation is refused.
    #[error("choose place or release")]
    ChoosePlaceOrRelease,
    /// The requested administration operation is refused.
    #[error("choose receipt or order")]
    ChooseReceiptOrOrder,
    /// The requested administration operation is refused.
    #[error("current configuration points to a different store")]
    CurrentConfigurationPointsToADifferentStore,
    /// The requested administration operation is refused.
    #[error("machine cursor overflow")]
    MachineCursorOverflow,
    /// The requested administration operation is refused.
    #[error("parked target cursor overflow")]
    ParkedTargetCursorOverflow,
    /// The requested administration operation is refused.
    #[error("uncertain order count overflow")]
    UncertainOrderCountOverflow,
    /// Preserves the original admin dependency failure.
    #[error(transparent)]
    Operator(Box<crate::operator::Error>),
    /// Preserves the original admin dependency failure.
    #[error(transparent)]
    Runtime(#[from] crate::runtime::Error),
    /// Preserves the original admin dependency failure.
    #[error(transparent)]
    Store(#[from] analyte_store::Error),
    /// Preserves the original admin dependency failure.
    #[error(transparent)]
    Intake(#[from] crate::IntakeError),
    /// Preserves the original admin dependency failure.
    #[error(transparent)]
    Config(#[from] crate::installation::config::Error),
    /// Preserves the original admin dependency failure.
    #[error(transparent)]
    Text(#[from] analyte_model::TextError),
    /// Preserves the original admin dependency failure.
    #[error(transparent)]
    Identity(#[from] analyte_model::InvalidUuid),
    /// Preserves the original admin dependency failure.
    #[error(transparent)]
    Uuid(#[from] uuid::Error),
    /// Preserves the original admin dependency failure.
    #[error(transparent)]
    Integer(#[from] std::num::TryFromIntError),
    /// Preserves the original admin dependency failure.
    #[error(transparent)]
    ParseInteger(#[from] std::num::ParseIntError),
    /// Preserves the original admin dependency failure.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    /// Preserves the original admin dependency failure.
    #[error(transparent)]
    Token(#[from] super::TokenError),
    /// Preserves the original admin dependency failure.
    #[error(transparent)]
    SinkDestinationError(#[from] analyte_sink::DestinationError),
    /// Preserves the owning sink API boundary.
    #[error(transparent)]
    SinkDeliveryError(#[from] analyte_sink::DeliveryError),
    /// Preserves the owning sink API boundary.
    #[error(transparent)]
    SinkPollError(#[from] analyte_sink::PollError),
    /// Preserves the original admin dependency failure.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// Preserves the original admin dependency failure.
    #[error(transparent)]
    Secret(#[from] crate::installation::secrets::SecretError),
}

impl From<crate::operator::Error> for Error {
    fn from(error: crate::operator::Error) -> Self {
        Self::Operator(Box::new(error))
    }
}

analyte_store::forward_command_errors!(Error);
