//! Classifies atomic interpretation, revision assignment and audited reprocessing failures.
use crate::StorageError;
/// Failures of reconciliation; a failed builder rolls back revisions and deliveries together.
#[derive(Debug, thiserror::Error)]
pub enum ReconciliationError {
    /// A persistence primitive failed; its recovery classification is retained.
    #[error(transparent)]
    Storage(#[from] StorageError),
    /// Retained evidence failed an application integrity check.
    #[error("stored evidence is corrupt")]
    Corruption,
    /// The worker could not establish completion of this operation.
    #[error("{0}")]
    Worker(#[from] crate::WorkerError),
    /// A delivery builder supplied an invalid immutable identity.
    #[error(transparent)]
    Identity(#[from] analyte_model::InvalidUuid),
    /// Legacy content comparison exceeded the configured reconciliation row bound.
    #[error("observation history comparison exceeds page_rows; operator reconciliation required")]
    RevisionHistoryLimit,
    /// Requested receipt has never been stored.
    #[error("receipt does not exist")]
    UnknownReceipt,
    /// Parked item does not exist, is already resolved or belongs to other evidence.
    #[error("parked item is missing, resolved or unrelated to this receipt")]
    ReprocessParked,
    /// The protected comparison is stale or ineligible.
    #[error("measurement comparison changed or is no longer eligible; open the item again")]
    ReviewChanged,
    /// An immutable identity or state transition conflicted.
    #[error("identity or state constraint conflict")]
    Conflict,
    /// Interpretation input does not agree with its receipt or result identity.
    #[error("invalid interpretation input")]
    InvalidInterpretationInput,
    /// An operator action or target does not match the operation being authorized.
    #[error("audit action or target mismatch")]
    AuditMismatch,
    /// The host's transactional delivery builder refused construction; the cause is retained.
    #[error("delivery construction failed")]
    DeliveryBuild(#[source] Box<dyn std::error::Error + Send + Sync>),
    /// A required LAW lifecycle policy was not supplied with custody.
    #[error("custody lifecycle policy required")]
    LifecyclePolicyRequired,
    /// An atomic result conflict has no retained rejection interpretation.
    #[error("atomic conflict evidence required")]
    ConflictEvidenceRequired,
    /// Protected receipt evidence could not be read before reconciliation.
    #[error(transparent)]
    Receipt(#[from] crate::OperatorError),
    /// Retained machine work could not be correlated for execution custody.
    #[error(transparent)]
    Work(#[from] crate::OrderError),
    /// Keyed duplicate evidence could not be constructed.
    #[error("retention digest key rejected")]
    DigestKey(#[from] hmac::digest::InvalidLength),
}

impl From<rusqlite::Error> for ReconciliationError {
    fn from(error: rusqlite::Error) -> Self {
        StorageError::from(error).into()
    }
}
impl From<std::io::Error> for ReconciliationError {
    fn from(error: std::io::Error) -> Self {
        StorageError::from(error).into()
    }
}
impl From<serde_json::Error> for ReconciliationError {
    fn from(error: serde_json::Error) -> Self {
        StorageError::from(error).into()
    }
}
