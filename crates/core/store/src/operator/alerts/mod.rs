//! Stores nonclinical alert states and transition history.
use crate::{OperatorError as Error, Store, key};
use analyte_model::Identifier;
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
#[cfg(test)]
mod tests;

/// Stable signal identity; thresholds and responders may change without losing history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertKey {
    /// Oldest unfinished delivery.
    OldestUndelivered,
    /// Last durable message or journaled frame from this machine.
    MachineSilent(Identifier),
    /// Free bytes relative to the configured reserve.
    DiskReserve,
    /// Oldest or untrusted destination order cache.
    OrderCacheStale,
    /// Uncertain machine command count.
    UncertainOrders,
    /// Unresolved parked count.
    ParkedItems,
    /// Restore reconciliation pause.
    RestorePaused,
    /// Items wait for a lab app destination this run cannot deliver to, and why.
    UndeliverableDestination(Identifier, Undeliverable),
}

/// Why waiting items cannot reach their lab app destination. Each reason is its
/// own condition, so a changed reason clears one alert and raises the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Undeliverable {
    /// No set-up machine and no kept destination defines it.
    NotConfigured,
    /// A kept destination is defined, but its keys could not be loaded.
    CredentialsMissing,
}

/// Latest durable evaluation; timestamps are Unix seconds.
#[derive(Debug, Clone)]
pub struct AlertState {
    /// Stable signal identity.
    pub key: AlertKey,
    /// Role or person responsible; never included in ordinary logs.
    pub responder: String,
    /// Whether the condition remains raised.
    pub active: bool,
    /// Most recent raise, retained after clear.
    pub raised_at: Option<i64>,
    /// Most recent clear, retained after a subsequent raise.
    pub cleared_at: Option<i64>,
    /// Nonclinical numeric measurement; absent means unknown.
    pub value: Option<u64>,
    /// Time of the last successful evaluation.
    pub evaluated_at: i64,
}

/// One retained raise or clear, with its responder and measurement.
#[derive(Debug)]
pub struct AlertTransition {
    /// Monotonic history cursor.
    pub sequence: i64,
    /// Snapshot at transition time.
    pub state: AlertState,
}

impl Store {
    /// Reads one bounded page of active alerts, including retained policies no longer configured.
    /// Pass the final key serialized as JSON as the cursor, or an empty string initially.
    /// # Errors
    /// Reports corrupt identities/state, oversized cursors or SQLite failures.
    pub fn active_alerts(&self, after: &str) -> Result<Vec<AlertState>, Error> {
        self.bound(after.as_bytes())?;
        let mut stmt = self.db.prepare(
            "SELECT key FROM alert_state WHERE active=1 AND key>?1 ORDER BY key LIMIT ?2",
        )?;
        let keys = stmt
            .query_map(
                params![
                    after,
                    i64::try_from(self.limits.page_rows)
                        .map_err(|_| Error::InvalidOperatorInput)?
                ],
                |row| row.get::<_, String>(0),
            )?
            .collect::<Result<Vec<_>, _>>()?;
        keys.into_iter()
            .map(|encoded| {
                let identity = serde_json::from_str(&encoded)?;
                self.alert_state(&identity)?.ok_or(Error::Corruption)
            })
            .collect()
    }

    /// Reads one policy's current durable state without payloads.
    /// # Errors
    /// Reports SQLite, corrupt measurement or identity encoding failures.
    pub fn alert_state(&self, identity: &AlertKey) -> Result<Option<AlertState>, Error> {
        let row = self.db.query_row(
            "SELECT responder,active,raised_at,cleared_at,value,evaluated_at FROM alert_state WHERE key=?1",
            [key(identity)?],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, bool>(1)?, r.get::<_, Option<i64>>(2)?,
                r.get::<_, Option<i64>>(3)?, r.get::<_, Option<String>>(4)?, r.get::<_, i64>(5)?))
        ).optional()?;
        row.map(
            |(responder, active, raised_at, cleared_at, value, evaluated_at)| {
                Ok(AlertState {
                    key: identity.clone(),
                    responder,
                    active,
                    raised_at,
                    cleared_at,
                    value: value
                        .map(|value| value.parse::<u64>())
                        .transpose()
                        .map_err(|_| Error::Corruption)?,
                    evaluated_at,
                })
            },
        )
        .transpose()
    }

    /// Records a successful evaluation atomically with a transition only if active changes.
    /// A cancelled caller may have committed; retrying the same condition never repeats a transition.
    /// An initial clear creates state but no clear history. History is append-only and retained.
    /// # Errors
    /// Refuses invalid responders or database failures, including a full disk.
    pub fn evaluate_alert(
        &mut self,
        identity: &AlertKey,
        responder: &str,
        active: bool,
        value: Option<u64>,
        now: i64,
    ) -> Result<Option<AlertState>, Error> {
        if responder.trim().is_empty()
            || responder.len() > 256
            || responder.chars().any(char::is_control)
        {
            return Err(Error::InvalidOperatorInput);
        }
        let value = value.map(|value| value.to_string());
        // Like audit records, bounded safety history may use the admission reserve.
        // Otherwise reaching that reserve would prevent its own disk alert.
        // A real disk-full failure is returned; no successful transition is invented.
        let encoded = key(identity)?;
        let tx = self.db.transaction()?;
        let previous: Option<bool> = tx
            .query_row(
                "SELECT active FROM alert_state WHERE key=?1",
                [&encoded],
                |r| r.get(0),
            )
            .optional()?;
        let changed = previous.unwrap_or(false) != active;
        tx.execute("INSERT INTO alert_state(key,responder,active,raised_at,cleared_at,value,evaluated_at) VALUES(?1,?2,?3,CASE WHEN ?3 THEN ?5 END,NULL,?4,?5)
            ON CONFLICT(key) DO UPDATE SET responder=excluded.responder,active=excluded.active,value=excluded.value,evaluated_at=excluded.evaluated_at,
            raised_at=CASE WHEN ?6 AND ?3 THEN ?5 ELSE alert_state.raised_at END,
            cleared_at=CASE WHEN ?6 AND NOT ?3 THEN ?5 ELSE alert_state.cleared_at END",
            params![encoded, responder, active, value, now, changed])?;
        if changed {
            tx.execute(
                "INSERT INTO alert_history(key,active,time,responder,value) VALUES(?1,?2,?3,?4,?5)",
                params![encoded, active, now, responder, value],
            )?;
        }
        tx.commit()?;
        if changed {
            self.alert_state(identity)
        } else {
            Ok(None)
        }
    }

    /// Reads one bounded page of retained transitions, ordered by sequence.
    /// # Errors
    /// Reports database failures or corrupt history.
    pub fn alert_history(&self, after: i64) -> Result<Vec<AlertTransition>, Error> {
        let mut stmt = self.db.prepare("SELECT sequence,key,responder,active,time,value FROM alert_history WHERE sequence>?1 ORDER BY sequence LIMIT ?2")?;
        let mut rows = stmt.query(params![
            after,
            i64::try_from(self.limits.page_rows).map_err(|_| Error::InvalidOperatorInput)?
        ])?;
        let mut result = Vec::new();
        while let Some(row) = rows.next()? {
            let active: bool = row.get(3)?;
            let time: i64 = row.get(4)?;
            let value: Option<String> = row.get(5)?;
            result.push(AlertTransition {
                sequence: row.get(0)?,
                state: AlertState {
                    key: serde_json::from_str(&row.get::<_, String>(1)?)?,
                    responder: row.get(2)?,
                    active,
                    raised_at: active.then_some(time),
                    cleared_at: (!active).then_some(time),
                    value: value
                        .map(|value| value.parse::<u64>())
                        .transpose()
                        .map_err(|_| Error::Corruption)?,
                    evaluated_at: time,
                },
            });
        }
        Ok(result)
    }

    /// Reads a scalar signal only. Missing machine/cache history remains unknown.
    /// Indexed pending/parked/uncertain queries do not inspect clinical evidence.
    /// # Errors
    /// Reports capacity probe, SQLite or corrupt numeric data.
    pub fn alert_value(
        &self,
        identity: &AlertKey,
        now: i64,
        destinations: &[(Identifier, u64)],
    ) -> Result<Option<u64>, Error> {
        if destinations.len() > 128 {
            return Err(Error::InvalidOperatorInput);
        }
        let scalar: Option<i64> = match identity {
            AlertKey::OldestUndelivered => self
                .db
                .query_row(
                    "SELECT min(enqueued_at) FROM outbox WHERE state!=?1",
                    [crate::DeliveryState::Done],
                    |r| r.get::<_, Option<i64>>(0),
                )?
                .map(|time| now.checked_sub(time).filter(|age| *age >= 0))
                .unwrap_or(Some(0)),
            AlertKey::MachineSilent(machine) => self
                .db
                .query_row(
                    "SELECT time FROM machine_activity WHERE machine=?1",
                    [machine.as_str()],
                    |r| r.get::<_, i64>(0),
                )
                .optional()?
                .and_then(|time| now.checked_sub(time).filter(|age| *age >= 0)),
            AlertKey::DiskReserve => return Ok(Some(self.space.available(&self.path)?)),
            AlertKey::OrderCacheStale => {
                if destinations.is_empty() {
                    return Ok(None);
                }
                let mut oldest = 0;
                for (destination, freshness) in destinations {
                    let trust = self.order_trust(destination, now, *freshness)?;
                    let crate::OrderTrust::Trusted { age } = trust else {
                        return Ok(None);
                    };
                    oldest = oldest.max(i64::try_from(age).map_err(|_| Error::Corruption)?);
                }
                Some(oldest)
            }
            AlertKey::UncertainOrders => Some(self.db.query_row(
                "SELECT value FROM alert_counts WHERE kind='uncertain'",
                [],
                |r| r.get(0),
            )?),
            AlertKey::ParkedItems => Some(self.db.query_row(
                "SELECT value FROM alert_counts WHERE kind='parked'",
                [],
                |r| r.get(0),
            )?),
            AlertKey::RestorePaused => Some(i64::from(self.restore_unreconciled()?)),
            AlertKey::UndeliverableDestination(destination, _) => Some(
                crate::delivery::outbox::count_undelivered(&self.db, destination)?,
            ),
        };
        scalar
            .map(u64::try_from)
            .transpose()
            .map_err(|_| Error::Corruption)
    }
}
