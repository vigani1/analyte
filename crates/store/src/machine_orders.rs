//! Durable machine command reservations; a reserved identity is never sent twice.
use crate::{Audit, CachedOrder, Error, OrderEventDelivery, Store, digest, key};
use analyte_model::{Identifier, OrderState, SourceText};
use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use std::num::NonZeroU64;

/// Stable app namespace, order and machine identity for one attempted command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MachineOrderKey {
    /// App namespace, also the event destination.
    pub destination: Identifier,
    /// App order identity.
    pub order: Identifier,
    /// Installed machine identity.
    pub machine: Identifier,
}

/// Persisted lifecycle snapshot; history is append-only.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MachineOrder {
    /// Stable local operator identity.
    pub id: i64,
    /// App and machine ownership.
    pub key: MachineOrderKey,
    /// App order version reserved before sending.
    pub version: NonZeroU64,
    /// Latest validated lifecycle state.
    pub state: OrderState,
    /// First reservation Unix second.
    pub created_at: i64,
    /// Latest transition Unix second.
    pub updated_at: i64,
    /// Exact evidence code; never a guessed machine status.
    pub source_code: SourceText,
}

/// Specific refusal, separate from storage failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SendRefusal {
    /// The app no longer supplies this order.
    #[error("order is absent from the current cache generation")]
    Missing,
    /// Cache must be refreshed before sending.
    #[error("order cache is not complete and fresh")]
    Untrusted,
    /// A newer order must be mapped again.
    #[error("cached order version changed")]
    VersionChanged,
    /// Withdrawal forbids a new send.
    #[error("order is withdrawn")]
    Withdrawn,
    /// Expired work must not be sent.
    #[error("order expired")]
    Expired,
    /// Only requested orders are eligible.
    #[error("order is cancelled or is no longer requested")]
    NotRequested,
    /// A previous reservation blocks automatic replay, regardless of version.
    #[error("order already has a machine reservation")]
    AlreadyReserved,
    /// Explicit analyzer targets exclude this machine.
    #[error("order does not target this machine")]
    WrongMachine,
}

/// Bounded, forward-only operator listing.
pub struct MachineOrderFilter {
    /// Continue after this local identity.
    pub after: i64,
    /// Optional installed machine restriction.
    pub machine: Option<Identifier>,
    /// Optional lifecycle state restriction.
    pub state: Option<OrderState>,
}

/// Candidate evidence that must still be checked atomically against the cache.
pub struct OrderReservation<'a> {
    /// App and machine identity.
    pub identity: &'a MachineOrderKey,
    /// Version the host mapped.
    pub version: NonZeroU64,
    /// Current Unix second.
    pub now: i64,
    /// Maximum trusted cache age in seconds.
    pub freshness: u64,
}

#[derive(Deserialize)]
struct CachedFields {
    version: NonZeroU64,
    state: String,
    tombstone: bool,
    target_analyzers: Option<Vec<String>>,
}

impl Store {
    /// Returns a bounded page of unreserved cache candidates, never permission to send.
    /// The host must map a candidate and call `reserve_send` immediately before I/O.
    /// # Errors
    /// Reports database or retained cache decoding failures.
    pub fn order_candidates(
        &self,
        destination: &Identifier,
        machine: &Identifier,
        after: &str,
    ) -> Result<Vec<CachedOrder>, Error> {
        let mut statement = self.db.prepare("SELECT evidence FROM orders o WHERE destination=?1 AND id>?3 AND generation=(SELECT generation FROM feed WHERE destination=?1) AND NOT EXISTS(SELECT 1 FROM machine_orders m WHERE m.destination=o.destination AND m.order_id=o.id AND m.machine=?2) ORDER BY id LIMIT ?4")?;
        let rows = statement
            .query_map(
                params![
                    destination.as_str(),
                    machine.as_str(),
                    after,
                    i64::try_from(self.limits.page_rows).map_err(|_| Error::InvalidInput)?
                ],
                |r| r.get::<_, String>(0),
            )?
            .collect::<Result<Vec<_>, _>>()?;
        rows.iter()
            .map(|row| Ok(serde_json::from_str(row)?))
            .collect()
    }

    /// Atomically rechecks the current cache and persists Sent, history and outbox.
    /// Once committed, cancellation or a lost reply requires reconciliation,
    /// never a second automatic reservation. The host builds contract bytes
    /// from the exact snapshot passed to its builder inside this transaction.
    /// # Errors
    /// Returns storage/builder failures; expected eligibility refusals are values.
    pub fn reserve_send(
        &mut self,
        identity: &MachineOrderKey,
        version: NonZeroU64,
        now: i64,
        freshness: u64,
        build: impl FnOnce(&MachineOrder) -> Result<OrderEventDelivery, Error>,
    ) -> Result<Result<MachineOrder, SendRefusal>, Error> {
        let code = SourceText::new("host-reserved").map_err(|_| Error::InvalidInput)?;
        self.start_order(
            &OrderReservation {
                identity,
                version,
                now,
                freshness,
            },
            OrderState::Sent,
            code,
            build,
        )
    }

    /// Records a mapping rejection and app event without claiming anything was sent.
    /// # Errors
    /// Reports cache refusals, builder errors and storage failures.
    pub fn reject_unsent(
        &mut self,
        candidate: &OrderReservation<'_>,
        reason: SourceText,
        build: impl FnOnce(&MachineOrder) -> Result<OrderEventDelivery, Error>,
    ) -> Result<Result<MachineOrder, SendRefusal>, Error> {
        self.start_order(candidate, OrderState::Rejected, reason, build)
    }

    fn start_order(
        &mut self,
        candidate: &OrderReservation<'_>,
        initial: OrderState,
        code: SourceText,
        build: impl FnOnce(&MachineOrder) -> Result<OrderEventDelivery, Error>,
    ) -> Result<Result<MachineOrder, SendRefusal>, Error> {
        let OrderReservation {
            identity,
            version,
            now,
            freshness,
        } = *candidate;
        self.capacity(self.limits.record_bytes)?;
        let limit = self.limits.record_bytes;
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing = tx
            .query_row(
                "SELECT id FROM machine_orders WHERE destination=?1 AND order_id=?2 AND machine=?3",
                params![
                    identity.destination.as_str(),
                    identity.order.as_str(),
                    identity.machine.as_str()
                ],
                |r| r.get::<_, i64>(0),
            )
            .optional()?;
        if existing.is_some() {
            return Ok(Err(SendRefusal::AlreadyReserved));
        }
        let poll = tx
            .query_row(
                "SELECT polled FROM feed WHERE destination=?1 AND complete=1 AND trusted=1",
                [identity.destination.as_str()],
                |r| r.get::<_, i64>(0),
            )
            .optional()?;
        if poll
            .and_then(|p| now.checked_sub(p))
            .and_then(|age| u64::try_from(age).ok())
            .is_none_or(|age| age >= freshness)
        {
            return Ok(Err(SendRefusal::Untrusted));
        }
        let cached = tx.query_row("SELECT evidence FROM orders WHERE destination=?1 AND id=?2 AND generation=(SELECT generation FROM feed WHERE destination=?1)",params![identity.destination.as_str(),identity.order.as_str()], |r| r.get::<_,String>(0)).optional()?;
        let Some(cached) = cached else {
            return Ok(Err(SendRefusal::Missing));
        };
        let cached: CachedOrder = serde_json::from_str(&cached)?;
        let fields: CachedFields = serde_json::from_str(&cached.document)?;
        if fields.version != version {
            return Ok(Err(SendRefusal::VersionChanged));
        }
        if cached.tombstone || fields.tombstone {
            return Ok(Err(SendRefusal::Withdrawn));
        }
        if cached.expiry <= now {
            return Ok(Err(SendRefusal::Expired));
        }
        if fields.state != "requested" {
            return Ok(Err(SendRefusal::NotRequested));
        }
        if fields
            .target_analyzers
            .as_ref()
            .is_some_and(|targets| !targets.iter().any(|t| t == identity.machine.as_str()))
        {
            return Ok(Err(SendRefusal::WrongMachine));
        }
        let state = OrderState::Requested.transition(initial)?;
        tx.execute("INSERT INTO machine_orders(destination,order_id,machine,version,state,created_at,updated_at,source_code) VALUES (?1,?2,?3,?4,?5,?6,?6,?7)",params![identity.destination.as_str(),identity.order.as_str(),identity.machine.as_str(),version.to_string(),key(&state)?,now,code.as_str()])?;
        let record = MachineOrder {
            id: tx.last_insert_rowid(),
            key: identity.clone(),
            version,
            state,
            created_at: now,
            updated_at: now,
            source_code: code,
        };
        append(&tx, &record, &build(&record)?, limit)?;
        tx.commit()?;
        Ok(Ok(record))
    }

    /// Returns a bounded page without exposing cache payloads.
    /// # Errors
    /// Reports database failures or invalid persisted evidence.
    pub fn machine_orders(&self, filter: &MachineOrderFilter) -> Result<Vec<MachineOrder>, Error> {
        let mut statement = self.db.prepare("SELECT id FROM machine_orders WHERE id>?1 AND (?2 IS NULL OR machine=?2) AND (?3 IS NULL OR state=?3) ORDER BY id LIMIT ?4")?;
        let state = filter.state.map(|s| key(&s)).transpose()?;
        let ids = statement
            .query_map(
                params![
                    filter.after,
                    filter.machine.as_ref().map(Identifier::as_str),
                    state,
                    i64::try_from(self.limits.page_rows).map_err(|_| Error::InvalidInput)?
                ],
                |r| r.get::<_, i64>(0),
            )?
            .collect::<Result<Vec<_>, _>>()?;
        ids.into_iter().map(|id| load(&self.db, id)).collect()
    }

    /// Persists a legal transition, history and its exact app event together.
    /// # Errors
    /// Refuses missing orders, illegal moves, clock reversal, bounds and I/O failures.
    pub fn transition_order(
        &mut self,
        id: i64,
        to: OrderState,
        code: SourceText,
        now: i64,
        build: impl FnOnce(&MachineOrder) -> Result<OrderEventDelivery, Error>,
    ) -> Result<MachineOrder, Error> {
        self.change_order(id, to, code, now, None, build)
    }

    /// Reconciles an uncertain order with an operator's audited evidence.
    /// # Errors
    /// Refuses non-uncertain orders, unsupported outcomes and invalid audit targets.
    pub fn reconcile_order(
        &mut self,
        id: i64,
        outcome: OrderState,
        audit: &Audit,
        build: impl FnOnce(&MachineOrder) -> Result<OrderEventDelivery, Error>,
    ) -> Result<MachineOrder, Error> {
        if !matches!(
            outcome,
            OrderState::Accepted | OrderState::Rejected | OrderState::CancelConfirmed
        ) || !audit
            .targets
            .iter()
            .any(|target| target.as_str() == id.to_string())
        {
            return Err(Error::InvalidInput);
        }
        let code = SourceText::new("operator-reconciled").map_err(|_| Error::InvalidInput)?;
        self.change_order(id, outcome, code, audit.time, Some(audit), build)
    }

    fn change_order(
        &mut self,
        id: i64,
        to: OrderState,
        code: SourceText,
        now: i64,
        audit: Option<&Audit>,
        build: impl FnOnce(&MachineOrder) -> Result<OrderEventDelivery, Error>,
    ) -> Result<MachineOrder, Error> {
        self.capacity(self.limits.record_bytes)?;
        let limit = self.limits.record_bytes;
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut record = load(&tx, id)?;
        if now < record.updated_at || audit.is_some() && record.state != OrderState::Uncertain {
            return Err(Error::Conflict);
        }
        record.state = record.state.transition(to)?;
        record.updated_at = now;
        record.source_code = code;
        tx.execute(
            "UPDATE machine_orders SET state=?1,updated_at=?2,source_code=?3 WHERE id=?4",
            params![key(&record.state)?, now, record.source_code.as_str(), id],
        )?;
        append(&tx, &record, &build(&record)?, limit)?;
        if let Some(audit) = audit {
            let evidence = key(audit)?;
            if evidence.len() > limit {
                return Err(Error::InvalidInput);
            }
            tx.execute("INSERT INTO audit(evidence) VALUES (?1)", [evidence])?;
        }
        tx.commit()?;
        Ok(record)
    }
}

fn load(db: &rusqlite::Connection, id: i64) -> Result<MachineOrder, Error> {
    let evidence: String = db.query_row("SELECT evidence FROM machine_order_history WHERE machine_order=?1 ORDER BY sequence DESC LIMIT 1", [id], |r| r.get(0))?;
    Ok(serde_json::from_str(&evidence)?)
}

fn append(
    tx: &Transaction<'_>,
    record: &MachineOrder,
    event: &OrderEventDelivery,
    limit: usize,
) -> Result<(), Error> {
    let evidence = key(record)?;
    if evidence.len() > limit
        || event.payload.len() > limit
        || event.destination != record.key.destination
        || event.order_id != record.key.order
        || event.contract_version != 1
    {
        return Err(Error::InvalidInput);
    }
    tx.execute(
        "INSERT INTO machine_order_history(machine_order,evidence) VALUES (?1,?2)",
        params![record.id, evidence],
    )?;
    tx.execute("INSERT INTO outbox(id,kind,order_id,destination,ordering_key,payload,digest,contract,state,next_attempt,enqueued_at) VALUES (?1,'order_event',?2,?3,?2,?4,?5,?6,'pending',0,?7)",params![key(&event.id)?,event.order_id.as_str(),event.destination.as_str(),event.payload,digest(&event.payload).0.as_slice(),event.contract_version,record.updated_at])?;
    Ok(())
}
