//! Distinguishes runtime startup, worker, storage and shutdown failures while retaining their causes.
/// Failures owned by runtime workers and protected-store startup.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Retains the integer parsing failure.
    #[error(transparent)]
    ParseInteger(#[from] std::num::ParseIntError),
    /// Retains the concrete integer failure.
    #[error(transparent)]
    Integer(#[from] std::num::TryFromIntError),
    /// Retains the concrete identity failure.
    #[error(transparent)]
    Identity(#[from] analyte_model::InvalidUuid),
    /// Retains the machine configuration failure.
    #[error(transparent)]
    MachineConfig(#[from] analyte_config::Error),
    /// Retains the concrete token failure.
    #[error(transparent)]
    Token(#[from] crate::admin::TokenError),
    /// Retains the concrete hooks failure.
    #[error(transparent)]
    Hooks(#[from] super::test_hooks::Error),
    /// Retains the concrete operator failure.
    #[error(transparent)]
    Operator(Box<crate::operator::Error>),
    /// Retains the concrete config failure.
    #[error(transparent)]
    Config(#[from] crate::installation::config::Error),
    /// Retains the concrete storage failure.
    #[error(transparent)]
    Storage(#[from] analyte_store::StorageError),
    /// Administration startup retains its owning error boundary.
    #[error(transparent)]
    Admin(Box<crate::admin::Error>),
    /// Logging initialization preserves its original cause.
    #[error(transparent)]
    Logging(Box<dyn std::error::Error + Send + Sync>),
    /// Operator evidence command failed.
    #[error(transparent)]
    ParkedCommand(#[from] ParkedCommand),
    /// Operator delivery command failed.
    #[error(transparent)]
    DeliveryCommand(#[from] DeliveryCommand),
    /// The host could not represent the storage request deadline.
    #[error(transparent)]
    Deadline(#[from] crate::DeadlineError),
    /// The runtime cannot safely continue the requested operation.
    #[error(
        "SCM dispatcher unavailable; use analyte run instead outside the Service Control Manager"
    )]
    ScmDispatcherUnavailableUseAnalyteRunInsteadOutsideTheServiceControlManager(
        #[source] DispatchCause,
    ),
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
    /// The runtime cannot safely continue the requested operation.
    #[error("admin shutdown exceeded 5 seconds")]
    AdminShutdownExceeded5Seconds(#[source] tokio::time::error::Elapsed),
    /// The runtime cannot safely continue the requested operation.
    #[error("alert evaluator shutdown exceeded 5 seconds")]
    AlertEvaluatorShutdownExceeded5Seconds(#[source] tokio::time::error::Elapsed),
    /// The runtime cannot safely continue the requested operation.
    #[error("alert evaluator stopped unexpectedly")]
    AlertEvaluatorStoppedUnexpectedly,
    /// The runtime cannot safely continue the requested operation.
    #[error("capture ownership retained after machine shutdown")]
    CaptureOwnershipRetainedAfterMachineShutdown,
    /// The runtime cannot safely continue the requested operation.
    #[error("data directory must deny group and other access")]
    DataDirectoryMustDenyGroupAndOtherAccess,
    /// The runtime cannot safely continue the requested operation.
    #[error("delivery owner stopped unexpectedly")]
    DeliveryOwnerStoppedUnexpectedly,
    /// The runtime cannot safely continue the requested operation.
    #[error("delivery shutdown exceeded 5 seconds; interrupted attempts will recover on restart")]
    DeliveryShutdownExceeded5SecondsInterruptedAttemptsWillRecoverOnRestart(
        #[source] tokio::time::error::Elapsed,
    ),
    /// The runtime cannot safely continue the requested operation.
    #[error("loopback overrides require the test service entry point")]
    LoopbackOverridesRequireTheTestServiceEntryPoint,
    /// The runtime cannot safely continue the requested operation.
    #[error("machine retained store ownership after shutdown")]
    MachineRetainedStoreOwnershipAfterShutdown,
    /// The runtime cannot safely continue the requested operation.
    #[error("pending LAW recovery page bound exceeded; machine startup refused")]
    PendingLawRecoveryPageBoundExceededMachineStartupRefused,
    /// The runtime cannot safely continue the requested operation.
    #[error("receive commits exceeded 15 seconds")]
    ReceiveCommitsExceeded15Seconds(#[source] tokio::time::error::Elapsed),
    /// The runtime cannot safely continue the requested operation.
    #[error("retention deadline overflow")]
    RetentionDeadlineOverflow,
    /// The runtime cannot safely continue the requested operation.
    #[error("shutdown bound must be 30 through 300 seconds")]
    ShutdownBoundMustBe30Through300Seconds,
    /// The runtime cannot safely continue the requested operation.
    #[error("store startup deadline overflow")]
    StoreStartupDeadlineOverflow,
    /// The runtime cannot safely continue the requested operation.
    #[error("stuck shutdown hook requires the loopback test service")]
    StuckShutdownHookRequiresTheLoopbackTestService,
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
    /// Startup LAW recovery combined retained evidence with reconciliation.
    #[error(transparent)]
    LawRecovery(#[from] super::law_recovery::RecoveryError),
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

/// Failures of the parked-evidence command, including its host input refusals.
#[derive(Debug, thiserror::Error)]
pub enum ParkedCommand {
    /// Retains the failure of this command operation.
    #[error(transparent)]
    Evidence(#[from] analyte_store::OperatorError),
    /// Retains the failure of this command operation.
    #[error(transparent)]
    Audit(#[from] analyte_store::StorageError),
    /// Retains the failure of this command operation.
    #[error(transparent)]
    Worker(#[from] analyte_store::WorkerError),
    /// Retains the failure of this command operation.
    #[error("reprocessing requires the interpretation route")]
    ReprocessRequiresInterpretation,
    /// Retains the failure of this command operation.
    #[error("operator sequence cursor overflow")]
    OperatorCursorOverflow,
}

/// Boundaries combined by the audited delivery listing command.
#[derive(Debug, thiserror::Error)]
pub enum DeliveryCommand {
    /// Retains the failure of this command operation.
    #[error(transparent)]
    Delivery(#[from] analyte_store::DeliveryError),
    /// Retains the failure of this command operation.
    #[error(transparent)]
    Audit(#[from] analyte_store::StorageError),
    /// Retains the failure of this command operation.
    #[error(transparent)]
    Worker(#[from] analyte_store::WorkerError),
}

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
