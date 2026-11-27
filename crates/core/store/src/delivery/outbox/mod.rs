//! Claims queued deliveries in per-key order, records attempt outcomes and requeues interrupted in-flight items.
use crate::{
    Attempt, Audit, Claim, DeliveryError as Error, DeliveryKind, DeliveryState, Store, key,
};
use analyte_model::{DeliveryId, Identifier};
use rusqlite::{OptionalExtension, params};
#[cfg(test)]
mod tests;

/// SQL listing each outbox item `o` that matches `$filter` with the machine that owns
/// it, as `(item, machine)` rows: a result through its receipt, an order event through
/// the machine orders it reports. The outbox's kind check keeps the two branches apart:
/// a result never has an order and an order event never has a receipt.
macro_rules! outbox_owners {
    ($filter:literal) => {
        concat!(
            "SELECT o.id AS item, json_extract(r.metadata,'$.machine') AS machine ",
            "FROM outbox o JOIN receipts r ON r.id=o.receipt WHERE ",
            $filter,
            " UNION ALL SELECT o.id, m.machine FROM outbox o JOIN machine_orders m ",
            "ON m.order_id=o.order_id AND m.destination=o.destination WHERE ",
            $filter
        )
    };
}
pub(crate) use outbox_owners;

/// Counts `destination`'s items the app has not accepted yet: every state but
/// done and declined, the two finished ones.
pub(crate) fn count_undelivered(
    db: &rusqlite::Connection,
    destination: &Identifier,
) -> rusqlite::Result<i64> {
    let [pending, in_flight, parked, paused, conflict] = DeliveryState::NOT_DONE;
    db.query_row(
        "SELECT count(*) FROM outbox WHERE destination=?1 AND state IN (?2,?3,?4,?5,?6)",
        params![
            destination.as_str(),
            pending,
            in_flight,
            parked,
            paused,
            conflict
        ],
        |r| r.get(0),
    )
}

impl Store {
    /// Claims this destination's eligible head item; an unresolved earlier item blocks only its
    /// own key, and a paused item blocks the whole destination.
    /// # Errors
    /// Reports database failures; cancellation retains the original in-flight identity.
    pub fn claim_destination(
        &mut self,
        now: i64,
        destination: &Identifier,
    ) -> Result<Option<Claim>, Error> {
        let tx = self.db.transaction()?;
        let found = tx.query_row("SELECT id,payload,destination,contract,kind,(SELECT count(*) FROM attempts a WHERE a.delivery=o.id) FROM outbox o WHERE state=?2 AND next_attempt<=?1 AND o.destination=?5 AND NOT EXISTS(SELECT 1 FROM outbox older WHERE older.destination=o.destination AND older.kind=o.kind AND older.ordering_key=o.ordering_key AND older.sequence<o.sequence AND older.state NOT IN (?3,?6)) AND NOT EXISTS(SELECT 1 FROM outbox paused WHERE paused.destination=o.destination AND paused.state=?4) ORDER BY sequence LIMIT 1", params![now, DeliveryState::Pending, DeliveryState::Done, DeliveryState::Paused, destination.as_str(), DeliveryState::Declined], |r| Ok((r.get::<_,String>(0)?,r.get::<_,Vec<u8>>(1)?,r.get::<_,String>(2)?,r.get::<_,u32>(3)?,r.get::<_,DeliveryKind>(4)?,r.get::<_,i64>(5)?))).optional()?;
        let result =
            if let Some((id, payload, destination, contract_version, kind, earlier_attempts)) =
                found
            {
                tx.execute(
                    "UPDATE outbox SET state=?2 WHERE id=?1",
                    params![id, DeliveryState::InFlight],
                )?;
                Some(Claim {
                    id: serde_json::from_str(&id)?,
                    payload,
                    destination: Identifier::new(&destination).map_err(|_| Error::Corruption)?,
                    contract_version,
                    kind,
                    earlier_attempts: u64::try_from(earlier_attempts)
                        .map_err(|_| Error::Corruption)?,
                })
            } else {
                None
            };
        tx.commit()?;
        Ok(result)
    }

    /// Counts this destination's items the app has not accepted yet, in any state but
    /// done: waiting, in flight, scheduled for a retry, paused, parked or conflicting.
    /// A delivery-only worker stops when this reaches zero.
    /// # Errors
    /// Reports database failures or a corrupt count.
    pub fn undelivered(&self, destination: &Identifier) -> Result<u64, Error> {
        u64::try_from(count_undelivered(&self.db, destination)?).map_err(|_| Error::Corruption)
    }

    /// Records an attempt and its next state atomically, retaining bytes and delivery ID.
    /// # Errors
    /// Refuses outcomes for items that are not in flight.
    pub fn attempted(&mut self, id: DeliveryId, outcome: &Attempt, now: i64) -> Result<(), Error> {
        self.bound(crate::document(outcome)?.as_bytes())?;
        let (state, next) = match outcome {
            Attempt::Done => (DeliveryState::Done, 0),
            Attempt::RetryAt(time) => (DeliveryState::Pending, *time),
            Attempt::Interrupted => (DeliveryState::Pending, now),
            Attempt::Parked(_) => (DeliveryState::Parked, 0),
            Attempt::Paused(_) => (DeliveryState::Paused, 0),
            Attempt::Conflict(_) => (DeliveryState::Conflict, 0),
            Attempt::Declined(_) => (DeliveryState::Declined, 0),
        };
        let tx = self.db.transaction()?;
        let id = key(&id)?;
        if matches!(outcome, Attempt::Declined(_))
            && !crate::delivery::notices::is_notice(&tx, &id)?
        {
            return Err(Error::InvalidDeliveryInput);
        }
        if tx.execute(
            "UPDATE outbox SET state=?2,next_attempt=?3 WHERE id=?1 AND state=?4",
            params![id, state, next, DeliveryState::InFlight],
        )? != 1
        {
            return Err(Error::Conflict);
        }
        tx.execute(
            "INSERT INTO attempts(delivery,outcome,time) VALUES (?1,?2,?3)",
            params![id, crate::document(outcome)?, now],
        )?;
        match outcome {
            Attempt::Parked(reason) | Attempt::Conflict(reason) => {
                tx.execute(
                    "INSERT INTO parked(evidence) VALUES (?1)",
                    [crate::parked::evidence(
                        match outcome {
                            Attempt::Parked(_) => crate::ParkedKind::DeliveryRejected.as_str(),
                            Attempt::Conflict(_) => crate::ParkedKind::DeliveryConflict.as_str(),
                            Attempt::Done
                            | Attempt::Interrupted
                            | Attempt::RetryAt(_)
                            | Attempt::Paused(_)
                            | Attempt::Declined(_) => return Err(Error::InvalidDeliveryInput),
                        },
                        &serde_json::from_str::<String>(&id)?,
                        reason.as_str(),
                        None,
                    )?],
                )?;
            }
            Attempt::Declined(reason) => {
                crate::delivery::notices::decline(&tx, &id, reason, now)?;
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
            .prepare("SELECT 1 FROM outbox WHERE state=?1 LIMIT 1")?
            .exists([DeliveryState::InFlight])?
        {
            return Ok(0);
        }
        let tx = self.db.transaction()?;
        let ids = {
            let mut query =
                tx.prepare("SELECT id FROM outbox WHERE state=?2 ORDER BY sequence LIMIT ?1")?;
            query
                .query_map(
                    params![
                        i64::try_from(self.limits.page_rows)
                            .map_err(|_| Error::InvalidDeliveryInput)?,
                        DeliveryState::InFlight
                    ],
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
            let encoded = crate::document(&entry)?;
            if encoded.len() > self.limits.record_bytes {
                return Err(Error::Storage(crate::StorageError::LimitExceeded {
                    what: "record_bytes",
                }));
            }
            let outcome = crate::document(&Attempt::Interrupted)?;
            required = required
                .checked_add(encoded.len())
                .and_then(|bytes| bytes.checked_add(outcome.len()))
                .and_then(|bytes| bytes.checked_add(128))
                .ok_or(Error::Storage(crate::StorageError::Capacity))?;
            Self::capacity_on(&tx, &self.path, &self.limits, &*self.space, required)?;
            tx.execute(
                "INSERT INTO attempts(delivery,outcome,time) VALUES (?1,?2,?3)",
                params![id, outcome, now],
            )?;
            tx.execute("INSERT INTO audit(evidence) VALUES (?1)", [encoded])?;
            tx.execute(
                "UPDATE outbox SET state=?3,next_attempt=?2 WHERE id=?1",
                params![id, now, DeliveryState::Pending],
            )?;
        }
        tx.commit()?;
        Ok(ids.len())
    }
}
