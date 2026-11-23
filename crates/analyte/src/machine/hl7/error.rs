//! Classifies HL7 session refusals and order failures with their shared causes.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    /// HL7 display services absent.
    #[error("HL7 display services absent")]
    DisplayServicesAbsent,
    /// HL7 host phase timed out; receipt requires reconciliation.
    #[error("HL7 host phase timed out; receipt requires reconciliation")]
    HostPhaseTimeout,
    /// HL7 message refused.
    #[error("HL7 message refused")]
    MessageRefused,
    /// HL7 order services absent.
    #[error("HL7 order services absent")]
    OrderServicesAbsent,
    /// HL7 pending receipt limit exceeded.
    #[error("HL7 pending receipt limit exceeded")]
    PendingReceiptLimitExceeded,
    /// HL7 query services absent.
    #[error("HL7 query services absent")]
    QueryServicesAbsent,
    /// HL7 receipt correlation missing.
    #[error("HL7 receipt correlation missing")]
    ReceiptCorrelationMissing,
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
    /// Unexpected outgoing HL7 outcome.
    #[error("unexpected outgoing HL7 outcome")]
    UnexpectedOutgoingHl7Outcome,
    /// Order lookup, mapping, reservation or completion failed.
    #[error(transparent)]
    Orders(#[from] OrderError),
    /// The HL7 acknowledgment conversation refused an event.
    #[error(transparent)]
    Dialog(#[from] analyte_dialog::hl7::Error),
    /// HL7 segment syntax or acknowledgment fields were invalid.
    #[error(transparent)]
    Syntax(#[from] analyte_hl7::Error),
    /// Retains link, store and evidence causes from [`crate::machine::receive::error::Error`],
    /// uncertain custody and independent cleanup failures in their original order.
    #[error(transparent)]
    Refusal(#[from] crate::machine::receive::custody::Refusal<Self>),
}

crate::machine::receive::custody::refusal_conversions!(Error);

/// Failures answering HL7 order queries or sending HL7 orders.
#[derive(Debug, thiserror::Error)]
pub(crate) enum OrderError {
    /// The query deadline could not be represented.
    #[error("HL7 query deadline overflow")]
    QueryDeadlineOverflow,
    /// The machine sent an HL7 query on a link configured only to send orders.
    #[error("HL7 query policy missing")]
    QueryPolicyMissing,
    /// HL7 order mapping failed.
    #[error(transparent)]
    Mapping(#[from] analyte_mapping::hl7::OrderError),
    /// HL7 order wire mapping failed.
    #[error(transparent)]
    WireMapping(#[from] analyte_mapping::hl7::WireError),
    /// HL7 order or response syntax was invalid.
    #[error(transparent)]
    Syntax(#[from] analyte_hl7::Error),
    /// A continuation page is no longer eligible when it is due to be written.
    #[error("HL7 continuation order can no longer be reserved")]
    DisplayPageReservation,
    /// A display page cannot fit the configured wire frame.
    #[error(transparent)]
    Framing(#[from] analyte_framing::hl7::Error),
    /// Shared order lookup, storage or settlement failed.
    #[error(transparent)]
    Shared(crate::orders::OrderError),
    /// Retains both the triggering failure and a later independent settlement failure.
    #[error(transparent)]
    Cleanup(Box<crate::machine::receive::custody::Cleanup<Self>>),
}
crate::orders::shared_conversions!(OrderError);
