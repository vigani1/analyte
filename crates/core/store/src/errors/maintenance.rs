//! Classifies startup, backup, restore and finite-retention failures.
use crate::StorageError;
use crate::{SCHEMA_VERSION, StartupStep};
/// Failures of store maintenance, preserving schema and timeout causes.
#[derive(Debug, thiserror::Error)]
pub enum MaintenanceError {
    /// A persistence primitive failed; its recovery classification is retained.
    #[error(transparent)]
    Storage(#[from] StorageError),
    /// Retained evidence failed an application integrity check.
    #[error("stored evidence is corrupt")]
    Corruption,
    /// The worker could not establish completion of this operation.
    #[error("{0}")]
    Worker(#[from] crate::WorkerError),
    /// Protected receipt evidence could not be read for a maintenance decision.
    #[error(transparent)]
    Receipt(#[from] crate::OperatorError),
    /// The host cannot represent a storage operation deadline.
    #[error("storage operation deadline overflow")]
    DeadlineOverflow,
    /// Another process currently owns a backup or retention maintenance operation.
    #[error("retention maintenance is busy; retry after the active batch or backup")]
    MaintenanceBusy,
    /// The SQLite file cannot return space incrementally, so finite retention is refused.
    #[error("retention requires a store created with incremental vacuum")]
    RetentionVacuumRequired,
    /// A finite policy requires the original host credential.
    #[error("retention digest key is required")]
    RetentionKeyRequired,
    /// Changing the digest key would lose duplicate recognition.
    #[error("retention digest key fingerprint mismatch; key rotation is unsupported")]
    RetentionKeyMismatch,
    /// Startup exceeded its budget; the active schema transaction is rolled back.
    #[error("store startup deadline expired during {step}")]
    StartupTimeout {
        /// The operation active when the deadline expired.
        step: StartupStep,
    },
    /// Limits cannot establish bounded store operations.
    #[error("invalid storage limits")]
    InvalidLimits,
    /// Another owner prevents exclusive restore access.
    #[error("store is in use; stop the service before restoring")]
    InUse,
    /// The database belongs to a newer implementation.
    #[error("schema version {0} is newer than supported version {SCHEMA_VERSION}")]
    SchemaTooNew(i64),
    /// The database predates the starting schema of the first release.
    #[error(
        "schema version {0} is unsupported; the oldest supported version is {oldest}",
        oldest = crate::upkeep::migration::STARTING_VERSION
    )]
    SchemaTooOld(i64),
    /// Restore, retention or startup input cannot authorize maintenance.
    #[error("invalid maintenance input")]
    InvalidMaintenanceInput,
    /// A bounded operation expired; a submitted write may have committed.
    #[error("storage deadline expired; reconcile using the original identity")]
    Timeout,
    /// An operator action or target does not match the operation being authorized.
    #[error("audit action or target mismatch")]
    AuditMismatch,
    /// An immutable identity or state transition conflicted.
    #[error("identity or state constraint conflict")]
    Conflict,
    /// Requested receipt has never been stored.
    #[error("receipt does not exist")]
    UnknownReceipt,
    /// A keyed retention digest could not be initialized.
    #[error("retention digest key rejected")]
    DigestKey(#[from] hmac::digest::InvalidLength),
    /// Retained order evidence could not be decoded for a maintenance decision.
    #[error(transparent)]
    OrderEvidence(#[from] crate::OrderError),
}

impl From<rusqlite::Error> for MaintenanceError {
    fn from(error: rusqlite::Error) -> Self {
        StorageError::from(error).into()
    }
}
impl From<std::io::Error> for MaintenanceError {
    fn from(error: std::io::Error) -> Self {
        StorageError::from(error).into()
    }
}
impl From<serde_json::Error> for MaintenanceError {
    fn from(error: serde_json::Error) -> Self {
        StorageError::from(error).into()
    }
}
