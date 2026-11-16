//! Stores durable evidence, revisions and deliveries without interpreting results or resending machine orders.
#![forbid(unsafe_code)]
mod operator;
use operator::{alerts, parked, query};
mod upkeep;
pub use upkeep::retention::{DigestKey, HoldTarget, PurgeRun, RetentionHold};
use upkeep::{startup, worker};
mod orders;
use orders::{machine_orders, polls};
mod delivery;
use delivery::operator as delivery_operator;
mod results;
use results::revisions;
mod intake;
use intake::{records, reprocess, transfers};

pub use startup::{DEFAULT_STARTUP_TIMEOUT, StartupStep};

pub use polls::{PollIntent, PollIntentId, PollIntentState, PollOutcome};

mod sql_types;

pub use alerts::*;
pub use operator::notifications::*;
pub use transfers::{FrameSequence, TransferOutcome};

pub use reprocess::Reprocess;

pub use delivery_operator::{DeliveryState, StoredDelivery};
/// Schema version this build creates and migrates to; readers wait for it
/// before using tables added by the latest migration.
pub const SCHEMA_VERSION: i64 = 14;
pub use machine_orders::*;

pub use revisions::law::*;
pub use revisions::{InterpretationCommit, RevisionOutcome, StatusLifecycle};

pub use query::{Frame, ParkedItem, Receipt};

pub use records::*;

pub use worker::Handle;

use analyte_model::Digest;
#[cfg(test)]
use analyte_model::{DeliveryId, Identifier, ReceiptId, ReceiptMetadata, SampleId};
use rusqlite::Connection;
#[cfg(test)]
use rusqlite::params;
use serde::Serialize;
use sha2::{Digest as _, Sha256};
#[cfg(test)]
use std::time::Instant;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

/// Recovery decisions remain distinct from ordinary database failures.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Analyzer cancellation requires visible reconciliation before any new cancel admission.
    #[error("AWOS is analyzer-cancelled; app cancellation intent requires reconciliation")]
    AnalyzerCancelled,

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
    /// Purged evidence can no longer be replayed.
    #[error("item was purged by retention; reprocessing and resending are unavailable")]
    Purged,
    /// Startup exceeded its budget; the active migration transaction is rolled back.
    #[error("store startup deadline expired during {step}")]
    StartupTimeout {
        /// The operation active when the deadline expired.
        step: StartupStep,
    },
    /// Legacy content comparison exceeded the configured reconciliation row bound.
    #[error("observation history comparison exceeds page_rows; operator reconciliation required")]
    RevisionHistoryLimit,
    /// Requested receipt has never been stored.
    #[error("receipt does not exist")]
    UnknownReceipt,
    /// Parked item does not exist, is already resolved or belongs to other evidence.
    #[error("parked item is missing, resolved or unrelated to this receipt")]
    ReprocessParked,

    /// No delivery exists with the requested identity.
    #[error("delivery does not exist")]
    UnknownDelivery,
    /// Only completed, parked or setup-paused deliveries can be resent.
    #[error("delivery state does not permit resend")]
    ResendState,
    /// A newer observation revision exists; explicitly name the old delivery to resend it.
    #[error("delivery is superseded; explicit operator acknowledgement required")]
    SupersededDelivery,
    /// Another owner prevents exclusive restore access.
    #[error("store is in use; stop the service before restoring")]
    InUse,
    /// The configured storage reserve was reached.
    #[error("storage capacity reserve reached")]
    Capacity,
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
        cause: Box<Error>,
    },
    /// A previous poll may already have moved this machine's cursor.
    #[error("poll intent remains unresolved; reconcile before polling again")]
    PollIntentUnresolved,
    /// No retained poll has the requested identity.
    #[error("poll intent does not exist")]
    UnknownPollIntent,
    /// SQLite or stored evidence failed an integrity check.
    #[error("stored evidence is corrupt")]
    Corruption,
    /// An immutable identity or state transition conflicted.
    #[error("identity or state constraint conflict")]
    Conflict,
    /// The model rejected a machine-order lifecycle transition.
    #[error("machine-order lifecycle transition rejected: {0}")]
    OrderTransition(#[from] analyte_model::TransitionError),
    /// SQLite reported a full disk.
    #[error("disk full")]
    DiskFull,
    /// A bounded operation expired; a submitted write may have committed.
    #[error("storage deadline expired; reconcile using the original identity")]
    Timeout,
    /// The dedicated worker stopped.
    #[error("storage worker stopped")]
    Closed,
    /// The generated local command identity exceeds the caller's byte budget.
    #[error("command identity exceeds caller byte limit")]
    ItemIdentityTooLong,
    /// Input exceeds a configured bound or violates its declared provenance.
    #[error("invalid or oversized storage input")]
    InvalidInput,
    /// A filesystem operation failed.
    #[error("filesystem operation failed")]
    Io(#[from] std::io::Error),
    /// SQLite failed; its original cause is preserved.
    #[error("SQLite operation failed: {0}")]
    Sqlite(rusqlite::Error),
    /// The host's transactional delivery builder refused construction; the cause is retained.
    #[error("delivery construction failed")]
    DeliveryBuild(#[source] Box<dyn std::error::Error + Send + Sync>),
    /// Typed evidence could not be encoded or decoded.
    #[error("evidence encoding failed: {0}")]
    Json(#[from] serde_json::Error),
}

impl From<rusqlite::Error> for Error {
    fn from(error: rusqlite::Error) -> Self {
        if let rusqlite::Error::SqliteFailure(code, _) = &error {
            #[expect(
                clippy::wildcard_enum_match_arm,
                reason = "SQLite error codes are external and non-exhaustive; unknown failures retain their cause"
            )]
            match code.code {
                rusqlite::ErrorCode::DiskFull => return Self::DiskFull,
                rusqlite::ErrorCode::DatabaseCorrupt | rusqlite::ErrorCode::NotADatabase => {
                    return Self::Corruption;
                }
                rusqlite::ErrorCode::ConstraintViolation => return Self::Conflict,
                rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked => {
                    return Self::Timeout;
                }
                _ => {} // Other SQLite errors retain their diagnostic cause.
            }
        }
        Self::Sqlite(error)
    }
}

/// Host-provided capacity probe; errors refuse new evidence rather than guessing.
pub trait FreeSpace: Send + 'static {
    /// Reports available bytes on the database volume.
    /// # Errors
    /// Returns the operating system's error when capacity cannot be determined.
    fn available(&self, path: &Path) -> std::io::Result<u64>;
}

/// Explicit limits; the host chooses these for its supported outage duration.
pub struct Limits {
    /// Minimum free bytes retained after admitting an input.
    pub reserve_bytes: u64,
    /// Maximum allocated SQLite bytes, including free pages.
    pub database_bytes: u64,
    /// Maximum individual encoded record or raw input.
    pub record_bytes: usize,
    /// Maximum rows returned or accepted in one operation.
    pub page_rows: usize,
    /// SQLite lock wait, bounded independently of the caller's deadline.
    pub busy_timeout: Duration,
}

impl Limits {
    /// The one admission rule for limits, shared by open and restore so neither
    /// can publish a store with bounds the other refuses.
    pub(crate) fn validate(&self) -> Result<(), Error> {
        if self.record_bytes == 0
            || self.page_rows == 0
            || self.busy_timeout.is_zero()
            || self.busy_timeout > Duration::from_secs(30)
        {
            return Err(Error::InvalidInput);
        }
        Ok(())
    }
}

/// One connection, owned by a dedicated worker when used from async code.
pub struct Store {
    retention_key: Option<DigestKey>,
    _lock: std::fs::File,
    db: Connection,
    path: PathBuf,
    limits: Limits,
    space: Box<dyn FreeSpace>,
}

fn key<T: Serialize>(value: &T) -> Result<String, Error> {
    Ok(serde_json::to_string(value)?)
}

fn digest(bytes: &[u8]) -> Digest {
    Digest(Sha256::digest(bytes).into())
}

struct BoundedJson {
    bytes: Vec<u8>,
    limit: usize,
}

impl std::io::Write for BoundedJson {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            return Err(std::io::Error::other(
                "encoded record exceeds configured bound",
            ));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Store {
    fn encode<T: Serialize>(&self, value: &T) -> Result<String, Error> {
        let mut writer = BoundedJson {
            bytes: Vec::new(),
            limit: self.limits.record_bytes,
        };
        serde_json::to_writer(&mut writer, value)?;
        String::from_utf8(writer.bytes).map_err(|_| Error::Corruption)
    }
}

/// Authoritative cache availability, distinct from an authoritative empty answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderTrust {
    /// Complete current snapshot inside the configured age limit.
    Trusted {
        /// Seconds since the last successful poll.
        age: u64,
    },
    /// Complete snapshot whose age reaches or exceeds the configured limit.
    Stale {
        /// Seconds since the last successful poll.
        age: u64,
    },
    /// Snapshot is unfinished or invalidated by a feed reset.
    Incomplete,
    /// Restored history requires operator reconciliation.
    RestorePaused,
    /// No feed exists or clock evidence cannot establish its age.
    Unknown,
}
impl OrderTrust {
    /// Only a complete, fresh, unpaused snapshot can establish an empty answer.
    #[must_use]
    pub fn is_trusted(self) -> bool {
        matches!(self, Self::Trusted { .. })
    }
}

#[cfg(test)]
mod test_support;
#[cfg(test)]
#[path = "tests.rs"]
mod tests;
