//! Distinguishes runtime startup, worker, storage, shutdown and audited command failures while retaining their causes.
/// Failures owned by runtime workers and protected-store startup.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A persistence primitive refused startup or an operator operation.
    #[error(transparent)]
    Storage(#[from] analyte_store::StorageError),

    /// An audited parked-evidence command failed.
    #[error(transparent)]
    ParkedCommand(#[from] ParkedCommandError),
    /// An audited delivery command failed.
    #[error(transparent)]
    DeliveryCommand(#[from] DeliveryCommandError),

    /// Administration startup or supervision failed.
    #[error(transparent)]
    Admin(Box<crate::admin::Error>),
    /// An audited operator operation failed.
    #[error(transparent)]
    Operator(Box<crate::operator::Error>),
    /// Program configuration could not be loaded.
    #[error(transparent)]
    Config(#[from] crate::installation::config::Error),
    /// No program file was named or found in the usual places.
    #[error(transparent)]
    ProgramFile(#[from] crate::installation::program_file::LocateError),
    /// A synthetic gate could not publish its store probe.
    #[error(transparent)]
    TestHook(#[from] super::test_hooks::Error),
    /// The service stop channel closed unexpectedly.
    #[error(transparent)]
    StopChannel(#[from] tokio::sync::oneshot::error::RecvError),
    /// The process tracing subscriber could not be installed.
    #[error(transparent)]
    Tracing(Box<dyn std::error::Error + Send + Sync>),

    /// The host could not represent the storage request deadline.
    #[error(transparent)]
    Deadline(#[from] crate::DeadlineError),
    /// The runtime cannot safely continue the requested operation.
    #[error(
        "SCM dispatcher unavailable; use analyte run instead outside the Service Control Manager"
    )]
    ScmDispatcherUnavailable(#[source] DispatchCause),
    /// The runtime cannot safely continue the requested operation.
    #[error("SCM runtime owner panicked")]
    ScmRuntimeOwnerPanicked,
    /// The runtime cannot safely continue the requested operation.
    #[error("SCM stop sender closed")]
    ScmStopSenderClosed(#[source] tokio::sync::oneshot::error::RecvError),
    /// The runtime cannot safely continue the requested operation.
    #[error("SIGINT signal stream closed")]
    SigintSignalStreamClosed,
    /// The runtime cannot safely continue the requested operation.
    #[error("SIGTERM signal stream closed")]
    SigtermSignalStreamClosed,
    /// The runtime cannot safely continue the requested operation.
    #[error("admin owner stopped unexpectedly")]
    AdminOwnerStoppedUnexpectedly,
    /// An operator asked on the setup page to restart and apply saved settings.
    /// The service stopped gracefully and exits nonzero, so its manager (Windows
    /// recovery actions, systemd `Restart=on-failure`) starts it again.
    #[error(
        "restart requested on the setup page; the service manager starts Analyte again with the saved settings"
    )]
    RestartRequested,
    /// The runtime cannot safely continue the requested operation.
    #[error("admin shutdown exceeded 5 seconds")]
    AdminShutdownExceeded5Seconds(#[source] tokio::time::error::Elapsed),
    /// The runtime cannot safely continue the requested operation.
    #[error("alert evaluator shutdown exceeded 5 seconds")]
    AlertShutdownTimeout(#[source] tokio::time::error::Elapsed),
    /// The runtime cannot safely continue the requested operation.
    #[error("alert evaluator stopped unexpectedly")]
    AlertEvaluatorStoppedUnexpectedly,
    /// The runtime cannot safely continue the requested operation.
    #[error("capture ownership retained after machine shutdown")]
    CaptureOwnershipRetained,
    /// The runtime cannot safely continue the requested operation.
    #[error("data directory must deny group and other access")]
    InsecureDataDirectory,
    /// The Windows data directory grants access beyond the allowed principals.
    #[cfg(windows)]
    #[error("data directory: {0}")]
    InsecureWindowsDataDirectory(#[from] super::service::acl::Error),
    /// The runtime cannot safely continue the requested operation.
    #[error("delivery owner stopped unexpectedly")]
    DeliveryOwnerStoppedUnexpectedly,
    /// The runtime cannot safely continue the requested operation.
    #[error("delivery shutdown exceeded 5 seconds; interrupted attempts will recover on restart")]
    DeliveryShutdownTimeout(#[source] tokio::time::error::Elapsed),
    /// The runtime cannot safely continue the requested operation.
    #[error("loopback overrides require the test service entry point")]
    TestServiceRequired,
    /// The runtime cannot safely continue the requested operation.
    #[error("machine retained store ownership after shutdown")]
    StoreOwnershipRetained,
    /// The runtime cannot safely continue the requested operation.
    #[error("startup recovery page bound exceeded; machine startup refused")]
    RecoveryPageLimit,
    /// The runtime cannot safely continue the requested operation.
    #[error("receive commits exceeded 15 seconds")]
    ReceiveCommitsExceeded15Seconds(#[source] tokio::time::error::Elapsed),
    /// The runtime cannot safely continue the requested operation.
    #[error("retention deadline overflow")]
    RetentionDeadlineOverflow,
    /// The runtime cannot safely continue the requested operation.
    #[error("shutdown bound must be 30 through 300 seconds")]
    InvalidShutdownBound,
    /// The runtime cannot safely continue the requested operation.
    #[error("store startup deadline overflow")]
    StoreStartupDeadlineOverflow,
    /// The runtime cannot safely continue the requested operation.
    #[error("shutdown hooks require the loopback test service")]
    ShutdownTestServiceRequired,
    /// The runtime cannot safely continue the requested operation.
    #[error("transport shutdown exceeded 5 seconds")]
    TransportShutdownExceeded5Seconds(#[source] tokio::time::error::Elapsed),
    /// Preserves the original runtime dependency failure.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// Preserves the Maintenance store boundary.
    #[error(transparent)]
    Maintenance(#[from] analyte_store::MaintenanceError),
    /// Preserves the Evidence store boundary.
    #[error(transparent)]
    Evidence(#[from] analyte_store::OperatorError),
    /// Preserves the Delivery store boundary.
    #[error(transparent)]
    Delivery(#[from] analyte_store::DeliveryError),
    /// Preserves the Worker store boundary.
    #[error(transparent)]
    Worker(#[from] analyte_store::WorkerError),
    /// Preserves the `WorkerStart` store boundary.
    #[error(transparent)]
    WorkerStart(#[from] analyte_store::WorkerStartError),
    /// Startup recovery combined retained evidence with reconciliation.
    #[error(transparent)]
    Recovery(#[from] super::recovery::RecoveryError),
    /// Preserves the original runtime dependency failure.
    #[error(transparent)]
    SinkDestinationError(#[from] analyte_sink::DestinationError),
    /// Preserves the owning sink API boundary.
    #[error(transparent)]
    SinkDeliveryError(#[from] analyte_sink::DeliveryError),
    /// Preserves the owning sink API boundary.
    #[error(transparent)]
    SinkPollError(#[from] analyte_sink::PollError),
    /// Preserves the original runtime dependency failure.
    #[error(transparent)]
    Secrets(#[from] crate::installation::secrets::SecretError),
    /// Preserves the original runtime dependency failure.
    #[error(transparent)]
    Intake(#[from] crate::IntakeError),
    /// Preserves the original runtime dependency failure.
    #[error(transparent)]
    Orders(#[from] crate::orders::OrderError),
    /// Preserves the original runtime dependency failure.
    #[error(transparent)]
    Text(#[from] analyte_model::TextError),
    /// Preserves the original runtime dependency failure.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    /// Preserves the original runtime dependency failure.
    #[error(transparent)]
    Join(#[from] tokio::task::JoinError),
}

/// Original static diagnostic returned by the Windows service dispatcher.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct DispatchCause(pub &'static str);

impl From<crate::admin::Error> for Error {
    fn from(error: crate::admin::Error) -> Self {
        Self::Admin(Box::new(error))
    }
}
impl From<crate::operator::Error> for Error {
    fn from(error: crate::operator::Error) -> Self {
        Self::Operator(Box::new(error))
    }
}

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
    /// Resolving was sent to the wrong command route.
    #[error("resolving requires the recovery route")]
    ResolveUsesRecovery,
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
