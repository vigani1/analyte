//! Lists delivery states and bounded operator metadata, and resends deliveries with the same ID and bytes (ADR 0008).
use crate::{Audit, DeliveryError as Error, DeliveryKind, OrderingKey, Store, key};
use analyte_model::{DeliveryId, Identifier};
use rusqlite::{OptionalExtension, params};

/// Durable app delivery state, independent of machine order lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryState {
    /// Waiting for a delivery attempt.
    Pending,
    /// Attempt outcome is not yet known.
    InFlight,
    /// App accepted the delivery or reported a duplicate.
    Done,
    /// App rejected the delivery.
    Parked,
    /// Destination setup needs repair.
    Paused,
    /// App reported conflicting content under this delivery identity.
    Conflict,
}

/// Bounded operator listing without clinical payloads.
pub struct StoredDelivery {
    /// Pagination cursor, independent of UUID ordering.
    pub sequence: i64,
    /// Stable app delivery identity.
    pub id: DeliveryId,
    /// Destination chosen when bytes were constructed.
    pub destination: Identifier,
    /// Current durable delivery outcome.
    pub state: DeliveryState,
}

impl Store {
    /// Reads one delivery's operator metadata without exposing its bytes.
    /// # Errors
    /// Reports database failures or corrupt stored metadata; unknown IDs return absence.
    pub fn delivery(&self, id: DeliveryId) -> Result<Option<StoredDelivery>, Error> {
        let row: Option<(i64, String, DeliveryState)> = self
            .db
            .query_row(
                "SELECT sequence,destination,state FROM outbox WHERE id=?1",
                [key(&id)?],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        row.map(|(sequence, destination, state)| {
            Ok(StoredDelivery {
                sequence,
                id,
                destination: Identifier::new(&destination).map_err(|_| Error::Corruption)?,
                state,
            })
        })
        .transpose()
    }

    /// Lists a bounded page of deliveries after a sequence cursor.
    /// # Errors
    /// Reports database failures and corrupt stored identities or states.
    pub fn deliveries(&self, after: i64) -> Result<Vec<StoredDelivery>, Error> {
        let mut statement = self.db.prepare("SELECT sequence,id,destination,state FROM outbox WHERE sequence>?1 ORDER BY sequence LIMIT ?2")?;
        let rows = statement.query_map(
            params![
                after,
                i64::try_from(self.limits.page_rows).map_err(|_| Error::InvalidDeliveryInput)?
            ],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, DeliveryState>(3)?,
                ))
            },
        )?;
        rows.map(|row| {
            let (sequence, id, destination, state) = row?;
            Ok(StoredDelivery {
                sequence,
                id: serde_json::from_str(&id)?,
                destination: Identifier::new(&destination).map_err(|_| Error::Corruption)?,
                state,
            })
        })
        .collect()
    }

    /// Requeues existing bytes and ID with one atomic operator audit (plan §11).
    /// `explicit_superseded` acknowledges resending the specifically named old delivery.
    /// Audit action must be `delivery-resend` and its targets must name this delivery.
    /// Cancellation after submission may leave the resend committed; inspect its audit.
    /// # Errors
    /// Distinguishes unknown, superseded and ineligible deliveries; refuses invalid
    /// audit evidence, insufficient capacity and database failures without changing state.
    pub fn resend_delivery(
        &mut self,
        id: DeliveryId,
        explicit_superseded: bool,
        audit: &Audit,
    ) -> Result<(), Error> {
        type ResendRow = (
            DeliveryState,
            DeliveryKind,
            String,
            Option<String>,
            Option<i64>,
        );
        if self.delivery_was_purged(id)? {
            return Err(Error::Purged);
        }
        let target = format!("delivery:{}", serde_json::from_str::<String>(&key(&id)?)?);
        if audit.action.as_str() != "delivery-resend"
            || !audit.targets.iter().any(|item| item.as_str() == target)
        {
            return Err(Error::AuditMismatch);
        }
        let encoded = self.encode(audit)?;
        self.capacity(encoded.len())?;
        let tx = self
            .db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let id_key = key(&id)?;
        let row: Option<ResendRow> = tx
            .query_row(
                "SELECT state,kind,ordering_key,receipt,version FROM outbox WHERE id=?1",
                [&id_key],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .optional()?;
        let (state, kind, ordering, receipt, version) = row.ok_or(Error::UnknownDelivery)?;
        match state {
            DeliveryState::Done | DeliveryState::Parked | DeliveryState::Paused => {}
            DeliveryState::Pending | DeliveryState::InFlight | DeliveryState::Conflict => {
                return Err(Error::ResendState);
            }
        }
        match kind {
            DeliveryKind::Result => {
                let ordering: OrderingKey = serde_json::from_str(&ordering)?;
                let identity = key(&(&ordering.machine, &ordering.observation))?;
                let revision: Option<i64> = tx.query_row("SELECT max(revision) FROM observation_revisions WHERE identity_key=?1 AND receipt=?2 AND interpretation_version=?3", params![identity, receipt, version], |r| r.get(0))?;
                let revision = revision.ok_or(Error::Corruption)?;
                let superseded: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM observation_revisions WHERE identity_key=?1 AND revision>?2)", params![identity, revision], |r| r.get(0))?;
                if superseded && !explicit_superseded {
                    return Err(Error::SupersededDelivery);
                }
            }
            DeliveryKind::OrderEvent => {}
        }
        tx.execute(
            "UPDATE outbox SET state=?2,next_attempt=0 WHERE id=?1",
            params![id_key, DeliveryState::Pending],
        )?;
        tx.execute("INSERT INTO audit(evidence) VALUES (?1)", [encoded])?;
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;

impl std::fmt::Display for DeliveryState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Pending => "Pending",
            Self::InFlight => "InFlight",
            Self::Done => "Done",
            Self::Parked => "Parked",
            Self::Paused => "Paused",
            Self::Conflict => "Conflict",
        })
    }
}

#[test]
fn delivery_state_spellings_are_compatible() {
    for (value, expected) in [
        (DeliveryState::Pending, "Pending"),
        (DeliveryState::InFlight, "InFlight"),
        (DeliveryState::Done, "Done"),
        (DeliveryState::Parked, "Parked"),
        (DeliveryState::Paused, "Paused"),
        (DeliveryState::Conflict, "Conflict"),
    ] {
        assert_eq!(value.to_string(), expected);
    }
}
