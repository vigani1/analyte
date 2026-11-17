//! Distinguishes machine refusal, transport, custody, deadlines and independent cleanup failures.
/// Operational failures within one owned machine session.
#[derive(Debug, thiserror::Error)]
pub(crate) enum SessionError {
    #[cfg(test)]
    #[error(transparent)]
    Fixture(crate::Error),
    #[error("LAW independent peer closed before ORL")]
    LawPeerClosedBeforeOrl,
    #[error(transparent)]
    OrderMapping(#[from] analyte_mapping::OrderMappingError),
    #[error(transparent)]
    PoctData(#[from] analyte_poct::DataError),
    #[error(transparent)]
    AstmFrame(#[from] analyte_framing::astm::Error),
    #[error(transparent)]
    Watch(#[from] analyte_transport::folder::WatchError),
    #[error(transparent)]
    MappingRead(#[from] analyte_mapping::read::Error),
    /// The session cannot safely proceed under this state or bound.
    #[error("ASTM admission size overflow")]
    AstmAdmissionSizeOverflow,
    /// The session cannot safely proceed under this state or bound.
    #[error("ASTM message bound exceeded")]
    AstmMessageBoundExceeded,
    /// The session cannot safely proceed under this state or bound.
    #[error("ASTM storage or resource limit refused transfer")]
    AstmStorageOrResourceLimitRefusedTransfer,
    /// The session cannot safely proceed under this state or bound.
    #[error("ASTM transfer wire-byte bound exceeded")]
    AstmTransferWireByteBoundExceeded,
    /// The session cannot safely proceed under this state or bound.
    #[error("Autolyser poll confirmation did not reconcile custody")]
    AutolyserPollConfirmationDidNotReconcileCustody,
    /// The session cannot safely proceed under this state or bound.
    #[error("Autolyser previous poll requires operator reconciliation before R can advance")]
    AutolyserPreviousPollRequiresOperatorReconciliationBeforeRCanAdvance,
    /// The session cannot safely proceed under this state or bound.
    #[error("Autolyser received another stack's order")]
    AutolyserReceivedAnotherStackSOrder,
    /// The session cannot safely proceed under this state or bound.
    #[error("Autolyser report receipt missing")]
    AutolyserReportReceiptMissing,
    /// The session cannot safely proceed under this state or bound.
    #[error("HL7 display services absent")]
    Hl7DisplayServicesAbsent,
    /// The session cannot safely proceed under this state or bound.
    #[error("HL7 host phase timed out; receipt requires reconciliation")]
    Hl7HostPhaseTimedOutReceiptRequiresReconciliation,
    /// The session cannot safely proceed under this state or bound.
    #[error("HL7 message refused")]
    Hl7MessageRefused,
    /// The session cannot safely proceed under this state or bound.
    #[error("HL7 order services absent")]
    Hl7OrderServicesAbsent,
    /// The session cannot safely proceed under this state or bound.
    #[error("HL7 pending receipt limit exceeded")]
    Hl7PendingReceiptLimitExceeded,
    /// The session cannot safely proceed under this state or bound.
    #[error("HL7 query services absent")]
    Hl7QueryServicesAbsent,
    /// The session cannot safely proceed under this state or bound.
    #[error("HL7 receipt correlation missing")]
    Hl7ReceiptCorrelationMissing,
    /// The session cannot safely proceed under this state or bound.
    #[error("LAW connection deadline overflow")]
    LawConnectionDeadlineOverflow,
    /// The session cannot safely proceed under this state or bound.
    #[error("LAW inbound task panicked; machine owner stopped")]
    LawInboundTaskPanickedMachineOwnerStopped(#[source] tokio::task::JoinError),
    /// The session cannot safely proceed under this state or bound.
    #[error("LAW inbound task was cancelled before returning its outcome")]
    LawInboundTaskWasCancelledBeforeReturningItsOutcome(#[source] tokio::task::JoinError),
    /// The session cannot safely proceed under this state or bound.
    #[error("LAW independent destination absent")]
    LawIndependentDestinationAbsent,
    /// The session cannot safely proceed under this state or bound.
    #[error("LAW independent peer disconnected")]
    LawIndependentPeerDisconnected,
    /// The session cannot safely proceed under this state or bound.
    #[error("LAW independent transaction deadline")]
    LawIndependentTransactionDeadline(#[source] tokio::time::error::Elapsed),
    /// The session cannot safely proceed under this state or bound.
    #[error("LAW input still awaits custody")]
    LawInputStillAwaitsCustody,
    /// The session cannot safely proceed under this state or bound.
    #[error("LAW outbound connect deadline")]
    LawOutboundConnectDeadline(#[source] tokio::time::error::Elapsed),
    /// The session cannot safely proceed under this state or bound.
    #[error("LAW outbound deadline absent")]
    LawOutboundDeadlineAbsent,
    /// The session cannot safely proceed under this state or bound.
    #[error("LAW outbound queue unavailable; reserved work not issued")]
    LawOutboundQueueUnavailableReservedWorkNotIssued,
    /// The session cannot safely proceed under this state or bound.
    #[error("LAW outbound work expired before connection")]
    LawOutboundWorkExpiredBeforeConnection,
    /// The session cannot safely proceed under this state or bound.
    #[error("LAW outgoing deadline absent")]
    LawOutgoingDeadlineAbsent,
    /// The session cannot safely proceed under this state or bound.
    #[error("LAW queued work has no deadline")]
    LawQueuedWorkHasNoDeadline,
    /// The session cannot safely proceed under this state or bound.
    #[error("LAW receipt correlation missing")]
    LawReceiptCorrelationMissing,
    /// The session cannot safely proceed under this state or bound.
    #[error("LAW response deadline absent after issuance")]
    LawResponseDeadlineAbsentAfterIssuance,
    /// The session cannot safely proceed under this state or bound.
    #[error("LAW response send deadline expired")]
    LawResponseSendDeadlineExpired(#[source] analyte_transport::Error),
    /// The session cannot safely proceed under this state or bound.
    #[error("POCT backpressure violated")]
    PoctBackpressureViolated,
    /// The session cannot safely proceed under this state or bound.
    #[error("POCT framing length")]
    PoctFramingLength,
    /// The session cannot safely proceed under this state or bound.
    #[error("POCT message limit exceeded")]
    PoctMessageLimitExceeded,
    /// The session cannot safely proceed under this state or bound.
    #[error("VIDAS active dialog has no deadline")]
    VidasActiveDialogHasNoDeadline,
    /// The session cannot safely proceed under this state or bound.
    #[error("VIDAS control suffix mismatch")]
    VidasControlSuffixMismatch,
    /// The session cannot safely proceed under this state or bound.
    #[error("VIDAS mapped request missing")]
    VidasMappedRequestMissing,
    /// The session cannot safely proceed under this state or bound.
    #[error("VIDAS order poll deadline overflow")]
    VidasOrderPollDeadlineOverflow,
    /// The session cannot safely proceed under this state or bound.
    #[error("VIDAS received another stack's order")]
    VidasReceivedAnotherStackSOrder,
    /// The session cannot safely proceed under this state or bound.
    #[error("VIDAS transfer parked; reconcile before reconnect")]
    VidasTransferParkedReconcileBeforeReconnect,
    /// The session cannot safely proceed under this state or bound.
    #[error("display receipt absent")]
    DisplayReceiptAbsent,
    /// The session cannot safely proceed under this state or bound.
    #[error("file completion reply lost; source remains claimed")]
    FileCompletionReplyLostSourceRemainsClaimed(
        #[source] std::sync::mpsc::SendError<analyte_transport::folder::Disposition>,
    ),
    /// The session cannot safely proceed under this state or bound.
    #[error("file owner panicked")]
    FileOwnerPanicked,
    /// The session cannot safely proceed under this state or bound.
    #[error("file owner shutdown timed out")]
    FileOwnerShutdownTimedOut(#[source] tokio::time::error::Elapsed),
    /// The session cannot safely proceed under this state or bound.
    #[error("file owner stopped")]
    FileOwnerStopped,
    /// The session cannot safely proceed under this state or bound.
    #[error("file transport ended before completion; source remains claimed")]
    FileTransportEndedBeforeCompletionSourceRemainsClaimed,
    /// The session cannot safely proceed under this state or bound.
    #[error("file transport opened twice")]
    FileTransportOpenedTwice,
    /// The session cannot safely proceed under this state or bound.
    #[error("file transport receipt bound exceeded")]
    FileTransportReceiptBoundExceeded,
    /// The session cannot safely proceed under this state or bound.
    #[error("independent LAW stack absent")]
    IndependentLawStackAbsent,
    /// The session cannot safely proceed under this state or bound.
    #[error("invalid serial data bits")]
    InvalidSerialDataBits,
    /// The session cannot safely proceed under this state or bound.
    #[error("invalid serial stop bits")]
    InvalidSerialStopBits,
    /// The session cannot safely proceed under this state or bound.
    #[error("late receipt absent")]
    LateReceiptAbsent,
    /// The session cannot safely proceed under this state or bound.
    #[error("order poll deadline overflow")]
    OrderPollDeadlineOverflow,
    /// The session cannot safely proceed under this state or bound.
    #[error("order response receipt absent")]
    OrderResponseReceiptAbsent,
    /// The session cannot safely proceed under this state or bound.
    #[error("query clock overflow")]
    QueryClockOverflow,
    /// The session cannot safely proceed under this state or bound.
    #[error("query receipt absent")]
    QueryReceiptAbsent,
    /// The session cannot safely proceed under this state or bound.
    #[error("query write deadline overflow")]
    QueryWriteDeadlineOverflow,
    /// The session cannot safely proceed under this state or bound.
    #[error("serial owner panicked")]
    SerialOwnerPanicked,
    /// The session cannot safely proceed under this state or bound.
    #[error("serial owner shutdown timed out")]
    SerialOwnerShutdownTimedOut(#[source] tokio::time::error::Elapsed),
    /// The session cannot safely proceed under this state or bound.
    #[error("transport close stage exceeded 25 seconds")]
    TransportCloseStageExceeded25Seconds(#[source] tokio::time::error::Elapsed),
    /// The session cannot safely proceed under this state or bound.
    #[error("unexpected Autolyser insertion outcome")]
    UnexpectedAutolyserInsertionOutcome,
    /// The session cannot safely proceed under this state or bound.
    #[error("unexpected outgoing HL7 outcome")]
    UnexpectedOutgoingHl7Outcome,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Transport(#[from] analyte_transport::Error),
    #[error(transparent)]
    Store(#[from] analyte_store::Error),
    #[error(transparent)]
    Intake(#[from] crate::IntakeError),
    #[error(transparent)]
    Orders(#[from] crate::orders::OrderError),
    #[error(transparent)]
    Reading(#[from] super::interpretation::ReadError),
    #[error(transparent)]
    Text(#[from] analyte_model::TextError),
    #[error(transparent)]
    List(#[from] analyte_model::ListError),
    #[error(transparent)]
    Config(#[from] analyte_config::Error),
    #[error(transparent)]
    AstmDialog(#[from] analyte_dialog::astm::Error),
    #[error(transparent)]
    Hl7Dialog(#[from] analyte_dialog::hl7::Error),
    #[error(transparent)]
    LawDialog(#[from] analyte_dialog::law::Error),
    #[error(transparent)]
    PoctDialog(#[from] analyte_dialog::poct::Error),
    #[error(transparent)]
    VidasDialog(#[from] analyte_dialog::vidas::Error),
    #[error(transparent)]
    AutolyserDialog(#[from] analyte_dialog::autolyser::Error),
    #[error(transparent)]
    Astm(#[from] analyte_astm::Error),
    #[error(transparent)]
    Hl7(#[from] analyte_hl7::Error),
    #[error(transparent)]
    Law(#[from] analyte_hl7::law::Error),
    #[error(transparent)]
    Poct(#[from] analyte_poct::MessageError),
    #[error(transparent)]
    Vidas(#[from] analyte_vidas::syntax::Error),
    #[error(transparent)]
    Autolyser(#[from] analyte_autolyser::syntax::Error),
    #[error(transparent)]
    Address(#[from] std::net::AddrParseError),
    #[error(transparent)]
    Integer(#[from] std::num::TryFromIntError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Timestamp(#[from] jiff::Error),
    #[error(transparent)]
    Write(#[from] super::custody::WriteError),
    #[error(transparent)]
    IntakeWait(#[from] super::custody::StoreWaitError<crate::IntakeError>),
    #[error(transparent)]
    StoreWait(#[from] super::custody::StoreWaitError<analyte_store::Error>),
    #[error(transparent)]
    Mllp(#[from] analyte_framing::hl7::Error),
    #[error(transparent)]
    VidasFrame(#[from] analyte_framing::vidas::Error),
    #[error(transparent)]
    AutolyserFrame(#[from] analyte_framing::autolyser::Error),
    #[cfg(feature = "test-hooks")]
    #[error(transparent)]
    TestHook(crate::Error),
    #[error("{original}; dialog effect failed: {effect}")]
    Cleanup {
        #[source]
        original: Box<Self>,
        effect: Box<Self>,
    },
    #[error(transparent)]
    Custody(Box<super::custody::StoreWaitError<Self>>),
    #[error("file transport interrupted; source remains claimed: {0}")]
    FileInterrupted(analyte_transport::Reason),
    #[error("LAW independent transport failed: {0}")]
    LawTransportFailed(analyte_transport::Reason),
    #[error("{reason}")]
    Deadline {
        reason: DeadlineReason,
        #[source]
        source: tokio::time::error::Elapsed,
    },
}
impl From<super::custody::StoreWaitError<Self>> for SessionError {
    fn from(error: super::custody::StoreWaitError<Self>) -> Self {
        Self::Custody(Box::new(error))
    }
}
impl super::custody::CleanupFailure for SessionError {
    fn with_effect(original: Self, effect: Self) -> Self {
        Self::Cleanup {
            original: Box::new(original),
            effect: Box::new(effect),
        }
    }
}

// Compatibility inspection for the unchanged file-custody regression assertion.
#[cfg(test)]
impl SessionError {
    pub(super) fn downcast_ref<T: std::error::Error + 'static>(&self) -> Option<&T> {
        if let Self::Store(cause) = self {
            let cause: &(dyn std::error::Error + 'static) = cause;
            return cause.downcast_ref();
        }
        if let Self::Intake(crate::IntakeError::Store(cause)) = self {
            let cause: &(dyn std::error::Error + 'static) = cause;
            return cause.downcast_ref();
        }
        None
    }
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
