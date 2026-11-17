//! Classifies startup, backup, restore and finite-retention failures.
use crate::{SCHEMA_VERSION, StartupStep};
storage_boundary! {
/// Failures of store maintenance, preserving migration and timeout causes.
#[derive(Debug, thiserror::Error)]
pub enum MaintenanceError {
    /// The host cannot represent a storage operation deadline.
    #[error("storage operation deadline overflow")]
    DeadlineOverflow,











    /// Another process currently owns a backup or retention maintenance operation.
    #[error("retention maintenance is busy; retry after the active batch or backup")]
    MaintenanceBusy,
    /// An existing SQLite file needs one offline conversion before finite retention.
    #[error("retention requires incremental vacuum; stop the service and run prepare-retention")]
    RetentionVacuumRequired,
    /// A finite policy requires the original host credential.
    #[error("retention digest key is required")]
    RetentionKeyRequired,
    /// Changing the digest key would lose duplicate recognition.
    #[error("retention digest key fingerprint mismatch; key rotation is unsupported")]
    RetentionKeyMismatch,
    /// Startup exceeded its budget; the active migration transaction is rolled back.
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
    /// A legacy row cannot be converted without guessing; migration is rolled back.
    #[error("cannot migrate {table} row {row}: {cause}")]
    MigrationEvidence {
        /// Table containing the refused legacy record.
        table: &'static str,
        /// Stable row identity retained for diagnosis.
        row: i64,
        /// Specific decoding, bound or storage failure.
        #[source]
        cause: Box<MaintenanceError>,
    },
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
    /// Migration could not decode legacy machine-order ownership evidence.
    #[error(transparent)]
    OrderEvidence(#[from] crate::OrderError),

}
}
impl From<MaintenanceError> for crate::Error {
    fn from(error: MaintenanceError) -> Self {
        match error {
            MaintenanceError::DeadlineOverflow => Self::DeadlineOverflow,
            MaintenanceError::StorageCorruption(cause) => Self::StorageCorruption(cause),
            MaintenanceError::Capacity => Self::Capacity,
            MaintenanceError::Corruption => Self::Corruption,
            MaintenanceError::DiskFull(cause) => Self::DiskFull(cause),
            MaintenanceError::LimitExceeded { what } => Self::LimitExceeded { what },
            MaintenanceError::Busy(cause) => Self::Busy(cause),
            MaintenanceError::ConstraintViolation(cause) => Self::ConstraintViolation(cause),
            MaintenanceError::Text(cause) => Self::Text(cause),
            MaintenanceError::Io(cause) => Self::Io(cause),
            MaintenanceError::Sqlite(cause) => Self::Sqlite(cause),
            MaintenanceError::Json(cause) => Self::Json(cause),
            MaintenanceError::MaintenanceBusy => Self::MaintenanceBusy,
            MaintenanceError::RetentionVacuumRequired => Self::RetentionVacuumRequired,
            MaintenanceError::RetentionKeyRequired => Self::RetentionKeyRequired,
            MaintenanceError::RetentionKeyMismatch => Self::RetentionKeyMismatch,
            MaintenanceError::StartupTimeout { step } => Self::StartupTimeout { step },
            MaintenanceError::InvalidLimits => Self::InvalidLimits,
            MaintenanceError::InUse => Self::InUse,
            MaintenanceError::SchemaTooNew(cause) => Self::SchemaTooNew(cause),
            MaintenanceError::MigrationEvidence { table, row, cause } => Self::MigrationEvidence {
                table,
                row,
                cause: Box::new((*cause).into()),
            },
            MaintenanceError::InvalidMaintenanceInput => Self::InvalidMaintenanceInput,
            MaintenanceError::Timeout => Self::Timeout,
            MaintenanceError::AuditMismatch => Self::AuditMismatch,
            MaintenanceError::Conflict => Self::Conflict,
            MaintenanceError::UnknownReceipt => Self::UnknownReceipt,
            MaintenanceError::DigestKey(cause) => Self::DigestKey(cause),
            MaintenanceError::OrderEvidence(cause) => {
                Self::MaintenanceOrderEvidence(Box::new(cause))
            }
        }
    }
}
