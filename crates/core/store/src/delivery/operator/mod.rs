//! Lists delivery states and bounded operator metadata, and resends deliveries with the same ID and bytes (ADR 0008).
use crate::delivery::outbox::outbox_owners;
use crate::{Audit, DeliveryError as Error, DeliveryKind, OrderingKey, RecentOutcome, Store, key};
use analyte_model::{DeliveryId, Identifier};
use rusqlite::{OptionalExtension, params};
#[cfg(test)]
mod tests;

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
    /// The app does not take this kind of delivery: a Needs review notice it
    /// answered with 404 or 501, or one queued for it after that. Finished,
    /// like Done, but never accepted (ADR 0038).
    Declined,
}

impl DeliveryState {
    /// Every state but Done, named so that reads of what waits go through the
    /// index by lab app and state instead of every delivery. A delivery in
    /// Done holds no attempt after its own latest accepted one, since every
    /// attempt moves a delivery out of Done unless it is itself accepted; so a
    /// try after a lab app's last accepted delivery always belongs to a
    /// delivery in one of these.
    pub(crate) const NOT_DONE: [Self; 5] = [
        Self::Pending,
        Self::InFlight,
        Self::Parked,
        Self::Paused,
        Self::Conflict,
    ];
}

/// Bounded operator listing without clinical payloads.
#[derive(Debug, Clone, PartialEq, Eq)]
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

/// Which deliveries an operator listing shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryFilter {
    /// Every delivery.
    All,
    /// Every delivery the app has not accepted yet: every state but the
    /// finished ones, done and declined.
    Undelivered,
}

/// One delivery as an operator page lists it: what was stored about it, never
/// its bytes, values or test codes; sample IDs leave the store as stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedDelivery {
    /// Its identity, destination and state.
    pub delivery: StoredDelivery,
    /// A result or an order message.
    pub kind: DeliveryKind,
    /// When it was queued, in Unix seconds; none when an older version did not
    /// record it and the time was filled in later.
    pub queued_at: Option<i64>,
    /// The machines whose result or order it carries, at most
    /// [`UNDELIVERED_MACHINES`], in name order; empty when none is kept.
    pub machines: Vec<Identifier>,
    /// For a result, what its reading produced; none for an order message or
    /// when the reading is no longer kept.
    pub reading: Option<RecentOutcome>,
}

/// How many deliveries each [`DeliveryFilter`] lists.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DeliveryCounts {
    /// Every delivery.
    pub all: u64,
    /// Those the app has not accepted yet.
    pub undelivered: u64,
}

/// What the app has not accepted yet for one destination, without payloads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Undelivered {
    /// Destination chosen when the bytes were built.
    pub destination: Identifier,
    /// Items a worker still sends: waiting, in flight or scheduled for a retry.
    pub sending: u64,
    /// Items held for an operator: paused, parked or conflicting.
    pub held: u64,
    /// Machines whose results or orders these items carry, at most
    /// [`UNDELIVERED_MACHINES`] of them, in name order.
    pub machines: Vec<Identifier>,
}

impl Undelivered {
    /// Every item the app has not accepted yet: still sending plus held.
    #[must_use]
    pub fn waiting(&self) -> u64 {
        self.sending.saturating_add(self.held)
    }
}

/// How many machine names one [`Undelivered`] lists.
pub const UNDELIVERED_MACHINES: usize = 8;

impl Store {
    /// Lists, per destination, what the app has not accepted yet; destinations with
    /// nothing open are left out. At most `page_rows` destinations, in name order.
    /// # Errors
    /// Reports database failures, corrupt counts and corrupt stored names.
    pub fn undelivered_by_destination(&self) -> Result<Vec<Undelivered>, Error> {
        let rows: Vec<(String, i64, i64)> = self
            .db
            .prepare(concat!(
                "SELECT destination,sum(state IN (?1,?2)),sum(state IN (?3,?4,?5)) FROM outbox ",
                "WHERE state NOT IN (?6,?8) GROUP BY destination ORDER BY destination LIMIT ?7"
            ))?
            .query_map(
                params![
                    DeliveryState::Pending,
                    DeliveryState::InFlight,
                    DeliveryState::Paused,
                    DeliveryState::Parked,
                    DeliveryState::Conflict,
                    DeliveryState::Done,
                    i64::try_from(self.limits.page_rows)
                        .map_err(|_| Error::InvalidDeliveryInput)?,
                    DeliveryState::Declined,
                ],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )?
            .collect::<Result<_, _>>()?;
        let mut machines = self.db.prepare(concat!(
            "WITH owners AS (",
            outbox_owners!("o.destination=?1 AND (o.state=?2 OR o.state=?3 OR o.state=?4 OR o.state=?5 OR o.state=?6)"),
            ")",
            " SELECT DISTINCT machine FROM owners ORDER BY machine LIMIT ?7"
        ))?;
        let [pending, in_flight, parked, paused, conflict] = DeliveryState::NOT_DONE;
        rows.into_iter()
            .map(|(destination, sending, held)| {
                let names: Vec<String> = machines
                    .query_map(
                        params![
                            destination,
                            pending,
                            in_flight,
                            parked,
                            paused,
                            conflict,
                            i64::try_from(UNDELIVERED_MACHINES)
                                .map_err(|_| Error::InvalidDeliveryInput)?
                        ],
                        |r| r.get(0),
                    )?
                    .collect::<Result<_, _>>()?;
                Ok(Undelivered {
                    destination: Identifier::new(&destination).map_err(|_| Error::Corruption)?,
                    sending: u64::try_from(sending).map_err(|_| Error::Corruption)?,
                    held: u64::try_from(held).map_err(|_| Error::Corruption)?,
                    machines: names
                        .iter()
                        .map(|name| Identifier::new(name).map_err(|_| Error::Corruption))
                        .collect::<Result<_, _>>()?,
                })
            })
            .collect()
    }

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

    /// Lists a bounded page of deliveries after a sequence cursor, with what
    /// `filter` keeps and what was stored about each: its kind, when it was
    /// queued, its machines and, for a result, its reading's result count and
    /// sample IDs. Reading changes nothing.
    /// # Errors
    /// Reports database failures and corrupt stored identities, states or evidence.
    pub fn listed_deliveries(
        &self,
        after: i64,
        filter: DeliveryFilter,
    ) -> Result<Vec<ListedDelivery>, Error> {
        self.listed_deliveries_up_to(after, filter, self.limits.page_rows)
    }

    /// Like [`Store::listed_deliveries`], with at most `limit` rows, so a page
    /// that shows fewer than a store page builds only those it may show.
    /// # Errors
    /// Reports database failures and corrupt stored identities, states or evidence.
    pub fn listed_deliveries_up_to(
        &self,
        after: i64,
        filter: DeliveryFilter,
        limit: usize,
    ) -> Result<Vec<ListedDelivery>, Error> {
        type Row = (
            i64,
            String,
            String,
            DeliveryState,
            DeliveryKind,
            i64,
            bool,
            Option<String>,
        );
        let rows: Vec<Row> = self
            .db
            .prepare("SELECT o.sequence,o.id,o.destination,o.state,o.kind,o.enqueued_at,o.enqueue_time_backfilled,(SELECT evidence FROM interpretations i WHERE i.receipt=o.receipt AND i.version=o.version) FROM outbox o WHERE o.sequence>?1 AND (?2=0 OR o.state NOT IN (?3,?5)) ORDER BY o.sequence LIMIT ?4")?
            .query_map(
                params![
                    after,
                    filter == DeliveryFilter::Undelivered,
                    DeliveryState::Done,
                    i64::try_from(limit.min(self.limits.page_rows))
                        .map_err(|_| Error::InvalidDeliveryInput)?,
                    DeliveryState::Declined,
                ],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                        r.get(6)?,
                        r.get(7)?,
                    ))
                },
            )?
            .collect::<Result<_, _>>()?;
        let mut machines = self.db.prepare(concat!(
            "WITH owners AS (",
            outbox_owners!("o.id=?1"),
            ")",
            " SELECT DISTINCT machine FROM owners WHERE machine IS NOT NULL ORDER BY machine LIMIT ?2"
        ))?;
        rows.into_iter()
            .map(
                |(sequence, id, destination, state, kind, queued, backfilled, evidence)| {
                    let names: Vec<String> = machines
                        .query_map(
                            params![
                                id,
                                i64::try_from(UNDELIVERED_MACHINES)
                                    .map_err(|_| Error::InvalidDeliveryInput)?
                            ],
                            |r| r.get(0),
                        )?
                        .collect::<Result<_, _>>()?;
                    let reading = evidence
                        .map(|evidence| {
                            serde_json::from_str(&evidence).map(|read| RecentOutcome::of(&read))
                        })
                        .transpose()?;
                    Ok(ListedDelivery {
                        delivery: StoredDelivery {
                            sequence,
                            id: serde_json::from_str(&id)?,
                            destination: Identifier::new(&destination)
                                .map_err(|_| Error::Corruption)?,
                            state,
                        },
                        kind,
                        queued_at: (!backfilled).then_some(queued),
                        machines: names
                            .iter()
                            .map(|name| Identifier::new(name).map_err(|_| Error::Corruption))
                            .collect::<Result<_, _>>()?,
                        reading,
                    })
                },
            )
            .collect()
    }

    /// Counts the deliveries each [`DeliveryFilter`] lists.
    /// # Errors
    /// Reports database failures and corrupt counts.
    pub fn delivery_counts(&self) -> Result<DeliveryCounts, Error> {
        let (all, undelivered): (i64, Option<i64>) = self.db.query_row(
            "SELECT count(*),sum(state NOT IN (?1,?2)) FROM outbox",
            [DeliveryState::Done, DeliveryState::Declined],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        Ok(DeliveryCounts {
            all: u64::try_from(all).map_err(|_| Error::Corruption)?,
            undelivered: u64::try_from(undelivered.unwrap_or(0)).map_err(|_| Error::Corruption)?,
        })
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
            DeliveryState::Done
            | DeliveryState::Parked
            | DeliveryState::Paused
            | DeliveryState::Declined => {}
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
            DeliveryKind::OrderEvent | DeliveryKind::ReviewNotice => {}
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

impl std::fmt::Display for DeliveryState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Pending => "Pending",
            Self::InFlight => "InFlight",
            Self::Done => "Done",
            Self::Parked => "Parked",
            Self::Paused => "Paused",
            Self::Conflict => "Conflict",
            Self::Declined => "Declined",
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
        (DeliveryState::Declined, "Declined"),
    ] {
        assert_eq!(value.to_string(), expected);
    }
}
