//! Distinguishes runtime startup, worker, storage and shutdown failures while retaining their causes.
/// Failures owned by runtime workers and protected-store startup.
#[derive(Debug, thiserror::Error)]
pub enum Error {
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
    /// Preserves the original runtime dependency failure.
    #[error(transparent)]
    Store(#[from] analyte_store::Error),
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

analyte_store::forward_command_errors!(Error);
