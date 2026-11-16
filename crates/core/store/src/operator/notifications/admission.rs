//! Admits retained transitions and due reminders without losing progress at capacity.
use super::event::{self, EventKind};
use crate::{AlertKey, AlertState, Error, Store};
use analyte_model::Identifier;
use rusqlite::{OptionalExtension, Transaction, params};

impl Store {
    /// Admits at most one transition or reminder, bounded independently of history size.
    /// Full queues retain their cursor and evidence. A missed reminder interval emits
    /// one event, then schedules from now. Submitted cancellation may commit admission;
    /// its event, cursor and reminder deadline commit together to prevent duplication.
    /// # Errors
    /// Reports invalid limits, unknown endpoints, corrupt episode history or SQLite failures.
    pub fn prepare_alert_event(
        &mut self,
        endpoint: &Identifier,
        now: i64,
        reminder_seconds: u64,
        capacity: usize,
    ) -> Result<bool, Error> {
        if !(60..=86400).contains(&reminder_seconds) || !(1..=4096).contains(&capacity) {
            return Err(Error::InvalidOperatorInput);
        }
        let cursor: i64 = self.db.query_row(
            "SELECT cursor FROM alert_endpoints WHERE id=?1",
            [endpoint.as_str()],
            |r| r.get(0),
        )?;
        let transition = self.alert_history(cursor)?.into_iter().next();
        let tx = self.db.transaction()?;
        let count: u32 = tx.query_row(
            "SELECT count(*) FROM alert_events WHERE endpoint=?1",
            [endpoint.as_str()],
            |r| r.get(0),
        )?;
        let full =
            u64::from(count) >= u64::try_from(capacity).map_err(|_| Error::InvalidOperatorInput)?;
        tx.execute(
            "UPDATE alert_endpoints SET queue_full=?2 WHERE id=?1",
            params![endpoint.as_str(), full],
        )?;
        if full {
            tx.commit()?;
            return Ok(false);
        }
        let next = now
            .checked_add(i64::try_from(reminder_seconds).map_err(|_| Error::InvalidOperatorInput)?)
            .ok_or(Error::InvalidOperatorInput)?;
        if let Some(transition) = transition {
            let state = transition.state;
            let condition = crate::key(&state.key)?;
            let episode: (i64,i64) = tx.query_row("SELECT sequence,time FROM alert_history WHERE key=?1 AND active=1 AND sequence<=?2 ORDER BY sequence DESC LIMIT 1",
                params![condition, transition.sequence], |r| Ok((r.get(0)?,r.get(1)?)))?;
            let kind = if state.active {
                EventKind::Raised
            } else {
                EventKind::Resolved
            };
            enqueue(&tx, endpoint, &state, episode, kind, now)?;
            if state.active {
                tx.execute("INSERT INTO alert_reminders VALUES(?1,?2,?3,?4) ON CONFLICT(endpoint,condition) DO UPDATE SET episode=excluded.episode,next_reminder=excluded.next_reminder",
                    params![endpoint.as_str(),condition,episode.0,next])?;
            } else {
                tx.execute(
                    "DELETE FROM alert_reminders WHERE endpoint=?1 AND condition=?2",
                    params![endpoint.as_str(), condition],
                )?;
            }
            tx.execute(
                "UPDATE alert_endpoints SET cursor=?2 WHERE id=?1",
                params![endpoint.as_str(), transition.sequence],
            )?;
            tx.commit()?;
            return Ok(true);
        }
        let due: Option<(String,i64)> = tx.query_row("SELECT r.condition,r.episode FROM alert_reminders r JOIN alert_state s ON s.key=r.condition WHERE r.endpoint=?1 AND r.next_reminder<=?2 AND s.active=1 AND NOT EXISTS(SELECT 1 FROM alert_events e WHERE e.endpoint=r.endpoint AND e.condition=r.condition AND e.kind='reminder') ORDER BY r.next_reminder,r.condition LIMIT 1",
            params![endpoint.as_str(),now], |r| Ok((r.get(0)?,r.get(1)?))).optional()?;
        let Some((condition, episode)) = due else {
            tx.commit()?;
            return Ok(false);
        };
        let key: AlertKey = serde_json::from_str(&condition)?;
        let (responder, value, evaluated_at): (String, Option<String>, i64) = tx.query_row(
            "SELECT responder,value,evaluated_at FROM alert_state WHERE key=?1",
            [&condition],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        let raised_at: i64 = tx.query_row(
            "SELECT time FROM alert_history WHERE sequence=?1 AND active=1",
            [episode],
            |r| r.get(0),
        )?;
        let state = AlertState {
            key,
            responder,
            active: true,
            raised_at: Some(raised_at),
            cleared_at: None,
            value: value
                .map(|v| v.parse())
                .transpose()
                .map_err(|_| Error::Corruption)?,
            evaluated_at,
        };
        enqueue(
            &tx,
            endpoint,
            &state,
            (episode, raised_at),
            EventKind::Reminder,
            now,
        )?;
        tx.execute(
            "UPDATE alert_reminders SET next_reminder=?3 WHERE endpoint=?1 AND condition=?2",
            params![endpoint.as_str(), condition, next],
        )?;
        tx.commit()?;
        Ok(true)
    }
}

fn enqueue(
    tx: &Transaction<'_>,
    endpoint: &Identifier,
    state: &AlertState,
    episode: (i64, i64),
    kind: EventKind,
    now: i64,
) -> Result<(), Error> {
    let namespace: String = tx.query_row(
        "SELECT identity FROM alert_installation WHERE singleton=1",
        [],
        |r| r.get(0),
    )?;
    let id: String = tx.query_row("SELECT lower(hex(randomblob(16)))", [], |r| r.get(0))?;
    let payload = event::build(&namespace, &id, state, episode, kind)?;
    tx.execute(
        "INSERT INTO alert_events VALUES(?1,?2,?3,?4,?5,?6,?7)",
        params![
            id,
            endpoint.as_str(),
            crate::key(&state.key)?,
            episode.0,
            kind.as_str(),
            state.evaluated_at,
            payload
        ],
    )?;
    tx.execute(
        "INSERT INTO alert_progress(event,next_attempt) VALUES(?1,?2)",
        params![id, now],
    )?;
    Ok(())
}
