//! Combines protocol and link failures at the machine worker orchestration boundary.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    /// Complete-file pickup cannot be driven as a stream session.
    #[error("folder stack cannot drive a stream session")]
    FolderStream,
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
    #[error(transparent)]
    Cleanup(Box<crate::machine::custody::Cleanup<Self>>),
}
impl crate::machine::custody::CleanupFailure for Error {
    fn with_effect(original: Self, effect: Self) -> Self {
        Self::Cleanup(Box::new(crate::machine::custody::Cleanup {
            original,
            effect,
        }))
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

crate::machine::link::error::forward!(Error, Self::Link);

crate::machine::receive::error::forward!(Error, Self::Evidence);
