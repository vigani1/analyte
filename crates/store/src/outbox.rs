//! Outbox storage responsibilities.
use crate::{Attempt, Audit, Claim, DeliveryKind, Error, Store, key};
use analyte_model::{DeliveryId, Identifier};
use rusqlite::{OptionalExtension, params};

impl Store {
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
            Attempt::Interrupted => ("pending", now),
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
            Attempt::Done | Attempt::Interrupted | Attempt::RetryAt(_) | Attempt::Paused(_) => {}
        }
        tx.commit()?;
        Ok(())
    }

    /// Recovers one bounded page of interrupted app deliveries for idempotent retry.
    /// Call before starting the sole worker until zero is returned. IDs and bytes
    /// remain immutable; parked, conflicting and paused entries are never changed.
    /// Each recovered entry receives its own audit with the delivery as its target.
    /// # Errors
    /// Reports capacity, encoding or database failures; each page commits atomically.
    pub fn recover_in_flight(&mut self, now: i64, audit: &Audit) -> Result<usize, Error> {
        if !self
            .db
            .prepare("SELECT 1 FROM outbox WHERE state='in_flight' LIMIT 1")?
            .exists([])?
        {
            return Ok(0);
        }
        let tx = self.db.transaction()?;
        let ids = {
            let mut query = tx.prepare(
                "SELECT id FROM outbox WHERE state='in_flight' ORDER BY sequence LIMIT ?1",
            )?;
            query
                .query_map(
                    [i64::try_from(self.limits.page_rows).map_err(|_| Error::InvalidInput)?],
                    |row| row.get::<_, String>(0),
                )?
                .collect::<Result<Vec<_>, _>>()?
        };
        let mut required = 0usize;
        for id in &ids {
            let delivery: String = serde_json::from_str(id)?;
            let entry = Audit {
                actor: audit.actor.clone(),
                action: audit.action.clone(),
                targets: vec![Identifier::new(&delivery).map_err(|_| Error::Corruption)?],
                time: now,
                reason: audit.reason.clone(),
            };
            let encoded = key(&entry)?;
            if encoded.len() > self.limits.record_bytes {
                return Err(Error::InvalidInput);
            }
            let outcome = key(&Attempt::Interrupted)?;
            required = required
                .checked_add(encoded.len())
                .and_then(|bytes| bytes.checked_add(outcome.len()))
                .and_then(|bytes| bytes.checked_add(128))
                .ok_or(Error::Capacity)?;
            Self::capacity_on(&tx, &self.path, &self.limits, &*self.space, required)?;
            tx.execute(
                "INSERT INTO attempts(delivery,outcome,time) VALUES (?1,?2,?3)",
                params![id, outcome, now],
            )?;
            tx.execute("INSERT INTO audit(evidence) VALUES (?1)", [encoded])?;
            tx.execute(
                "UPDATE outbox SET state='pending',next_attempt=?2 WHERE id=?1",
                params![id, now],
            )?;
        }
        tx.commit()?;
        Ok(ids.len())
    }

    /// Explicitly releases a parked, conflicting or paused delivery with its existing bytes and ID.
    /// Interrupted attempts use startup recovery instead; operator release records setup or evidence repair.
    /// # Errors
    /// Refuses pending/done items, oversized audit evidence or database failures.
    pub fn release_delivery(&mut self, id: DeliveryId, audit: &Audit) -> Result<(), Error> {
        let encoded = self.encode(audit)?;
        self.bound(encoded.as_bytes())?;
        let tx = self.db.transaction()?;
        if tx.execute("UPDATE outbox SET state='pending',next_attempt=0 WHERE id=?1 AND state IN ('parked','paused','conflict')", [key(&id)?])? != 1 { return Err(Error::Conflict); }
        tx.execute(
            "INSERT INTO audit(evidence) VALUES (?1)",
            [key(&(id, encoded))?],
        )?;
        tx.commit()?;
        Ok(())
    }
}
