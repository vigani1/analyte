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
    /// Connection custody or retained evidence failed.
    #[error(transparent)]
    Evidence(super::receive::error::Error),
    /// A bounded synthetic gate stopped or could not resume the connection.
    #[error(transparent)]
    TestHook(crate::runtime::test_hooks::Error),
    /// Retains both the triggering failure and an independent cleanup failure.
    #[error(transparent)]
    Cleanup(Box<crate::machine::receive::custody::Cleanup<Self>>),
}
impl crate::machine::receive::custody::CleanupFailure for Error {
    fn with_effect(original: Self, effect: Self) -> Self {
        Self::Cleanup(Box::new(crate::machine::receive::custody::Cleanup {
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

impl<C> From<C> for Error
where
    super::receive::error::Error: From<C>,
{
    fn from(cause: C) -> Self {
        Self::Evidence(super::receive::error::Error::from(cause))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn protocol_errors_remain_distinct_at_worker_boundary() {
        let astm = Error::from(super::super::astm::Error::MessageBoundExceeded);
        assert!(
            matches!(astm, Error::Astm(error) if matches!(*error, super::super::astm::Error::MessageBoundExceeded))
        );
        let hl7 = Error::from(super::super::hl7::Error::MessageRefused);
        assert!(
            matches!(hl7, Error::Hl7(error) if matches!(*error, super::super::hl7::Error::MessageRefused))
        );
        let law = Error::from(super::super::law::Error::ConnectionDeadlineOverflow);
        assert!(
            matches!(law, Error::Law(error) if matches!(*error, super::super::law::Error::ConnectionDeadlineOverflow))
        );
        let poct = Error::from(super::super::poct::Error::MessageLimitExceeded);
        assert!(
            matches!(poct, Error::Poct(error) if matches!(*error, super::super::poct::Error::MessageLimitExceeded))
        );
        let vidas = Error::from(super::super::vidas::Error::ControlSuffixMismatch);
        assert!(
            matches!(vidas, Error::Vidas(error) if matches!(*error, super::super::vidas::Error::ControlSuffixMismatch))
        );
        let autolyser = Error::from(super::super::autolyser::Error::ReportReceiptMissing);
        assert!(
            matches!(autolyser, Error::Autolyser(error) if matches!(*error, super::super::autolyser::Error::ReportReceiptMissing))
        );
    }
    #[test]
    fn shared_link_and_custody_keep_concrete_recovery_causes() {
        let link = super::super::link::error::Error::from(analyte_transport::Error::Deadline(
            analyte_transport::DeadlineKind::WriteAll,
        ));
        assert!(matches!(
            link,
            super::super::link::error::Error::Transport(analyte_transport::Error::Deadline(
                analyte_transport::DeadlineKind::WriteAll
            ))
        ));
        let custody = super::super::receive::error::Error::from(
            analyte_store::IntakeError::Storage(analyte_store::StorageError::Capacity),
        );
        assert!(matches!(
            custody,
            super::super::receive::error::Error::Store(analyte_store::IntakeError::Storage(
                analyte_store::StorageError::Capacity
            ))
        ));
        let source = Error::from(std::io::Error::other("synthetic owner failure"));
        assert_eq!(source.to_string(), "synthetic owner failure");
    }
}
