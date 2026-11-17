//! Classifies atomic interpretation, revision assignment and audited reprocessing failures.
storage_boundary! {
    /// Failures of reconciliation; a failed builder rolls back revisions and deliveries together.
    #[derive(Debug, thiserror::Error)]
    pub enum ReconciliationError {
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
}
impl From<ReconciliationError> for crate::Error {
    fn from(error: ReconciliationError) -> Self {
        match error {
            ReconciliationError::Worker(cause) => Self::Worker(cause),
            ReconciliationError::Identity(cause) => Self::Identity(cause),
            ReconciliationError::Receipt(cause) => cause.into(),
            ReconciliationError::Work(cause) => cause.into(),
            ReconciliationError::DigestKey(cause) => Self::DigestKey(cause),
            ReconciliationError::StorageCorruption(cause) => Self::StorageCorruption(cause),
            ReconciliationError::Capacity => Self::Capacity,
            ReconciliationError::Corruption => Self::Corruption,
            ReconciliationError::DiskFull(cause) => Self::DiskFull(cause),
            ReconciliationError::LimitExceeded { what } => Self::LimitExceeded { what },
            ReconciliationError::Busy(cause) => Self::Busy(cause),
            ReconciliationError::ConstraintViolation(cause) => Self::ConstraintViolation(cause),
            ReconciliationError::Text(cause) => Self::Text(cause),
            ReconciliationError::Io(cause) => Self::Io(cause),
            ReconciliationError::Sqlite(cause) => Self::Sqlite(cause),
            ReconciliationError::Json(cause) => Self::Json(cause),
            ReconciliationError::RevisionHistoryLimit => Self::RevisionHistoryLimit,
            ReconciliationError::UnknownReceipt => Self::UnknownReceipt,
            ReconciliationError::ReprocessParked => Self::ReprocessParked,
            ReconciliationError::Conflict => Self::Conflict,
            ReconciliationError::InvalidInterpretationInput => Self::InvalidInterpretationInput,
            ReconciliationError::AuditMismatch => Self::AuditMismatch,
            ReconciliationError::DeliveryBuild(cause) => Self::DeliveryBuild(cause),
            ReconciliationError::LifecyclePolicyRequired => Self::LifecyclePolicyRequired,
            ReconciliationError::ConflictEvidenceRequired => Self::ConflictEvidenceRequired,
        }
    }
}
