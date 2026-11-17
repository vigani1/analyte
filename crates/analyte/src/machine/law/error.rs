//! Classifies LAW session refusals with shared link and custody causes.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    /// LAW independent peer closed before ORL.
    #[error("LAW independent peer closed before ORL")]
    PeerClosedBeforeOrl,
    /// LAW connection deadline overflow.
    #[error("LAW connection deadline overflow")]
    ConnectionDeadlineOverflow,
    /// LAW inbound task panicked; machine owner stopped.
    #[error("LAW inbound task panicked; machine owner stopped")]
    InboundTaskPanickedMachineOwnerStopped(#[source] tokio::task::JoinError),
    /// LAW inbound task was cancelled before returning its outcome.
    #[error("LAW inbound task was cancelled before returning its outcome")]
    InboundTaskWasCancelledBeforeReturningItsOutcome(#[source] tokio::task::JoinError),
    /// LAW independent destination absent.
    #[error("LAW independent destination absent")]
    IndependentDestinationAbsent,
    /// LAW independent peer disconnected.
    #[error("LAW independent peer disconnected")]
    IndependentPeerDisconnected,
    /// LAW independent transaction deadline.
    #[error("LAW independent transaction deadline")]
    IndependentTransactionDeadline(#[source] tokio::time::error::Elapsed),
    /// LAW input still awaits custody.
    #[error("LAW input still awaits custody")]
    InputStillAwaitsCustody,
    /// LAW outbound connect deadline.
    #[error("LAW outbound connect deadline")]
    OutboundConnectDeadline(#[source] tokio::time::error::Elapsed),
    /// LAW outbound deadline absent.
    #[error("LAW outbound deadline absent")]
    OutboundDeadlineAbsent,
    /// LAW outbound queue unavailable; reserved work not issued.
    #[error("LAW outbound queue unavailable; reserved work not issued")]
    OutboundQueueUnavailableReservedWorkNotIssued,
    /// LAW outbound work expired before connection.
    #[error("LAW outbound work expired before connection")]
    OutboundWorkExpiredBeforeConnection,
    /// LAW outgoing deadline absent.
    #[error("LAW outgoing deadline absent")]
    OutgoingDeadlineAbsent,
    /// LAW queued work has no deadline.
    #[error("LAW queued work has no deadline")]
    QueuedWorkHasNoDeadline,
    /// LAW receipt correlation missing.
    #[error("LAW receipt correlation missing")]
    ReceiptCorrelationMissing,
    /// LAW response deadline absent after issuance.
    #[error("LAW response deadline absent after issuance")]
    ResponseDeadlineAbsentAfterIssuance,
    /// LAW response send deadline expired.
    #[error("LAW response send deadline expired")]
    ResponseSendDeadlineExpired(#[source] analyte_transport::Error),
    /// Independent LAW stack absent.
    #[error("independent LAW stack absent")]
    IndependentLawStackAbsent,
    /// The independent LAW transport ended before confirmed completion.
    #[error("LAW independent transport failed: {0}")]
    TransportFailed(analyte_transport::Reason),
    /// Preserves the `Deadline` failure and its original cause.
    #[error("{reason}")]
    Deadline {
        reason: DeadlineReason,
        #[source]
        source: tokio::time::error::Elapsed,
    },
    /// An outbound command could not represent the mapped order.
    #[error(transparent)]
    OrderMapping(#[from] analyte_mapping::OrderMappingError),
    /// Order reservation or completion could not be persisted.
    #[error(transparent)]
    Orders(#[from] crate::orders::OrderError),
    /// The LAW transaction could not advance its phase.
    #[error(transparent)]
    Dialog(#[from] analyte_dialog::law::Error),
    /// HL7 segment syntax or acknowledgment fields were invalid.
    #[error(transparent)]
    Hl7(#[from] analyte_hl7::Error),
    /// LAW message grammar or profile validation failed.
    #[error(transparent)]
    Syntax(#[from] analyte_hl7::law::Error),
    /// The HL7 transport envelope failed MLLP framing.
    #[error(transparent)]
    Mllp(#[from] analyte_framing::hl7::Error),
    /// A bounded synthetic gate stopped or could not resume the connection.
    #[error(transparent)]
    TestHook(crate::runtime::test_hooks::Error),
    /// Retains uncertain custody or independent cleanup failures in their original order.
    #[error(transparent)]
    Refusal(#[from] crate::machine::custody::Refusal<Self>),
}

#[derive(Debug)]
pub(crate) enum DeadlineReason {
    Write,
    Effect,
}
impl std::fmt::Display for DeadlineReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Write => "LAW response send deadline expired",
            Self::Effect => "LAW processing deadline expired; no acceptance promised",
        })
    }
}

impl From<crate::machine::link::error::Error> for Error {
    fn from(error: crate::machine::link::error::Error) -> Self {
        Self::Refusal(crate::machine::custody::Refusal::Link(Box::new(error)))
    }
}
impl From<crate::machine::receive::error::Error> for Error {
    fn from(error: crate::machine::receive::error::Error) -> Self {
        Self::Refusal(crate::machine::custody::Refusal::Evidence(Box::new(error)))
    }
}
impl From<crate::machine::custody::StoreWaitError<Self>> for Error {
    fn from(error: crate::machine::custody::StoreWaitError<Self>) -> Self {
        Self::Refusal(crate::machine::custody::Refusal::Custody(Box::new(error)))
    }
}
impl crate::machine::custody::CleanupFailure for Error {
    fn with_effect(original: Self, effect: Self) -> Self {
        Self::Refusal(crate::machine::custody::Refusal::Cleanup(Box::new(
            crate::machine::custody::Cleanup { original, effect },
        )))
    }
}

crate::runtime::service::conversions::causes!(Error;
    crate::machine::link::error::Error => [
        std::io::Error,
        analyte_transport::Error,
        std::net::AddrParseError,
        analyte_transport::folder::WatchError,
    ],
    crate::machine::receive::error::Error => [
        analyte_store::Error,
        crate::IntakeError,
        crate::machine::interpretation::ReadError,
        analyte_model::TextError,
        analyte_model::ListError,
        analyte_config::Error,
        std::num::TryFromIntError,
        serde_json::Error,
        jiff::Error,
        crate::reading::Error,
        crate::machine::custody::WriteError,
        crate::machine::custody::StoreWaitError<crate::IntakeError>,
        crate::machine::custody::StoreWaitError<analyte_store::Error>,
    ],
    analyte_store::Error => [
        analyte_store::WorkerError,
        analyte_store::WorkerStartError,
        analyte_store::IntakeError,
        analyte_store::DeliveryError,
        analyte_store::OrderError,
        analyte_store::MaintenanceError,
        analyte_store::OperatorError,
        analyte_store::ReconciliationError,
        analyte_store::StorageError,
    ],
);
