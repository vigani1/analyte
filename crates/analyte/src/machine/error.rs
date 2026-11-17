//! Combines protocol and link failures at the machine worker orchestration boundary.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    /// The ASTM session refused further processing.
    #[error(transparent)]
    Astm(Box<super::astm::Error>),
    /// The HL7 session refused further processing.
    #[error(transparent)]
    Hl7(Box<super::hl7::Error>),
    /// The LAW session refused further processing.
    #[error(transparent)]
    Law(Box<super::law::Error>),
    /// The POCT session refused further processing.
    #[error(transparent)]
    Poct(Box<super::poct::Error>),
    /// The VIDAS session refused further processing.
    #[error(transparent)]
    Vidas(Box<super::vidas::Error>),
    /// The AUTOLYSER session refused further processing.
    #[error(transparent)]
    Autolyser(Box<super::autolyser::Error>),
    /// A transport owner or shutdown operation failed.
    #[error(transparent)]
    Link(#[from] super::link::error::Error),
    /// Connection custody or retained evidence failed.
    #[error(transparent)]
    Evidence(#[from] super::receive::error::Error),
    /// A bounded synthetic gate stopped or could not resume the connection.
    #[cfg(feature = "test-hooks")]
    #[error(transparent)]
    TestHook(crate::runtime::test_hooks::Error),
    /// Retains both the triggering failure and an independent cleanup failure.
    #[error("{original}; dialog effect failed: {effect}")]
    Cleanup {
        #[source]
        original: Box<Self>,
        effect: Box<Self>,
    },
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
impl crate::machine::custody::CleanupFailure for Error {
    fn with_effect(original: Self, effect: Self) -> Self {
        Self::Cleanup {
            original: Box::new(original),
            effect: Box::new(effect),
        }
    }
}

impl From<super::astm::Error> for Error {
    fn from(error: super::astm::Error) -> Self {
        Self::Astm(Box::new(error))
    }
}

impl From<super::hl7::Error> for Error {
    fn from(error: super::hl7::Error) -> Self {
        Self::Hl7(Box::new(error))
    }
}

impl From<super::law::Error> for Error {
    fn from(error: super::law::Error) -> Self {
        Self::Law(Box::new(error))
    }
}

impl From<super::poct::Error> for Error {
    fn from(error: super::poct::Error) -> Self {
        Self::Poct(Box::new(error))
    }
}

impl From<super::vidas::Error> for Error {
    fn from(error: super::vidas::Error) -> Self {
        Self::Vidas(Box::new(error))
    }
}

impl From<super::autolyser::Error> for Error {
    fn from(error: super::autolyser::Error) -> Self {
        Self::Autolyser(Box::new(error))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn protocol_errors_remain_distinct_at_worker_boundary() {
        let astm = Error::from(super::super::astm::Error::AstmMessageBoundExceeded);
        assert!(
            matches!(astm, Error::Astm(error) if matches!(*error, super::super::astm::Error::AstmMessageBoundExceeded))
        );
        let hl7 = Error::from(super::super::hl7::Error::Hl7MessageRefused);
        assert!(
            matches!(hl7, Error::Hl7(error) if matches!(*error, super::super::hl7::Error::Hl7MessageRefused))
        );
        let law = Error::from(super::super::law::Error::LawInputStillAwaitsCustody);
        assert!(
            matches!(law, Error::Law(error) if matches!(*error, super::super::law::Error::LawInputStillAwaitsCustody))
        );
        let poct = Error::from(super::super::poct::Error::PoctMessageLimitExceeded);
        assert!(
            matches!(poct, Error::Poct(error) if matches!(*error, super::super::poct::Error::PoctMessageLimitExceeded))
        );
        let vidas = Error::from(super::super::vidas::Error::VidasControlSuffixMismatch);
        assert!(
            matches!(vidas, Error::Vidas(error) if matches!(*error, super::super::vidas::Error::VidasControlSuffixMismatch))
        );
        let autolyser = Error::from(super::super::autolyser::Error::AutolyserReportReceiptMissing);
        assert!(
            matches!(autolyser, Error::Autolyser(error) if matches!(*error, super::super::autolyser::Error::AutolyserReportReceiptMissing))
        );
    }
    #[test]
    fn shared_link_and_custody_keep_concrete_recovery_causes() {
        let link =
            super::super::link::error::Error::from(analyte_transport::Error::Deadline("write all"));
        assert!(matches!(
            link,
            super::super::link::error::Error::Transport(analyte_transport::Error::Deadline(
                "write all"
            ))
        ));
        let custody = super::super::receive::error::Error::from(analyte_store::Error::Capacity);
        assert!(matches!(
            custody,
            super::super::receive::error::Error::Store(analyte_store::Error::Capacity)
        ));
        let source = Error::from(std::io::Error::other("synthetic owner failure"));
        assert_eq!(source.to_string(), "synthetic owner failure");
    }
}
