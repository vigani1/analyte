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
    /// The AWOS history page names an item index that the retained machine order does not
    /// hold, so there is no execution history to show for it.
    #[error("AWOS item not retained")]
    AwosItemNotRetained,
    /// A new admin session was refused because all 128 slots hold authenticated sessions
    /// that have not yet expired; unauthenticated ones are evicted first.
    #[error("admin session capacity reached; restart or wait for expiry")]
    SessionCapacity,
    /// A thread panicked while holding the in-memory session table, so no session can be
    /// looked up or issued until the program restarts.
    #[error("admin session lock poisoned")]
    AdminSessionLockPoisoned,
    /// The setup page asked for a restart; the server drained and stopped so the
    /// service manager starts the program again with the saved settings.
    #[error("restart requested on the setup page to apply saved settings")]
    RestartRequested,
    /// A hold form asked for an operation other than placing or releasing a hold.
    #[error("choose place or release")]
    ChoosePlaceOrRelease,
    /// A hold form named a scope other than a receipt or an order.
    #[error("choose receipt or order")]
    ChooseReceiptOrOrder,
    /// Reprocessing was refused because the configuration file on disk now names a data
    /// directory other than the store this admin server has open.
    #[error("current configuration points to a different store")]
    StoreMismatch,
    /// The machines page could not compute the cursor of its next page without integer
    /// overflow.
    #[error("machine cursor overflow")]
    MachineCursorOverflow,
    /// A parked item's page could not compute the cursor of its next page of targets without
    /// integer overflow.
    #[error("parked target cursor overflow")]
    ParkedTargetCursorOverflow,
    /// The overview's running count of uncertain machine orders would overflow; the page
    /// fails rather than show a wrong count.
    #[error("uncertain order count overflow")]
    UncertainOrderCountOverflow,
    /// An audited operator action failed.
    #[error(transparent)]
    Operator(Box<crate::operator::Error>),
    /// A runtime operation failed.
    #[error(transparent)]
    Runtime(#[from] crate::runtime::Error),
    /// An administration store command failed.
    #[error(transparent)]
    Store(#[from] CommandError),
    /// Retained analyzer execution evidence could not be queried.
    #[error(transparent)]
    Reconciliation(#[from] analyte_store::ReconciliationError),
    /// Audited raw-view input or evidence failed.
    #[error(transparent)]
    Raw(#[from] super::pages::RawError),
    /// Receipt interpretation failed.
    #[error(transparent)]
    Intake(#[from] crate::IntakeError),
    /// The installation configuration could not be read.
    #[error(transparent)]
    Config(#[from] crate::installation::config::Error),
    /// A bounded model text value is invalid.
    #[error(transparent)]
    Text(#[from] analyte_model::TextError),
    /// A model identity is invalid.
    #[error(transparent)]
    Identity(#[from] analyte_model::InvalidUuid),
    /// The supplied UUID is invalid.
    #[error(transparent)]
    Uuid(#[from] uuid::Error),
    /// An integer exceeds the target range.
    #[error(transparent)]
    Integer(#[from] std::num::TryFromIntError),
    /// The supplied integer is malformed.
    #[error(transparent)]
    ParseInteger(#[from] std::num::ParseIntError),
    /// The administration JSON could not be processed.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    /// The authentication token is invalid.
    #[error(transparent)]
    Token(#[from] super::TokenError),
    /// The delivery destination is invalid.
    #[error(transparent)]
    SinkDestinationError(#[from] analyte_sink::DestinationError),
    /// Preserves the owning sink API boundary.
    #[error(transparent)]
    SinkDeliveryError(#[from] analyte_sink::DeliveryError),
    /// Preserves the owning sink API boundary.
    #[error(transparent)]
    SinkPollError(#[from] analyte_sink::PollError),
    /// An administration I/O operation failed.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// The configured secret could not be loaded.
    #[error(transparent)]
    Secret(#[from] crate::installation::secrets::SecretError),
}

impl From<crate::operator::Error> for Error {
    fn from(error: crate::operator::Error) -> Self {
        Self::Operator(Box::new(error))
    }
}

impl From<analyte_store::OperatorError> for Error {
    fn from(error: analyte_store::OperatorError) -> Self {
        Self::Store(error.into())
    }
}
impl From<analyte_store::OrderError> for Error {
    fn from(error: analyte_store::OrderError) -> Self {
        Self::Store(error.into())
    }
}
impl From<analyte_store::MaintenanceError> for Error {
    fn from(error: analyte_store::MaintenanceError) -> Self {
        Self::Store(error.into())
    }
}
impl From<analyte_store::StorageError> for Error {
    fn from(error: analyte_store::StorageError) -> Self {
        Self::Store(error.into())
    }
}
impl From<analyte_store::DeliveryError> for Error {
    fn from(error: analyte_store::DeliveryError) -> Self {
        Self::Store(error.into())
    }
}
impl From<analyte_store::WorkerError> for Error {
    fn from(error: analyte_store::WorkerError) -> Self {
        Self::Store(error.into())
    }
}

/// Store and filesystem failures of mixed admin commands.
#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    /// The evidence boundary failed.
    #[error(transparent)]
    Evidence(#[from] analyte_store::OperatorError),
    /// The order boundary failed.
    #[error(transparent)]
    Order(#[from] analyte_store::OrderError),
    /// The maintenance boundary failed.
    #[error(transparent)]
    Maintenance(#[from] analyte_store::MaintenanceError),
    /// The audit boundary failed.
    #[error(transparent)]
    Audit(#[from] analyte_store::StorageError),
    /// The delivery boundary failed.
    #[error(transparent)]
    Delivery(#[from] analyte_store::DeliveryError),
    /// The worker boundary failed.
    #[error(transparent)]
    Worker(#[from] analyte_store::WorkerError),
    /// The host volume probe failed.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
