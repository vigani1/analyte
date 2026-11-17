//! Classifies HL7 session refusals with shared link and custody causes.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    /// HL7 display services absent.
    #[error("HL7 display services absent")]
    Hl7DisplayServicesAbsent,
    /// HL7 host phase timed out; receipt requires reconciliation.
    #[error("HL7 host phase timed out; receipt requires reconciliation")]
    Hl7HostPhaseTimedOutReceiptRequiresReconciliation,
    /// HL7 message refused.
    #[error("HL7 message refused")]
    Hl7MessageRefused,
    /// HL7 order services absent.
    #[error("HL7 order services absent")]
    Hl7OrderServicesAbsent,
    /// HL7 pending receipt limit exceeded.
    #[error("HL7 pending receipt limit exceeded")]
    Hl7PendingReceiptLimitExceeded,
    /// HL7 query services absent.
    #[error("HL7 query services absent")]
    Hl7QueryServicesAbsent,
    /// HL7 receipt correlation missing.
    #[error("HL7 receipt correlation missing")]
    Hl7ReceiptCorrelationMissing,
    /// Display receipt absent.
    #[error("display receipt absent")]
    DisplayReceiptAbsent,
    /// Late receipt absent.
    #[error("late receipt absent")]
    LateReceiptAbsent,
    /// Order response receipt absent.
    #[error("order response receipt absent")]
    OrderResponseReceiptAbsent,
    /// Query clock overflow.
    #[error("query clock overflow")]
    QueryClockOverflow,
    /// Query receipt absent.
    #[error("query receipt absent")]
    QueryReceiptAbsent,
    /// Query write deadline overflow.
    #[error("query write deadline overflow")]
    QueryWriteDeadlineOverflow,
    /// Unexpected outgoing HL7 outcome.
    #[error("unexpected outgoing HL7 outcome")]
    UnexpectedOutgoingHl7Outcome,
    /// An outbound command could not represent the mapped order.
    #[error(transparent)]
    OrderMapping(#[from] analyte_mapping::OrderMappingError),
    /// Order reservation or completion could not be persisted.
    #[error(transparent)]
    Orders(#[from] crate::orders::OrderError),
    /// The HL7 acknowledgment conversation refused an event.
    #[error(transparent)]
    Hl7Dialog(#[from] analyte_dialog::hl7::Error),
    /// HL7 segment syntax or acknowledgment fields were invalid.
    #[error(transparent)]
    Hl7(#[from] analyte_hl7::Error),
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

crate::machine::receive::error::session_boundary!(Error);
