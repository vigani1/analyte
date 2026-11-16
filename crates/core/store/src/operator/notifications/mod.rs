//! Stores webhook queues and attempt progress separately from clinical deliveries.
mod admission;
mod event;
use crate::{Error, Store};
use analyte_model::Identifier;
use rusqlite::{OptionalExtension, params};

/// Persisted notification identity; only the store creates one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AlertEventId(String);
impl AlertEventId {
    /// Borrows the stable wire identity without exposing event payloads.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Immutable bytes and identity for one queued webhook event.
#[derive(Debug, PartialEq, Eq)]
pub struct PendingAlert {
    /// Stable across retries and restart, including uncertain acceptance.
    id: AlertEventId,
    /// Original JSON bytes; never log these bytes.
    payload: Vec<u8>,
    /// Completed or reserved attempts, including interrupted attempts.
    attempts: u32,
}

impl PendingAlert {
    /// Borrows the persisted event identity used on every retry.
    #[must_use]
    pub fn id(&self) -> &AlertEventId {
        &self.id
    }
    /// Borrows original event bytes without permitting mutation under the same identity.
    #[must_use]
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }
    /// Returns the durable attempt reservation count.
    #[must_use]
    pub fn attempts(&self) -> u32 {
        self.attempts
    }
}

/// Fixed operational failure categories; remote text is never stored here.
#[derive(Clone, Copy, Debug)]
pub enum AlertFailure {
    /// HTTP attempt failed without a trustworthy response.
    Network,
    /// Installation credential unavailable or malformed.
    Credentials,
    /// Peer returned a non-success status.
    Http,
    /// Reserved attempt ended without a recorded outcome.
    Interrupted,
}
impl AlertFailure {
    fn as_str(self) -> &'static str {
        match self {
            Self::Network => "network",
            Self::Credentials => "credentials",
            Self::Http => "http",
            Self::Interrupted => "interrupted",
        }
    }
}

/// Operational snapshot of one endpoint without its URL, credentials or event payloads.
#[derive(Debug)]
pub struct AlertEndpointStatus {
    /// Installation endpoint reference.
    pub endpoint: Identifier,
    /// Pending events including exhausted failures.
    pub pending: u64,
    /// Oldest observation time; absent for an empty queue.
    pub oldest: Option<i64>,
    /// Last committed successful HTTP acceptance.
    pub last_success: Option<i64>,
    /// Earliest eligible retry deadline.
    pub next_attempt: Option<i64>,
    /// Exhausted events requiring operator action.
    pub exhausted: u64,
    /// Admission stopped without advancing retained transition progress.
    pub queue_full: bool,
    /// Events with a recorded failure or interrupted attempt.
    pub failed: u64,
}
impl Store {
    /// Registers an endpoint without resetting its cursor, queue or retry deadlines.
    /// Cancellation may commit registration; retry is idempotent.
    /// # Errors
    /// Reports SQLite failures and refuses more than 128 retained endpoint identities.
    pub fn register_alert_endpoint(&mut self, endpoint: &Identifier) -> Result<(), Error> {
        let tx = self.db.transaction()?;
        let existing: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM alert_endpoints WHERE id=?1)",
            [endpoint.as_str()],
            |r| r.get(0),
        )?;
        let count: u32 = tx.query_row("SELECT count(*) FROM alert_endpoints", [], |r| r.get(0))?;
        if !existing && count >= 128 {
            return Err(Error::InvalidInput);
        }
        tx.execute(
            "INSERT OR IGNORE INTO alert_endpoints(id) VALUES(?1)",
            [endpoint.as_str()],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Reserves one attempt and persists its next retry before returning immutable bytes.
    /// A crash or cancellation may leave this attempt reserved. Retry waits until its
    /// durable deadline and uses the same bytes. The final reserved attempt is marked
    /// exhausted until success is committed, so a restart cannot exceed the budget.
    /// # Errors
    /// Reports invalid policy bounds, corrupt state or SQLite failures.
    pub fn claim_alert_attempt(
        &mut self,
        endpoint: &Identifier,
        now: i64,
        max_attempts: u32,
        backoff_min: u64,
        backoff_max: u64,
    ) -> Result<Option<PendingAlert>, Error> {
        if !(1..=100).contains(&max_attempts)
            || backoff_min == 0
            || backoff_min > backoff_max
            || backoff_max > 86400
        {
            return Err(Error::InvalidInput);
        }
        let tx = self.db.transaction()?;
        let row: Option<(String, Vec<u8>, u32)> = tx.query_row(
            "SELECT e.id,e.payload,p.attempts FROM alert_events e JOIN alert_progress p ON p.event=e.id WHERE e.endpoint=?1 AND p.exhausted=0 AND p.next_attempt<=?2 AND NOT EXISTS(SELECT 1 FROM alert_events earlier WHERE earlier.endpoint=e.endpoint AND earlier.condition=e.condition AND earlier.rowid<e.rowid) ORDER BY e.rowid LIMIT 1",
            params![endpoint.as_str(),now], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        let Some((id, payload, previous)) = row else {
            return Ok(None);
        };
        if payload.len() > 16384 {
            return Err(Error::Corruption);
        }
        let attempts = previous.checked_add(1).ok_or(Error::Corruption)?;
        let delay = backoff_min
            .saturating_mul(1u64.checked_shl(previous.min(63)).unwrap_or(u64::MAX))
            .min(backoff_max);
        let next = now
            .checked_add(i64::try_from(delay).map_err(|_| Error::InvalidInput)?)
            .ok_or(Error::InvalidInput)?;
        tx.execute("UPDATE alert_progress SET attempts=?2,next_attempt=?3,exhausted=?4,failure='interrupted' WHERE event=?1",
            params![id, attempts, next, attempts>=max_attempts])?;
        tx.commit()?;
        if id.len() != 32 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(Error::Corruption);
        }
        Ok(Some(PendingAlert {
            id: AlertEventId(id),
            payload,
            attempts,
        }))
    }

    /// Records a fixed failure category without resetting the already reserved backoff.
    /// # Errors
    /// Reports SQLite failures or unknown event identities.
    pub fn fail_alert_attempt(
        &mut self,
        id: &AlertEventId,
        failure: AlertFailure,
    ) -> Result<(), Error> {
        let id = id.as_str();
        if self.db.execute(
            "UPDATE alert_progress SET failure=?2 WHERE event=?1",
            params![id, failure.as_str()],
        )? != 1
        {
            return Err(Error::InvalidInput);
        }
        Ok(())
    }

    /// Commits peer acceptance and frees queue capacity atomically.
    /// Cancellation can leave either the original pending bytes or committed success;
    /// it never constructs a replacement event.
    /// # Errors
    /// Reports SQLite failures or unknown identities.
    pub fn complete_alert_attempt(&mut self, id: &AlertEventId, now: i64) -> Result<(), Error> {
        let id = id.as_str();
        let tx = self.db.transaction()?;
        let endpoint: String =
            tx.query_row("SELECT endpoint FROM alert_events WHERE id=?1", [id], |r| {
                r.get(0)
            })?;
        tx.execute(
            "UPDATE alert_endpoints SET last_success=?2 WHERE id=?1",
            params![endpoint, now],
        )?;
        tx.execute("DELETE FROM alert_events WHERE id=?1", [id])?;
        tx.commit()?;
        Ok(())
    }

    /// Starts a new operator-authorized attempt budget for exhausted events only.
    /// Identity and bytes never change. The reset and audit commit together; an
    /// uncertain command can be inspected in status before being repeated.
    /// # Errors
    /// Refuses an empty retry selection, oversized queues or invalid audit evidence.
    pub fn retry_failed_alerts(
        &mut self,
        endpoint: &Identifier,
        audit: &crate::Audit,
    ) -> Result<(), Error> {
        if audit.action.as_str() != "alert-retry" || audit.targets.as_slice() != [endpoint.clone()]
        {
            return Err(Error::InvalidInput);
        }
        let encoded = self.encode(audit)?;
        let tx = self.db.transaction()?;
        let count: u32 = tx.query_row("SELECT count(*) FROM alert_events e JOIN alert_progress p ON p.event=e.id WHERE e.endpoint=?1 AND p.exhausted=1", [endpoint.as_str()], |r| r.get(0))?;
        if count == 0 || count > 4096 {
            return Err(Error::InvalidInput);
        }
        tx.execute("UPDATE alert_progress SET attempts=0,exhausted=0,next_attempt=?2,failure=NULL WHERE exhausted=1 AND event IN (SELECT id FROM alert_events WHERE endpoint=?1)", params![endpoint.as_str(),audit.time])?;
        tx.execute("INSERT INTO audit(evidence) VALUES(?1)", [encoded])?;
        tx.commit()?;
        Ok(())
    }

    /// Returns bounded metadata only, including retained removed endpoint queues.
    /// # Errors
    /// Reports corruption or SQLite failures rather than a healthy empty snapshot.
    pub fn alert_endpoint_status(&self) -> Result<Vec<AlertEndpointStatus>, Error> {
        let mut stmt = self.db.prepare("SELECT d.id,count(e.id),min(e.observed),d.last_success,min(CASE WHEN p.exhausted=0 THEN p.next_attempt END),coalesce(sum(p.exhausted),0),d.queue_full,coalesce(sum(p.failure IS NOT NULL),0) FROM alert_endpoints d LEFT JOIN alert_events e ON e.endpoint=d.id LEFT JOIN alert_progress p ON p.event=e.id GROUP BY d.id ORDER BY d.id LIMIT 129")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                u64::from(r.get::<_, u32>(1)?),
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                u64::from(r.get::<_, u32>(5)?),
                r.get(6)?,
                u64::from(r.get::<_, u32>(7)?),
            ))
        })?;
        let mut result = Vec::new();
        for row in rows {
            let (id, pending, oldest, last_success, next_attempt, exhausted, queue_full, failed) =
                row?;
            if result.len() == 128 {
                return Err(Error::Corruption);
            }
            result.push(AlertEndpointStatus {
                endpoint: Identifier::new(&id).map_err(|_| Error::Corruption)?,
                pending,
                oldest,
                last_success,
                next_attempt,
                exhausted,
                queue_full,
                failed,
            });
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests;
