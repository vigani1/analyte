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
    /// The owned transport or journaled input failed.
    #[error(transparent)]
    Link(#[from] crate::machine::link::error::Error),
    /// Durable storage or retained evidence could not complete.
    #[error(transparent)]
    Evidence(#[from] crate::machine::receive::error::Error),
    /// Retains both the triggering failure and an independent cleanup failure.
    #[error("{original}; dialog effect failed: {effect}")]
    Cleanup {
        #[source]
        original: Box<Self>,
        effect: Box<Self>,
    },
    /// The bounded session custody operation failed; its original cause is retained.
    #[error(transparent)]
    Custody(Box<crate::machine::custody::StoreWaitError<Self>>),
}
impl From<crate::machine::custody::StoreWaitError<Self>> for Error {
    fn from(error: crate::machine::custody::StoreWaitError<Self>) -> Self {
        Self::Custody(Box::new(error))
    }
}
impl crate::machine::custody::CleanupFailure for Error {
    fn with_effect(original: Self, effect: Self) -> Self {
        Self::Cleanup {
            original: Box::new(original),
            effect: Box::new(effect),
        }
    }
}
impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::Link(error.into())
    }
}
impl From<analyte_transport::Error> for Error {
    fn from(error: analyte_transport::Error) -> Self {
        Self::Link(error.into())
    }
}
impl From<std::net::AddrParseError> for Error {
    fn from(error: std::net::AddrParseError) -> Self {
        Self::Link(error.into())
    }
}
impl From<analyte_transport::folder::WatchError> for Error {
    fn from(error: analyte_transport::folder::WatchError) -> Self {
        Self::Link(error.into())
    }
}
impl From<analyte_store::Error> for Error {
    fn from(error: analyte_store::Error) -> Self {
        Self::Evidence(error.into())
    }
}
impl From<crate::IntakeError> for Error {
    fn from(error: crate::IntakeError) -> Self {
        Self::Evidence(error.into())
    }
}
impl From<crate::machine::interpretation::ReadError> for Error {
    fn from(error: crate::machine::interpretation::ReadError) -> Self {
        Self::Evidence(error.into())
    }
}
impl From<analyte_model::TextError> for Error {
    fn from(error: analyte_model::TextError) -> Self {
        Self::Evidence(error.into())
    }
}
impl From<analyte_model::ListError> for Error {
    fn from(error: analyte_model::ListError) -> Self {
        Self::Evidence(error.into())
    }
}
impl From<analyte_config::Error> for Error {
    fn from(error: analyte_config::Error) -> Self {
        Self::Evidence(error.into())
    }
}
impl From<std::num::TryFromIntError> for Error {
    fn from(error: std::num::TryFromIntError) -> Self {
        Self::Evidence(error.into())
    }
}
impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        Self::Evidence(error.into())
    }
}
impl From<jiff::Error> for Error {
    fn from(error: jiff::Error) -> Self {
        Self::Evidence(error.into())
    }
}
impl From<analyte_mapping::read::Error> for Error {
    fn from(error: analyte_mapping::read::Error) -> Self {
        Self::Evidence(error.into())
    }
}
impl From<crate::machine::custody::WriteError> for Error {
    fn from(error: crate::machine::custody::WriteError) -> Self {
        Self::Evidence(error.into())
    }
}
impl From<crate::machine::custody::StoreWaitError<crate::IntakeError>> for Error {
    fn from(error: crate::machine::custody::StoreWaitError<crate::IntakeError>) -> Self {
        Self::Evidence(error.into())
    }
}
impl From<crate::machine::custody::StoreWaitError<analyte_store::Error>> for Error {
    fn from(error: crate::machine::custody::StoreWaitError<analyte_store::Error>) -> Self {
        Self::Evidence(error.into())
    }
}
