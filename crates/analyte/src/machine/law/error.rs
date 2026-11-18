//! Classifies LAW session refusals and order failures with their shared causes.
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
    InboundTaskPanicked(#[source] tokio::task::JoinError),
    /// LAW inbound task was cancelled before returning its outcome.
    #[error("LAW inbound task was cancelled before returning its outcome")]
    InboundTaskCancelled(#[source] tokio::task::JoinError),
    /// LAW independent destination absent.
    #[error("LAW independent destination absent")]
    IndependentDestinationAbsent,
    /// LAW independent peer disconnected.
    #[error("LAW independent peer disconnected")]
    IndependentPeerDisconnected,
    /// LAW independent transaction deadline.
    #[error("LAW independent transaction deadline")]
    IndependentTransactionDeadline(#[source] tokio::time::error::Elapsed),
    /// LAW outbound connect deadline.
    #[error("LAW outbound connect deadline")]
    OutboundConnectDeadline(#[source] tokio::time::error::Elapsed),
    /// LAW outbound deadline absent.
    #[error("LAW outbound deadline absent")]
    OutboundDeadlineAbsent,
    /// LAW outbound queue unavailable; reserved work not issued.
    #[error("LAW outbound queue unavailable; reserved work not issued")]
    OutboundQueueUnavailable,
    /// LAW outbound work expired before connection.
    #[error("LAW outbound work expired before connection")]
    OutboundWorkExpired,
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
    MissingResponseDeadline,
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
    /// Order lookup, mapping, reservation or completion failed.
    #[error(transparent)]
    Orders(#[from] OrderError),
    /// The LAW transaction could not advance its phase.
    #[error(transparent)]
    Dialog(#[from] analyte_dialog::law::Error),
    /// HL7 segment syntax or acknowledgment fields were invalid.
    #[error(transparent)]
    Hl7(#[from] analyte_hl7::Error),
    /// Retains link, store and evidence causes from [`crate::machine::receive::error::Error`],
    /// uncertain custody and independent cleanup failures in their original order.
    #[error(transparent)]
    Refusal(#[from] crate::machine::receive::custody::Refusal<Self>),
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

crate::machine::receive::custody::refusal_conversions!(Error);

/// Failures answering LAW work queries or sending LAW work.
#[derive(Debug, thiserror::Error)]
pub(crate) enum OrderError {
    /// Cache collection exceeded the query deadline.
    #[error("operation deadline expired")]
    LookupDeadline,
    /// The preparation deadline could not be represented.
    #[error("LAW preparation deadline overflow")]
    PreparationDeadlineOverflow,
    /// The query deadline could not be represented.
    #[error("LAW query deadline overflow")]
    QueryDeadlineOverflow,
    /// The query failure has no recoverable acknowledgment outcome.
    #[error("LAW query failure has no recoverable acknowledgment outcome")]
    QueryFailureHasNoAckOutcome,
    /// The container assigning authority is outside the configured `SampleId` binding.
    #[error("LAW container assigning authority outside configured SampleId binding")]
    ContainerAuthorityMismatch,
    /// A malformed query lacks recoverable correlation.
    #[error("LAW malformed query lacks recoverable correlation")]
    QueryCorrelationMissing,
    /// The query deadline expired after reservation.
    #[error("LAW query deadline after reservation")]
    QueryDeadlineAfterReservation,
    /// The query deadline expired before reservation.
    #[error("LAW query deadline before reservation")]
    QueryDeadlineBeforeReservation,
    /// The query deadline expired during rendering.
    #[error("LAW query deadline during rendering")]
    QueryDeadlineDuringRendering,
    /// A LAW followup arrived inside another followup.
    #[error("unexpected nested LAW followup")]
    UnexpectedNestedFollowup,
    /// The query scope is outside the supported LAW subset.
    #[error("unsupported LAW query scope")]
    UnsupportedQueryScope,
    /// Failure injected only by the optional test harness.
    #[error(transparent)]
    TestHook(crate::runtime::test_hooks::Error),
    /// LAW work mapping failed.
    #[error(transparent)]
    Mapping(#[from] analyte_mapping::law::WireError),
    /// LAW work or query syntax was invalid.
    #[error(transparent)]
    Syntax(#[from] analyte_hl7::law::Error),
    /// HL7 segment syntax or acknowledgment fields were invalid.
    #[error(transparent)]
    Hl7(#[from] analyte_hl7::Error),
    /// The LAW work exchange rejected an event or deadline.
    #[error(transparent)]
    Dialog(#[from] analyte_dialog::law::Error),
    /// The store refused admission of this order version or lifecycle state.
    #[error(transparent)]
    Refused(#[from] analyte_store::SendRefusal),
    /// Shared order lookup, storage or settlement failed.
    #[error(transparent)]
    Shared(crate::orders::OrderError),
    /// Retains both the triggering failure and a later independent settlement failure.
    #[error(transparent)]
    Cleanup(Box<crate::machine::receive::custody::Cleanup<Self>>),
}
crate::orders::shared_conversions!(OrderError);
