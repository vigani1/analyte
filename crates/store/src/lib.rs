//! Durable evidence and transactional delivery state. No payloads are logged.
#![forbid(unsafe_code)]

mod migration;
mod query;
pub use query::{Frame, ParkedItem, Receipt};
mod records;
pub use records::*;
mod worker;
pub use worker::Handle;

use analyte_model::{DeliveryId, Digest, Identifier, ReceiptId, ReceiptMetadata, SampleId};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use sha2::{Digest as _, Sha256};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

/// Recovery decisions remain distinct from ordinary database failures.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The configured storage reserve was reached.
    #[error("storage capacity reserve reached")]
    Capacity,
    /// The database belongs to a newer implementation.
    #[error("schema version {0} is newer than supported version 2")]
    SchemaTooNew(i64),
    /// SQLite or stored evidence failed an integrity check.
    #[error("stored evidence is corrupt")]
    Corruption,
    /// An immutable identity or state transition conflicted.
    #[error("identity or state constraint conflict")]
    Conflict,
    /// SQLite reported a full disk.
    #[error("disk full")]
    DiskFull,
    /// A bounded operation expired; a submitted write may have committed.
    #[error("storage deadline expired; reconcile using the original identity")]
    Timeout,
    /// The dedicated worker stopped.
    #[error("storage worker stopped")]
    Closed,
    /// Input exceeds a configured bound or violates its declared provenance.
    #[error("invalid or oversized storage input")]
    InvalidInput,
    /// A filesystem operation failed.
    #[error("filesystem operation failed")]
    Io(#[from] std::io::Error),
    /// SQLite failed; its original cause is preserved.
    #[error("SQLite operation failed: {0}")]
    Sqlite(rusqlite::Error),
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

/// One connection, owned by a dedicated worker when used from async code.
pub struct Store {
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
    /// Opens, checks integrity, and applies forward-only migrations before accepting writes.
    /// # Errors
    /// Refuses corruption, newer schemas, invalid limits and filesystem/database failures.
    pub fn open(path: &Path, limits: Limits, space: Box<dyn FreeSpace>) -> Result<Self, Error> {
        if limits.record_bytes == 0
            || limits.page_rows == 0
            || limits.busy_timeout.is_zero()
            || limits.busy_timeout > Duration::from_secs(30)
        {
            return Err(Error::InvalidInput);
        }
        let mut db = Connection::open(path)?;
        db.busy_timeout(limits.busy_timeout)?;
        let version: i64 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version > 2 {
            return Err(Error::SchemaTooNew(version));
        }
        Self::integrity(&db)?;
        db.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON;",
        )?;
        if version == 0 {
            let tx = db.transaction()?;
            tx.execute_batch(include_str!("schema.sql"))?;
            tx.commit()?;
        }
        if version <= 1 {
            Self::migrate_v2(&mut db)?;
        }
        // SQL constraints also protect evidence from accidental maintenance updates.
        for table in ["receipts", "frames", "interpretations", "attempts", "audit"] {
            db.execute_batch(&format!("CREATE TRIGGER IF NOT EXISTS immutable_{table}_update BEFORE UPDATE ON {table} BEGIN SELECT RAISE(ABORT,'immutable evidence'); END; CREATE TRIGGER IF NOT EXISTS immutable_{table}_delete BEFORE DELETE ON {table} BEGIN SELECT RAISE(ABORT,'immutable evidence'); END;"))?;
        }
        Ok(Self {
            db,
            path: path.into(),
            limits,
            space,
        })
    }

    fn integrity(db: &Connection) -> Result<(), Error> {
        let result: String = db.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
        if result != "ok" {
            return Err(Error::Corruption);
        }
        let foreign_error = db.prepare("PRAGMA foreign_key_check")?.exists([])?;
        if foreign_error {
            return Err(Error::Corruption);
        }
        Ok(())
    }

    fn bound(&self, bytes: &[u8]) -> Result<(), Error> {
        if bytes.len() > self.limits.record_bytes {
            return Err(Error::InvalidInput);
        }
        Ok(())
    }

    fn capacity(&self, bytes: usize) -> Result<(), Error> {
        let input = u64::try_from(bytes).map_err(|_| Error::Capacity)?;
        let needed = self
            .limits
            .reserve_bytes
            .checked_add(input)
            .ok_or(Error::Capacity)?;
        let pages: u32 = self.db.query_row("PRAGMA page_count", [], |r| r.get(0))?;
        let size: u32 = self.db.query_row("PRAGMA page_size", [], |r| r.get(0))?;
        if self.space.available(&self.path)? <= needed
            || u64::from(pages)
                .checked_mul(u64::from(size))
                .and_then(|n| n.checked_add(input))
                .ok_or(Error::Capacity)?
                >= self.limits.database_bytes
        {
            return Err(Error::Capacity);
        }
        Ok(())
    }

    /// Commits exact bytes and effective config; supplied digests must match.
    /// # Errors
    /// Refuses insufficient capacity, oversized evidence, digest mismatches or duplicate IDs.
    pub fn receive(
        &mut self,
        metadata: &ReceiptMetadata,
        raw: &[u8],
        config: &[u8],
    ) -> Result<(), Error> {
        self.bound(raw)?;
        self.bound(config)?;
        let encoded = self.encode(metadata)?;
        self.bound(encoded.as_bytes())?;
        self.capacity(raw.len().checked_add(config.len()).ok_or(Error::Capacity)?)?;
        if digest(raw) != metadata.raw_digest || digest(config) != metadata.config_digest {
            return Err(Error::InvalidInput);
        }
        self.db.execute(
            "INSERT INTO receipts VALUES (?1,?2,?3,?4)",
            params![key(&metadata.id)?, encoded, raw, config],
        )?;
        Ok(())
    }

    /// Returns exact protected raw bytes for a receipt, or absence; never logs them.
    /// # Errors
    /// Reports database failures.
    pub fn raw(&self, id: ReceiptId) -> Result<Option<Vec<u8>>, Error> {
        Ok(self
            .db
            .query_row("SELECT raw FROM receipts WHERE id=?1", [key(&id)?], |r| {
                r.get(0)
            })
            .optional()?)
    }

    /// Commits one ASTM frame before returning its monotonic journal sequence.
    /// # Errors
    /// Refuses closed transfers, oversized frames, invalid frame numbers or capacity failures.
    pub fn frame(&mut self, transfer: &Identifier, number: u8, raw: &[u8]) -> Result<i64, Error> {
        self.bound(raw)?;
        self.capacity(raw.len())?;
        if number > 7 {
            return Err(Error::InvalidInput);
        }
        let tx = self.db.transaction()?;
        tx.execute(
            "INSERT OR IGNORE INTO transfers VALUES (?1,'open',NULL)",
            [transfer.as_str()],
        )?;
        let state: String = tx.query_row(
            "SELECT state FROM transfers WHERE id=?1",
            [transfer.as_str()],
            |r| r.get(0),
        )?;
        if state != "open" {
            return Err(Error::Conflict);
        }
        tx.execute(
            "INSERT INTO frames(transfer,number,raw) VALUES (?1,?2,?3)",
            params![transfer.as_str(), number, raw],
        )?;
        let sequence = tx.last_insert_rowid();
        tx.commit()?;
        Ok(sequence)
    }

    /// Completes a transfer with a stored receipt, or preserves it as aborted evidence.
    /// # Errors
    /// Refuses missing receipts and already closed transfers.
    pub fn finish_transfer(
        &mut self,
        transfer: &Identifier,
        receipt: Option<ReceiptId>,
    ) -> Result<(), Error> {
        let state = if receipt.is_some() {
            "complete"
        } else {
            "aborted"
        };
        let receipt = receipt.map(|id| key(&id)).transpose()?;
        if self.db.execute(
            "UPDATE transfers SET state=?2,receipt=?3 WHERE id=?1 AND state='open'",
            params![transfer.as_str(), state, receipt],
        )? != 1
        {
            return Err(Error::Conflict);
        }
        Ok(())
    }

    /// Lists unfinished or aborted transfer IDs in a bounded page after an ID.
    /// # Errors
    /// Reports database failures.
    pub fn incomplete(&self, after: &str) -> Result<Vec<String>, Error> {
        let mut stmt = self.db.prepare(
            "SELECT id FROM transfers WHERE state!='complete' AND id>?1 ORDER BY id LIMIT ?2",
        )?;
        Ok(stmt
            .query_map(
                params![
                    after,
                    i64::try_from(self.limits.page_rows).map_err(|_| Error::InvalidInput)?
                ],
                |r| r.get(0),
            )?
            .collect::<Result<_, _>>()?)
    }

    /// Adds one interpretation and all its immutable delivery bytes atomically.
    /// # Errors
    /// Rejects unsafe result delivery, mismatched receipt IDs, bounds and database conflicts.
    pub fn interpret(
        &mut self,
        receipt: ReceiptId,
        evidence: &Interpretation,
        deliveries: &[Delivery],
    ) -> Result<i64, Error> {
        if deliveries.len() > self.limits.page_rows {
            return Err(Error::InvalidInput);
        }
        match &evidence.outcome {
            Reading::Results(results) => {
                if results.iter().any(|r| r.receipt != receipt)
                    || (!deliveries.is_empty()
                        && (results.is_empty()
                            || results.iter().any(|r| !r.may_deliver_to_production())))
                {
                    return Err(Error::InvalidInput);
                }
            }
            Reading::Failed(_) => {
                if !deliveries.is_empty() {
                    return Err(Error::InvalidInput);
                }
            }
        }
        let encoded = self.encode(evidence)?;
        self.bound(encoded.as_bytes())?;
        for delivery in deliveries {
            self.bound(&delivery.payload)?;
        }
        if !deliveries.is_empty() {
            let stored = self.receipt(receipt)?.ok_or(Error::Conflict)?;
            for delivery in deliveries {
                if delivery.ordering_key.machine != stored.metadata.machine {
                    return Err(Error::InvalidInput);
                }
                if let Reading::Results(results) = &evidence.outcome {
                    let matches_observation = results.iter().any(|result| match &result.identity {
                        analyte_model::IdentityStatus::Resolved(identity) => {
                            identity.observation == delivery.ordering_key.observation
                        }
                        analyte_model::IdentityStatus::Unresolved(_) => false,
                    });
                    if !matches_observation {
                        return Err(Error::InvalidInput);
                    }
                }
            }
        }
        let tx = self.db.transaction()?;
        let receipt = key(&receipt)?;
        let version: i64 = tx.query_row(
            "SELECT coalesce(max(version),0)+1 FROM interpretations WHERE receipt=?1",
            [&receipt],
            |r| r.get(0),
        )?;
        tx.execute(
            "INSERT INTO interpretations VALUES (?1,?2,?3)",
            params![receipt, version, encoded],
        )?;
        for d in deliveries {
            tx.execute("INSERT INTO outbox(id,receipt,version,destination,ordering_key,payload,digest,contract,state,next_attempt,enqueued_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,'pending',0,unixepoch())", params![key(&d.id)?,receipt,version,d.destination.as_str(),key(&d.ordering_key)?,d.payload,digest(&d.payload).0.as_slice(),d.contract_version])?;
        }
        if let Reading::Failed(reason) = &evidence.outcome {
            tx.execute(
                "INSERT INTO parked(evidence) VALUES (?1)",
                [key(&("parse_failure", &receipt, reason))?],
            )?;
        }
        if let Reading::Results(results) = &evidence.outcome
            && results
                .iter()
                .any(analyte_model::ResultContent::requires_parking)
        {
            tx.execute(
                "INSERT INTO parked(evidence) VALUES (?1)",
                [key(&(
                    "unresolved_result",
                    &receipt,
                    "result requires operator reconciliation",
                ))?],
            )?;
        }
        tx.commit()?;
        Ok(version)
    }

    /// Claims an eligible head item; unresolved earlier items block only their own key.
    /// # Errors
    /// Reports database failures. In-flight items remain uncertain after restart.
    pub fn claim(&mut self, now: i64) -> Result<Option<Claim>, Error> {
        let tx = self.db.transaction()?;
        let found = tx.query_row("SELECT id,payload,destination,contract,kind,(SELECT count(*) FROM attempts a WHERE a.delivery=o.id) FROM outbox o WHERE state='pending' AND next_attempt<=?1 AND NOT EXISTS(SELECT 1 FROM outbox older WHERE older.destination=o.destination AND older.kind=o.kind AND older.ordering_key=o.ordering_key AND older.sequence<o.sequence AND older.state!='done') AND NOT EXISTS(SELECT 1 FROM outbox paused WHERE paused.destination=o.destination AND paused.state='paused') ORDER BY sequence LIMIT 1", [now], |r| Ok((r.get::<_,String>(0)?,r.get::<_,Vec<u8>>(1)?,r.get::<_,String>(2)?,r.get::<_,u32>(3)?,r.get::<_,String>(4)?,r.get::<_,i64>(5)?))).optional()?;
        let result =
            if let Some((id, payload, destination, contract_version, kind, earlier_attempts)) =
                found
            {
                tx.execute("UPDATE outbox SET state='in_flight' WHERE id=?1", [&id])?;
                Some(Claim {
                    id: serde_json::from_str(&id)?,
                    payload,
                    destination: Identifier::new(&destination).map_err(|_| Error::Corruption)?,
                    contract_version,
                    kind: match kind.as_str() {
                        "result" => DeliveryKind::Result,
                        "order_event" => DeliveryKind::OrderEvent,
                        _ => return Err(Error::Corruption),
                    },
                    earlier_attempts: u64::try_from(earlier_attempts)
                        .map_err(|_| Error::Corruption)?,
                })
            } else {
                None
            };
        tx.commit()?;
        Ok(result)
    }

    /// Records an attempt and its next state atomically, retaining bytes and delivery ID.
    /// # Errors
    /// Refuses outcomes for items that are not in flight.
    pub fn attempted(&mut self, id: DeliveryId, outcome: &Attempt, now: i64) -> Result<(), Error> {
        self.bound(key(outcome)?.as_bytes())?;
        let (state, next) = match outcome {
            Attempt::Done => ("done", 0),
            Attempt::RetryAt(time) => ("pending", *time),
            Attempt::Parked(_) => ("parked", 0),
            Attempt::Paused(_) => ("paused", 0),
            Attempt::Conflict(_) => ("conflict", 0),
        };
        let tx = self.db.transaction()?;
        let id = key(&id)?;
        if tx.execute(
            "UPDATE outbox SET state=?2,next_attempt=?3 WHERE id=?1 AND state='in_flight'",
            params![id, state, next],
        )? != 1
        {
            return Err(Error::Conflict);
        }
        tx.execute(
            "INSERT INTO attempts(delivery,outcome,time) VALUES (?1,?2,?3)",
            params![id, key(outcome)?, now],
        )?;
        match outcome {
            Attempt::Parked(reason) | Attempt::Conflict(reason) => {
                tx.execute(
                    "INSERT INTO parked(evidence) VALUES (?1)",
                    [key(&(state, &id, reason))?],
                )?;
            }
            Attempt::Done | Attempt::RetryAt(_) | Attempt::Paused(_) => {}
        }
        tx.commit()?;
        Ok(())
    }

    /// Appends operator-visible evidence without a deletion API.
    /// # Errors
    /// Refuses oversized evidence or database failures.
    pub fn park(&mut self, item: &Parked) -> Result<i64, Error> {
        let encoded = self.encode(item)?;
        self.bound(encoded.as_bytes())?;
        self.db
            .execute("INSERT INTO parked(evidence) VALUES (?1)", [encoded])?;
        Ok(self.db.last_insert_rowid())
    }

    /// Resolves once, recording who, when and how in both the item and audit trail.
    /// # Errors
    /// Refuses missing or already resolved items.
    pub fn resolve(&mut self, id: i64, resolution: &Audit) -> Result<(), Error> {
        let encoded = self.encode(resolution)?;
        self.bound(encoded.as_bytes())?;
        let tx = self.db.transaction()?;
        if tx.execute(
            "UPDATE parked SET resolution=?2 WHERE id=?1 AND resolution IS NULL",
            params![id, encoded],
        )? != 1
        {
            return Err(Error::Conflict);
        }
        tx.execute("INSERT INTO audit(evidence) VALUES (?1)", [encoded])?;
        tx.commit()?;
        Ok(())
    }

    /// Appends an audit entry permanently.
    /// # Errors
    /// Refuses oversized entries or database failures.
    pub fn audit(&mut self, entry: &Audit) -> Result<(), Error> {
        let encoded = self.encode(entry)?;
        self.bound(encoded.as_bytes())?;
        self.db
            .execute("INSERT INTO audit(evidence) VALUES (?1)", [encoded])?;
        Ok(())
    }

    /// Explicitly releases an uncertain or blocked delivery using its existing bytes and ID.
    /// The operator must reconcile a possible prior acceptance before using this operation.
    /// # Errors
    /// Refuses pending/done items, oversized audit evidence or database failures.
    pub fn release_delivery(&mut self, id: DeliveryId, audit: &Audit) -> Result<(), Error> {
        let encoded = self.encode(audit)?;
        self.bound(encoded.as_bytes())?;
        let tx = self.db.transaction()?;
        if tx.execute("UPDATE outbox SET state='pending',next_attempt=0 WHERE id=?1 AND state IN ('in_flight','parked','paused','conflict')", [key(&id)?])? != 1 { return Err(Error::Conflict); }
        tx.execute(
            "INSERT INTO audit(evidence) VALUES (?1)",
            [key(&(id, encoded))?],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Stores a bounded page with its cursor in the same commit.
    /// # Errors
    /// Rejects oversized pages and database failures.
    pub fn order_page(&mut self, page: &OrderPage) -> Result<(), Error> {
        if page.orders.len() > self.limits.page_rows {
            return Err(Error::InvalidInput);
        }
        self.bound(self.encode(page)?.as_bytes())?;
        let tx = self.db.transaction()?;
        tx.execute("INSERT INTO feed(destination,cursor,complete,trusted,polled) VALUES (?1,?2,?3,1,?4) ON CONFLICT(destination) DO UPDATE SET cursor=excluded.cursor,complete=max(feed.complete,excluded.complete),trusted=1,polled=excluded.polled",params![page.destination.as_str(),page.cursor.as_str(),page.complete,page.polled])?;
        for order in &page.orders {
            tx.execute("INSERT INTO orders VALUES (?1,?2,?3,?4,?5,?6,(SELECT generation FROM feed WHERE destination=?1)) ON CONFLICT(destination,id) DO UPDATE SET sample=excluded.sample,evidence=excluded.evidence,tombstone=excluded.tombstone,expiry=excluded.expiry,generation=excluded.generation", params![page.destination.as_str(),order.id.as_str(),order.sample.as_str(),key(order)?,order.tombstone,order.expiry])?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Invalidates the cursor and snapshot after 410; stale orders remain as evidence.
    /// # Errors
    /// Reports database failures.
    pub fn reset_orders(&mut self, destination: &Identifier) -> Result<(), Error> {
        self.db.execute(
            "UPDATE feed SET trusted=0,complete=0,cursor='',generation=generation+1 WHERE destination=?1",
            [destination.as_str()],
        )?;
        Ok(())
    }

    /// Returns three-way trust, refusing stale, incomplete, future-clock or oversized answers.
    /// # Errors
    /// Reports database or encoded evidence failures.
    pub fn orders(
        &self,
        destination: &Identifier,
        sample: &SampleId,
        now: i64,
        freshness: u64,
    ) -> Result<Lookup, Error> {
        let poll: Option<i64> = self
            .db
            .query_row(
                "SELECT polled FROM feed WHERE destination=?1 AND complete=1 AND trusted=1",
                [destination.as_str()],
                |r| r.get(0),
            )
            .optional()?;
        let Some(age) = poll
            .and_then(|p| now.checked_sub(p))
            .and_then(|age| u64::try_from(age).ok())
        else {
            return Ok(Lookup::NotTrusted);
        };
        if age >= freshness {
            return Ok(Lookup::NotTrusted);
        }
        let mut stmt=self.db.prepare("SELECT evidence FROM orders WHERE destination=?1 AND sample=?2 AND tombstone=0 AND expiry>?3 AND generation=(SELECT generation FROM feed WHERE destination=?1) LIMIT ?4")?;
        let limit = self
            .limits
            .page_rows
            .checked_add(1)
            .ok_or(Error::InvalidInput)?;
        let encoded = stmt
            .query_map(
                params![
                    destination.as_str(),
                    sample.as_str(),
                    now,
                    i64::try_from(limit).map_err(|_| Error::InvalidInput)?
                ],
                |r| r.get::<_, String>(0),
            )?
            .collect::<Result<Vec<_>, _>>()?;
        if encoded.len() > self.limits.page_rows {
            return Ok(Lookup::NotTrusted);
        }
        let orders = encoded
            .iter()
            .map(|s| serde_json::from_str(s))
            .collect::<Result<Vec<_>, _>>()?;
        if orders.is_empty() {
            Ok(Lookup::NoOrders)
        } else {
            Ok(Lookup::Orders(orders))
        }
    }

    /// Uses SQLite online backup, then checks the restored copy before reporting success.
    /// # Errors
    /// Refuses an existing target, timeout, corruption or database/filesystem failures.
    pub fn backup(&self, target: &Path, deadline: Instant) -> Result<(), Error> {
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(target)?;
        drop(file);
        let mut copy = Connection::open(target)?;
        {
            let backup = rusqlite::backup::Backup::new(&self.db, &mut copy)?;
            loop {
                if Instant::now() >= deadline {
                    return Err(Error::Timeout);
                }
                match backup.step(64)? {
                    rusqlite::backup::StepResult::Done => break,
                    rusqlite::backup::StepResult::More => {}
                    rusqlite::backup::StepResult::Busy | rusqlite::backup::StepResult::Locked => {
                        return Err(Error::Timeout);
                    }
                    _ => return Err(Error::Corruption), // Future SQLite states fail closed.
                }
            }
        }
        Self::integrity(&copy)
    }
}

#[cfg(test)]
mod tests;
