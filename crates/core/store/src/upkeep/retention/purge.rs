//! Purges at most sixteen eligible receipts in one deadline-bound, audited transaction.
use super::{eligibility, tombstones};
use crate::{Error, Store, key};
use analyte_model::{Identifier, ReceiptId};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::{fs::File, path::Path, time::Instant};

/// Nonclinical outcome of one bounded retention run (ADR 0013 item 4).
#[derive(Clone, Serialize, Deserialize)]
pub struct PurgeRun {
    /// Delivery payloads removed in this committed batch.
    pub deliveries: usize,
    /// Result revisions retained as keyed duplicate evidence.
    pub results: usize,
    /// Whether the batch rolled back after an operational failure.
    pub failed: bool,
    /// Explicit machine policy; `None` means keep.
    pub days: Option<u16>,
    /// Installation machine identity.
    pub machine: Identifier,
    /// Host run time in Unix seconds.
    pub time: i64,
    /// Number of receipt candidates examined.
    pub examined: usize,
    /// Number of receipts purged atomically with this audit.
    pub purged: usize,
    /// First and last purged receipt UUIDs, absent for an empty run.
    pub first: Option<ReceiptId>,
    /// Last purged receipt UUID.
    pub last: Option<ReceiptId>,
    /// Resume scanning after this receipt UUID; reset at the end of the store.
    pub after: Option<ReceiptId>,
    /// Whether maintenance or restore reconciliation prevented this run.
    pub paused: bool,
}

pub(crate) fn maintenance(path: &Path, exclusive: bool) -> Result<File, Error> {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path.with_extension("maintenance-lock"))?;
    let outcome = if exclusive {
        file.try_lock()
    } else {
        file.try_lock_shared()
    };
    outcome.map_err(|error| match error {
        std::fs::TryLockError::WouldBlock => Error::MaintenanceBusy,
        std::fs::TryLockError::Error(error) => Error::Io(error),
    })?;
    Ok(file)
}
impl Store {
    /// Purges a small batch and commits its nonclinical audit in the same transaction.
    /// Cancellation after submission may leave this entire batch committed; inspect its audit.
    /// Backup holds a shared maintenance lock; restore requires exclusive store ownership.
    /// # Errors
    /// Rejects missing keys, invalid periods, elapsed deadlines and database failures atomically.
    pub fn purge_batch(
        &mut self,
        machine: &Identifier,
        days: Option<u16>,
        after: Option<ReceiptId>,
        now: i64,
        deadline: Instant,
    ) -> Result<PurgeRun, Error> {
        if Instant::now() >= deadline {
            return Err(Error::Timeout);
        }
        if days.is_some_and(|days| !(1..=3650).contains(&days)) {
            return Err(Error::InvalidInput);
        }
        if days.is_some() {
            self.retention_space_ready()?;
        }
        let lock = maintenance(&self.path, true);
        let paused = match &lock {
            Ok(_) => self.restore_unreconciled()?,
            Err(Error::MaintenanceBusy) => true,
            Err(_) => return Err(lock.err().ok_or(Error::Corruption)?),
        };
        let interrupted = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let interrupt = interrupted.clone();
        self.db.progress_handler(
            100,
            Some(move || {
                Instant::now() >= deadline
                    && !interrupt.swap(true, std::sync::atomic::Ordering::AcqRel)
            }),
        )?;
        self.db.busy_timeout(
            self.limits
                .busy_timeout
                .min(deadline.saturating_duration_since(Instant::now())),
        )?;
        let result = self.purge_transaction(machine, days, after, now, paused);
        self.db.progress_handler(0, None::<fn() -> bool>)?;
        self.db.busy_timeout(self.limits.busy_timeout)?;
        let result = if interrupted.load(std::sync::atomic::Ordering::Acquire) {
            Err(Error::Timeout)
        } else {
            result
        };
        if result.is_err() {
            let failure = PurgeRun {
                deliveries: 0,
                results: 0,
                failed: true,
                days,
                machine: machine.clone(),
                time: now,
                examined: 0,
                purged: 0,
                first: None,
                last: None,
                after,
                paused,
            };
            self.db.busy_timeout(
                self.limits
                    .busy_timeout
                    .min(std::time::Duration::from_millis(250)),
            )?;
            let encoded = audit_evidence(&failure)?;
            self.bound(encoded.as_bytes())?;
            let audit = self
                .db
                .execute("INSERT INTO audit(evidence) VALUES (?1)", [encoded]);
            self.db.busy_timeout(self.limits.busy_timeout)?;
            audit?;
        }
        result
    }
    fn purge_transaction(
        &mut self,
        machine: &Identifier,
        days: Option<u16>,
        after: Option<ReceiptId>,
        now: i64,
        paused: bool,
    ) -> Result<PurgeRun, Error> {
        let mut run = PurgeRun {
            deliveries: 0,
            results: 0,
            failed: false,
            days,
            machine: machine.clone(),
            time: now,
            examined: 0,
            purged: 0,
            first: None,
            last: None,
            after: None,
            paused,
        };
        let credential = self.retention_key.as_ref();
        if days.is_some() && credential.is_none() {
            return Err(Error::RetentionKeyRequired);
        }
        let tx = self
            .db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        if let (Some(days), Some(credential)) = (days, credential)
            && !paused
        {
            let candidates = candidates(&tx, machine, after, self.limits.page_rows.min(16))?;
            run.examined = candidates.len();
            run.after = candidates.last().copied();
            let guards = tombstones::remove_delete_guards(&tx)?;
            for receipt in candidates {
                if eligibility::eligible(&tx, receipt, days, now, &self.limits)? {
                    let (deliveries, results) =
                        tombstones::purge(&tx, credential, receipt, now, self.limits.record_bytes)?;
                    run.deliveries = run
                        .deliveries
                        .checked_add(deliveries)
                        .ok_or(Error::InvalidInput)?;
                    run.results = run
                        .results
                        .checked_add(results)
                        .ok_or(Error::InvalidInput)?;
                    run.first.get_or_insert(receipt);
                    run.last = Some(receipt);
                    run.purged = run.purged.checked_add(1).ok_or(Error::InvalidInput)?;
                }
            }
            for sql in guards {
                tx.execute_batch(&sql)?;
            }
        }
        let encoded = audit_evidence(&run)?;
        if encoded.len() > self.limits.record_bytes {
            return Err(Error::InvalidInput);
        }
        tx.execute("INSERT INTO audit(evidence) VALUES (?1)", [encoded])?;
        tx.commit()?;
        Ok(run)
    }
    /// Reads the most recent committed run without clinical payloads.
    /// # Errors
    /// Reports corrupt audit evidence or database failures.
    pub fn last_purge_run(&self) -> Result<Option<PurgeRun>, Error> {
        let encoded: Option<String> = self.db.query_row("SELECT json_extract(evidence,'$.retention') FROM audit WHERE json_extract(evidence,'$.action')='retention-purge' ORDER BY sequence DESC LIMIT 1", [], |r| r.get(0)).optional()?;
        encoded
            .map(|text| serde_json::from_str::<PurgeRun>(&text).map_err(Error::from))
            .transpose()
    }
    /// Reports the oldest retained receipt time in Unix seconds.
    /// # Errors
    /// Reports database failures; no receipts returns absence.
    pub fn oldest_retained_receipt(&self) -> Result<Option<i64>, Error> {
        Ok(self.db.query_row(
            "SELECT min(unixepoch(json_extract(metadata,'$.received_at'))) FROM receipts",
            [],
            |r| r.get(0),
        )?)
    }
}
fn candidates(
    db: &rusqlite::Connection,
    machine: &Identifier,
    after: Option<ReceiptId>,
    limit: usize,
) -> Result<Vec<ReceiptId>, Error> {
    let mut statement = db.prepare("SELECT id FROM receipts WHERE json_extract(metadata,'$.machine')=?1 AND (?2 IS NULL OR id>?2) ORDER BY id LIMIT ?3")?;
    let rows = statement.query_map(
        params![
            machine.as_str(),
            after.map(|id| key(&id)).transpose()?,
            i64::try_from(limit).map_err(|_| Error::InvalidInput)?
        ],
        |r| r.get::<_, String>(0),
    )?;
    rows.map(|row| Ok(serde_json::from_str(&row?)?)).collect()
}

fn audit_evidence(run: &PurgeRun) -> Result<String, Error> {
    let targets = run
        .first
        .into_iter()
        .chain(run.last)
        .map(|id| {
            let text: String = serde_json::from_str(&key(&id)?)?;
            Identifier::new(&text).map_err(|_| Error::InvalidInput)
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let audit = crate::Audit {
        actor: Identifier::new("retention-worker").map_err(|_| Error::InvalidInput)?,
        action: Identifier::new("retention-purge").map_err(|_| Error::InvalidInput)?,
        targets,
        time: run.time,
        reason: analyte_model::Reason::new(if run.failed {
            "batch rolled back"
        } else {
            "finished-data retention run"
        })
        .map_err(|_| Error::InvalidInput)?,
    };
    let mut evidence = serde_json::to_value(audit)?;
    evidence
        .as_object_mut()
        .ok_or(Error::InvalidInput)?
        .insert("retention".into(), serde_json::to_value(run)?);
    key(&evidence)
}
