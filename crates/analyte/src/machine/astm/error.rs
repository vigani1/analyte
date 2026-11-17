//! Classifies ASTM session refusals with shared link and custody causes.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    /// ASTM admission size overflow.
    #[error("ASTM admission size overflow")]
    AdmissionSizeOverflow,
    /// ASTM message bound exceeded.
    #[error("ASTM message bound exceeded")]
    MessageBoundExceeded,
    /// ASTM storage or resource limit refused transfer.
    #[error("ASTM storage or resource limit refused transfer")]
    ResourceLimitRefusedTransfer,
    /// A frame commit failed; retains the storage failure or elapsed custody budget.
    #[error("ASTM storage or resource limit refused transfer")]
    CommitRefused(#[source] crate::machine::custody::StoreWaitError<analyte_store::IntakeError>),
    /// The dialog expired before receiving a durable frame-commit completion.
    #[error("ASTM storage or resource limit refused transfer")]
    CommitDeadline,
    /// ASTM transfer wire-byte bound exceeded.
    #[error("ASTM transfer wire-byte bound exceeded")]
    TransferWireByteBoundExceeded,
    /// Order poll deadline overflow.
    #[error("order poll deadline overflow")]
    OrderPollDeadlineOverflow,
    /// An outbound command could not represent the mapped order.
    #[error(transparent)]
    OrderMapping(#[from] analyte_mapping::OrderMappingError),
    /// A received ASTM frame failed envelope validation.
    #[error(transparent)]
    Frame(#[from] analyte_framing::astm::Error),
    /// Order reservation or completion could not be persisted.
    #[error(transparent)]
    Orders(#[from] crate::orders::OrderError),
    /// The ASTM conversation rejected the current event.
    #[error(transparent)]
    Dialog(#[from] analyte_dialog::astm::Error),
    /// ASTM record syntax could not be decoded.
    #[error(transparent)]
    Syntax(#[from] analyte_astm::Error),
    /// Retains link, store and evidence causes from [`crate::machine::receive::error::Error`],
    /// uncertain custody and independent cleanup failures in their original order.
    #[error(transparent)]
    Refusal(#[from] crate::machine::custody::Refusal<Self>),
}

impl<C> From<C> for Error
where
    crate::machine::receive::error::Error: From<C>,
{
    fn from(cause: C) -> Self {
        Self::Refusal(crate::machine::custody::Refusal::Evidence(Box::new(
            crate::machine::receive::error::Error::from(cause),
        )))
    }
}
impl From<crate::machine::custody::StoreWaitError<Self>> for Error {
    fn from(cause: crate::machine::custody::StoreWaitError<Self>) -> Self {
        Self::Refusal(crate::machine::custody::Refusal::Custody(Box::new(cause)))
    }
}
impl crate::machine::custody::CleanupFailure for Error {
    fn with_effect(original: Self, effect: Self) -> Self {
        Self::Refusal(crate::machine::custody::Refusal::Cleanup(Box::new(
            crate::machine::custody::Cleanup { original, effect },
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error as _;

    #[tokio::test(start_paused = true)]
    async fn adr0003_astm_refusal_preserves_custody_cause_and_operator_text() {
        use crate::machine::custody::{StoreWaitError, store_within};
        let storage = Error::CommitRefused(StoreWaitError::Operation(
            analyte_store::IntakeError::Storage(analyte_store::StorageError::Capacity),
        ));
        let expired = store_within(
            std::time::Duration::from_millis(1),
            std::future::pending::<Result<(), analyte_store::IntakeError>>(),
        )
        .await
        .unwrap_err();
        let timeout = Error::CommitRefused(expired);
        assert!(matches!(
            &storage,
            Error::CommitRefused(StoreWaitError::Operation(
                analyte_store::IntakeError::Storage(analyte_store::StorageError::Capacity)
            ))
        ));
        assert!(matches!(
            &timeout,
            Error::CommitRefused(StoreWaitError::Timeout(_))
        ));
        assert!(storage.source().is_some());
        assert!(timeout.source().is_some());
        for error in [
            storage,
            timeout,
            Error::CommitDeadline,
            Error::ResourceLimitRefusedTransfer,
        ] {
            assert_eq!(
                error.to_string(),
                "ASTM storage or resource limit refused transfer"
            );
        }
    }
}
