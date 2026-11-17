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
    /// A bounded synthetic gate stopped or could not resume the connection.
    #[error(transparent)]
    TestHook(crate::runtime::test_hooks::Error),
    /// Retains uncertain custody or independent cleanup failures in their original order.
    #[error(transparent)]
    Refusal(#[from] crate::machine::custody::Refusal<Self>),
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

impl From<crate::machine::link::error::Error> for Error {
    fn from(error: crate::machine::link::error::Error) -> Self {
        Self::Refusal(crate::machine::custody::Refusal::Link(Box::new(error)))
    }
}
impl From<crate::machine::receive::error::Error> for Error {
    fn from(error: crate::machine::receive::error::Error) -> Self {
        Self::Refusal(crate::machine::custody::Refusal::Evidence(Box::new(error)))
    }
}
impl From<crate::machine::custody::StoreWaitError<Self>> for Error {
    fn from(error: crate::machine::custody::StoreWaitError<Self>) -> Self {
        Self::Refusal(crate::machine::custody::Refusal::Custody(Box::new(error)))
    }
}
impl crate::machine::custody::CleanupFailure for Error {
    fn with_effect(original: Self, effect: Self) -> Self {
        Self::Refusal(crate::machine::custody::Refusal::Cleanup(Box::new(
            crate::machine::custody::Cleanup { original, effect },
        )))
    }
}

crate::runtime::service::conversions::causes!(Error;
    crate::machine::link::error::Error => [
        std::io::Error,
        analyte_transport::Error,
        std::net::AddrParseError,
        analyte_transport::folder::WatchError,
    ],
    crate::machine::receive::error::Error => [
        analyte_store::Error,
        crate::IntakeError,
        crate::machine::interpretation::ReadError,
        analyte_model::TextError,
        analyte_model::ListError,
        analyte_config::Error,
        std::num::TryFromIntError,
        serde_json::Error,
        jiff::Error,
        crate::reading::Error,
        crate::machine::custody::WriteError,
        crate::machine::custody::StoreWaitError<crate::IntakeError>,
        crate::machine::custody::StoreWaitError<analyte_store::Error>,
    ],
    analyte_store::Error => [
        analyte_store::WorkerError,
        analyte_store::WorkerStartError,
        analyte_store::IntakeError,
        analyte_store::DeliveryError,
        analyte_store::OrderError,
        analyte_store::MaintenanceError,
        analyte_store::OperatorError,
        analyte_store::ReconciliationError,
        analyte_store::StorageError,
    ],
);
