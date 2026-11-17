//! Classifies ASTM session refusals with shared link and custody causes.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    /// ASTM admission size overflow.
    #[error("ASTM admission size overflow")]
    AstmAdmissionSizeOverflow,
    /// ASTM message bound exceeded.
    #[error("ASTM message bound exceeded")]
    AstmMessageBoundExceeded,
    /// ASTM storage or resource limit refused transfer.
    #[error("ASTM storage or resource limit refused transfer")]
    AstmResourceLimitRefusedTransfer,
    /// A frame commit failed; retains the storage failure or elapsed custody budget.
    #[error("ASTM storage or resource limit refused transfer")]
    AstmCommitRefused(#[source] crate::machine::custody::StoreWaitError<analyte_store::Error>),
    /// The dialog expired before receiving a durable frame-commit completion.
    #[error("ASTM storage or resource limit refused transfer")]
    AstmCommitDeadline,
    /// ASTM transfer wire-byte bound exceeded.
    #[error("ASTM transfer wire-byte bound exceeded")]
    AstmTransferWireByteBoundExceeded,
    /// Order poll deadline overflow.
    #[error("order poll deadline overflow")]
    OrderPollDeadlineOverflow,
    /// An outbound command could not represent the mapped order.
    #[error(transparent)]
    OrderMapping(#[from] analyte_mapping::OrderMappingError),
    /// A received ASTM frame failed envelope validation.
    #[error(transparent)]
    AstmFrame(#[from] analyte_framing::astm::Error),
    /// Order reservation or completion could not be persisted.
    #[error(transparent)]
    Orders(#[from] crate::orders::OrderError),
    /// The ASTM conversation rejected the current event.
    #[error(transparent)]
    AstmDialog(#[from] analyte_dialog::astm::Error),
    /// ASTM record syntax could not be decoded.
    #[error(transparent)]
    Astm(#[from] analyte_astm::Error),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error as _;

    #[tokio::test(start_paused = true)]
    async fn adr0003_astm_refusal_preserves_custody_cause_and_operator_text() {
        use crate::machine::custody::{StoreWaitError, store_within};
        let storage =
            Error::AstmCommitRefused(StoreWaitError::Operation(analyte_store::Error::Capacity));
        let expired = store_within(
            std::time::Duration::from_millis(1),
            std::future::pending::<Result<(), analyte_store::Error>>(),
        )
        .await
        .unwrap_err();
        let timeout = Error::AstmCommitRefused(expired);
        assert!(matches!(
            &storage,
            Error::AstmCommitRefused(StoreWaitError::Operation(analyte_store::Error::Capacity))
        ));
        assert!(matches!(
            &timeout,
            Error::AstmCommitRefused(StoreWaitError::Timeout(_))
        ));
        assert!(storage.source().is_some());
        assert!(timeout.source().is_some());
        for error in [
            storage,
            timeout,
            Error::AstmCommitDeadline,
            Error::AstmResourceLimitRefusedTransfer,
        ] {
            assert_eq!(
                error.to_string(),
                "ASTM storage or resource limit refused transfer"
            );
        }
    }
}
