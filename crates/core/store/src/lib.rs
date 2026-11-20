//! Stores durable evidence, revisions and deliveries without interpreting results or resending machine orders.
#![forbid(unsafe_code)]
mod errors;
pub use errors::{
    DeliveryError, IntakeError, MaintenanceError, OperatorError, OrderError, ReconciliationError,
    StorageError, WorkerError, WorkerStartError,
};
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

pub use alerts::{AlertKey, AlertState, AlertTransition, Undeliverable};
pub use operator::notifications::{AlertEndpointStatus, AlertEventId, AlertFailure, PendingAlert};
pub use transfers::{FrameSequence, TransferOutcome};

pub use reprocess::Reprocess;

pub use delivery_operator::{DeliveryState, StoredDelivery, UNDELIVERED_MACHINES, Undelivered};
/// Schema version this build creates and migrates to; readers wait for it
/// before using tables added by the latest migration.
pub const SCHEMA_VERSION: i64 = 15;
pub use delivery::feed::OrderRefusal;
pub use machine_orders::CANCEL_CONFLICT_REASON;
pub use machine_orders::{
    ItemDecision, ItemOutcome, MachineOrder, MachineOrderFilter, MachineOrderItem, MachineOrderKey,
    OrderItems, OrderReservation, ReservationPreflight, ReserveOptions, SendRefusal,
};

/// LAW execution custody and retained message dispositions.
pub use revisions::law;
pub use revisions::{
    InterpretationCommit, NO_RESULTS_REASON, ReconcileAction, ReconcileInput, ReconcileMode,
    RevisionOutcome, StatusLifecycle, reconciled_outcome,
};

pub use query::{
    FailingDestination, Frame, LastDelivered, ParkedCounts, ParkedFilter, ParkedItem, Receipt,
    Unfinished,
};

pub use records::{
    Attempt, Audit, CachedOrder, Claim, Delivery, DeliveryKind, FrameProvenance, Health,
    Interpretation, Lookup, OrderEventDelivery, OrderPage, OrderingKey, Parked, ParkedKind,
    PendingOrders, Reading,
};

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
mod keys;
#[cfg(test)]
mod test_support;
#[cfg(test)]
#[path = "tests.rs"]
mod tests;
use keys::key;

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
    pub(crate) fn validate(&self) -> Result<(), MaintenanceError> {
        if self.record_bytes == 0
            || self.page_rows == 0
            || self.busy_timeout.is_zero()
            || self.busy_timeout > Duration::from_secs(30)
        {
            return Err(MaintenanceError::InvalidLimits);
        }
        Ok(())
    }
}

/// One connection, owned by a dedicated worker when used from async code.
pub struct Store {
    retention_key: Option<DigestKey>,
    db: Connection,
    path: PathBuf,
    limits: Limits,
    space: Box<dyn FreeSpace>,
    // Fields drop in order: the owner lock is released only after the
    // connection has closed, including SQLite's checkpoint on close.
    _lock: std::fs::File,
}

fn document<T: Serialize>(value: &T) -> Result<String, StorageError> {
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
            return Err(std::io::Error::new(
                std::io::ErrorKind::FileTooLarge,
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
    fn encode<T: Serialize>(&self, value: &T) -> Result<String, StorageError> {
        let mut writer = BoundedJson {
            bytes: Vec::new(),
            limit: self.limits.record_bytes,
        };
        serde_json::to_writer(&mut writer, value).map_err(|error| {
            if error.io_error_kind() == Some(std::io::ErrorKind::FileTooLarge) {
                StorageError::LimitExceeded {
                    what: "record_bytes",
                }
            } else {
                error.into()
            }
        })?;
        String::from_utf8(writer.bytes)
            .map_err(|cause| std::io::Error::new(std::io::ErrorKind::InvalidData, cause).into())
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
