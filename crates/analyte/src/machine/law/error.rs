//! Classifies LAW session refusals with shared link and custody causes.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    /// LAW independent peer closed before ORL.
    #[error("LAW independent peer closed before ORL")]
    LawPeerClosedBeforeOrl,
    /// LAW connection deadline overflow.
    #[error("LAW connection deadline overflow")]
    LawConnectionDeadlineOverflow,
    /// LAW inbound task panicked; machine owner stopped.
    #[error("LAW inbound task panicked; machine owner stopped")]
    LawInboundTaskPanickedMachineOwnerStopped(#[source] tokio::task::JoinError),
    /// LAW inbound task was cancelled before returning its outcome.
    #[error("LAW inbound task was cancelled before returning its outcome")]
    LawInboundTaskWasCancelledBeforeReturningItsOutcome(#[source] tokio::task::JoinError),
    /// LAW independent destination absent.
    #[error("LAW independent destination absent")]
    LawIndependentDestinationAbsent,
    /// LAW independent peer disconnected.
    #[error("LAW independent peer disconnected")]
    LawIndependentPeerDisconnected,
    /// LAW independent transaction deadline.
    #[error("LAW independent transaction deadline")]
    LawIndependentTransactionDeadline(#[source] tokio::time::error::Elapsed),
    /// LAW input still awaits custody.
    #[error("LAW input still awaits custody")]
    LawInputStillAwaitsCustody,
    /// LAW outbound connect deadline.
    #[error("LAW outbound connect deadline")]
    LawOutboundConnectDeadline(#[source] tokio::time::error::Elapsed),
    /// LAW outbound deadline absent.
    #[error("LAW outbound deadline absent")]
    LawOutboundDeadlineAbsent,
    /// LAW outbound queue unavailable; reserved work not issued.
    #[error("LAW outbound queue unavailable; reserved work not issued")]
    LawOutboundQueueUnavailableReservedWorkNotIssued,
    /// LAW outbound work expired before connection.
    #[error("LAW outbound work expired before connection")]
    LawOutboundWorkExpiredBeforeConnection,
    /// LAW outgoing deadline absent.
    #[error("LAW outgoing deadline absent")]
    LawOutgoingDeadlineAbsent,
    /// LAW queued work has no deadline.
    #[error("LAW queued work has no deadline")]
    LawQueuedWorkHasNoDeadline,
    /// LAW receipt correlation missing.
    #[error("LAW receipt correlation missing")]
    LawReceiptCorrelationMissing,
    /// LAW response deadline absent after issuance.
    #[error("LAW response deadline absent after issuance")]
    LawResponseDeadlineAbsentAfterIssuance,
    /// LAW response send deadline expired.
    #[error("LAW response send deadline expired")]
    LawResponseSendDeadlineExpired(#[source] analyte_transport::Error),
    /// Independent LAW stack absent.
    #[error("independent LAW stack absent")]
    IndependentLawStackAbsent,
    /// The independent LAW transport ended before confirmed completion.
    #[error("LAW independent transport failed: {0}")]
    LawTransportFailed(analyte_transport::Reason),
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
    LawDialog(#[from] analyte_dialog::law::Error),
    /// HL7 segment syntax or acknowledgment fields were invalid.
    #[error(transparent)]
    Hl7(#[from] analyte_hl7::Error),
    /// LAW message grammar or profile validation failed.
    #[error(transparent)]
    Law(#[from] analyte_hl7::law::Error),
    /// The HL7 transport envelope failed MLLP framing.
    #[error(transparent)]
    Mllp(#[from] analyte_framing::hl7::Error),
    /// A bounded synthetic gate stopped or could not resume the connection.
    #[cfg(feature = "test-hooks")]
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

crate::machine::receive::error::session_boundary!(Error);
