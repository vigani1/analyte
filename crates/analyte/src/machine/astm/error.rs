//! Classifies ASTM session refusals and order failures with their shared causes.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    /// A query arrived without its transfer start evidence.
    #[error("ASTM query transfer start is missing")]
    QueryStartMissing,
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
    #[error("ASTM frame commit refused: {0}")]
    CommitRefused(
        #[source] crate::machine::receive::custody::StoreWaitError<analyte_store::IntakeError>,
    ),
    /// The dialog expired before receiving a durable frame-commit completion.
    #[error("ASTM frame commit completion deadline expired")]
    CommitDeadline,
    /// ASTM transfer wire-byte bound exceeded.
    #[error("ASTM transfer wire-byte bound exceeded")]
    TransferWireByteBoundExceeded,
    /// A received ASTM frame failed envelope validation.
    #[error(transparent)]
    Frame(#[from] analyte_framing::astm::Error),
    /// The query deadline could not be represented.
    #[error("ASTM query deadline overflow")]
    QueryDeadlineOverflow,
    /// The specimen selector uses escapes outside the supported subset.
    #[error("ASTM specimen selector contains unsupported escapes")]
    UnsupportedSpecimenEscapes,
    /// The query response exceeded the configured byte limit.
    #[error("ASTM query response byte limit exceeded")]
    QueryResponseTooLarge,
    /// ASTM order mapping failed.
    #[error(transparent)]
    Mapping(#[from] analyte_mapping::astm::OrderError),
    /// Shared order lookup, storage or settlement failed.
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
    Refusal(#[from] crate::machine::receive::custody::Refusal<Self>),
}

crate::machine::receive::custody::refusal_conversions!(Error);

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error as _;

    // ADR 0003
    #[tokio::test(start_paused = true)]
    async fn refusal_keeps_custody_cause_and_operator_text() {
        use crate::machine::receive::custody::{StoreWaitError, store_within};
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
        assert!(
            storage
                .to_string()
                .starts_with("ASTM frame commit refused: ")
        );
        assert!(
            timeout
                .to_string()
                .starts_with("ASTM frame commit refused: ")
        );
        assert_ne!(storage.to_string(), timeout.to_string());
        assert_eq!(
            Error::CommitDeadline.to_string(),
            "ASTM frame commit completion deadline expired"
        );
        assert_eq!(
            Error::ResourceLimitRefusedTransfer.to_string(),
            "ASTM storage or resource limit refused transfer"
        );
    }
}
