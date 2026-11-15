//! Durable machine command reservations; a reserved identity is never sent twice.
use crate::{Audit, CachedOrder, Error, OrderEventDelivery, Store, digest, key};
use analyte_model::{Identifier, NamespacedId, OrderState, SourceText};
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
    /// Durable command items; legacy v4 coverage is explicitly unknown.
    #[serde(default)]
    pub items: Vec<MachineOrderItem>,
}

const RESERVATION_OWNER_PREFIX: &str = "host-reserved:";

impl MachineOrder {
    /// True only while this connection's retained reservation is still Sending.
    /// Completed outcomes and another connection's reservation are never owned here.
    #[must_use]
    pub fn reserved_by(&self, owner: &Identifier) -> bool {
        self.state == OrderState::Sending
            && self
                .source_code
                .as_str()
                .strip_prefix(RESERVATION_OWNER_PREFIX)
                == Some(owner.as_str())
    }
}

/// Retained command identity and test coverage, independent of app order revisions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MachineOrderItem {
    /// Parent machine order identity.
    pub machine_order: i64,
    /// One-based stable item index.
    pub index: u16,
    /// Exact app tests covered by this command.
    pub tests: Vec<NamespacedId>,
    /// False only for migrated evidence whose original coverage was not retained.
    pub coverage_known: bool,
    /// State before a cancellation request; retained to handle explicit refusal.
    #[serde(default)]
    pub cancel_from: Option<OrderState>,
    /// Store-assigned EI.1; callers supply the configured EI.2 separately.
    pub placer: String,
    /// EI.2 retained with the item so config changes cannot change a cancellation identity.
    #[serde(default)]
    pub placer_namespace: Option<Identifier>,
    /// Last evidenced lifecycle state.
    pub state: OrderState,
    /// Last evidence time.
    pub updated_at: i64,
    /// Specific source outcome or migration limitation.
    pub source_code: SourceText,
}

/// One explicit application outcome at exchange completion.
pub struct ItemOutcome {
    /// Stable one-based item index.
    pub index: u16,
    /// Peer evidence; a refusal is not a confirmed cancellation.
    pub decision: ItemDecision,
    /// Exact peer code/reason, retained in protected history.
    pub code: SourceText,
}

/// Application evidence accepted by the host; never permission to resend.
#[derive(Debug, Clone, Copy)]
pub enum ItemDecision {
    /// Transfer/display application receipt confirmed (not order acceptance).
    Sent,
    /// Explicit acceptance of new work.
    Accepted,
    /// Explicit rejection of new work.
    Rejected,
    /// No reliable outcome, including timeout/disconnect.
    Uncertain,
    /// Explicit confirmation of cancellation.
    CancelConfirmed,
    /// Explicit refusal restores the previous state, retaining the reason.
    CancelRefused,
}

/// Reservation shape; protocol code cannot supply its own collision-prone identities.
#[derive(Debug, Clone, Copy)]
pub enum OrderItems<'a> {
    /// One item covers all tests in an ASTM/vendor record or query display.
    Together,
    /// One item per HL7 test, bounded by the encoded EI field length.
    PerTest {
        /// Required configured placer namespace.
        namespace: &'a Identifier,
        /// Edition's maximum encoded EI length.
        identity_bytes: usize,
    },
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
    tests: Vec<NamespacedId>,
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

    /// Atomically rechecks the current cache and persists Sending and history without an app event.
    /// Once committed, cancellation or a lost reply requires reconciliation,
    /// never a second automatic reservation. Only a later real outcome builds
    /// contract bytes; an internal reservation must not be reported as sent.
    /// # Errors
    /// Returns storage failures; expected eligibility refusals are values.
    pub fn reserve_send(
        &mut self,
        identity: &MachineOrderKey,
        version: NonZeroU64,
        now: i64,
        freshness: u64,
    ) -> Result<Result<MachineOrder, SendRefusal>, Error> {
        let code = SourceText::new("host-reserved").map_err(|_| Error::InvalidInput)?;
        self.start_order(
            &OrderReservation {
                identity,
                version,
                now,
                freshness,
            },
            OrderState::Sending,
            code,
            OrderItems::Together,
            None::<fn(&MachineOrder) -> Result<OrderEventDelivery, Error>>,
        )
    }

    /// Reserves query work with a retained connection owner for cancelled-wait recovery.
    /// Only the matching owner may treat an interrupted request as its own reservation.
    /// # Errors
    /// Reports storage failures; expected eligibility refusals remain explicit values.
    pub fn reserve_send_owned(
        &mut self,
        candidate: &OrderReservation<'_>,
        owner: &Identifier,
    ) -> Result<Result<MachineOrder, SendRefusal>, Error> {
        let code = SourceText::new(&format!("{RESERVATION_OWNER_PREFIX}{}", owner.as_str()))
            .map_err(|_| Error::InvalidInput)?;
        self.start_order(
            candidate,
            OrderState::Sending,
            code,
            OrderItems::Together,
            None::<fn(&MachineOrder) -> Result<OrderEventDelivery, Error>>,
        )
    }

    /// Reserves per-test HL7 items atomically with the parent, without an app event.
    /// # Errors
    /// Refuses oversized identities, missing/oversized test coverage and I/O failures.
    pub fn reserve_send_items(
        &mut self,
        candidate: &OrderReservation<'_>,
        items: OrderItems<'_>,
    ) -> Result<Result<MachineOrder, SendRefusal>, Error> {
        self.start_order(
            candidate,
            OrderState::Sending,
            SourceText::new("host-reserved").map_err(|_| Error::InvalidInput)?,
            items,
            None::<fn(&MachineOrder) -> Result<OrderEventDelivery, Error>>,
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
        self.start_order(
            candidate,
            OrderState::Rejected,
            reason,
            OrderItems::Together,
            Some(build),
        )
    }

    fn start_order(
        &mut self,
        candidate: &OrderReservation<'_>,
        initial: OrderState,
        code: SourceText,
        items: OrderItems<'_>,
        build: Option<impl FnOnce(&MachineOrder) -> Result<OrderEventDelivery, Error>>,
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
        let mut record = MachineOrder {
            id: tx.last_insert_rowid(),
            key: identity.clone(),
            version,
            state,
            created_at: now,
            updated_at: now,
            source_code: code,
            items: Vec::new(),
        };
        record.items = create_items(&tx, &record, fields.tests, items, limit)?;
        let event = build.map(|build| build(&record)).transpose()?;
        append(&tx, &record, event.as_ref(), limit)?;
        tx.commit()?;
        Ok(Ok(record))
    }

    /// Looks up a reservation by its immutable key after an interrupted store wait.
    /// # Errors
    /// Reports storage failures or invalid retained evidence; absence is explicit.
    pub fn machine_order(&self, identity: &MachineOrderKey) -> Result<Option<MachineOrder>, Error> {
        let id = self
            .db
            .query_row(
                "SELECT id FROM machine_orders WHERE destination=?1 AND order_id=?2 AND machine=?3",
                params![
                    identity.destination.as_str(),
                    identity.order.as_str(),
                    identity.machine.as_str()
                ],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;
        id.map(|id| load(&self.db, id)).transpose()
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

    /// Commits completed exchange outcomes, conservative aggregation and one app event.
    /// Missing decisions become Uncertain. Duplicate/unknown items and illegal moves
    /// roll back the entire exchange, including its event. Cancellation cannot resend work.
    /// # Errors
    /// Refuses invalid identities, illegal item transitions, bounds and storage failures.
    pub fn finish_order_items(
        &mut self,
        id: i64,
        outcomes: &[ItemOutcome],
        now: i64,
        build: impl FnOnce(&MachineOrder) -> Result<OrderEventDelivery, Error>,
    ) -> Result<MachineOrder, Error> {
        self.capacity(self.limits.record_bytes)?;
        let limit = self.limits.record_bytes;
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut order = load(&tx, id)?;
        if !matches!(
            order.state,
            OrderState::Sending | OrderState::Sent | OrderState::CancelRequested
        ) || now < order.updated_at
            || outcomes.len() > order.items.len()
            || order.items.is_empty()
        {
            return Err(Error::InvalidInput);
        }
        let mut seen = std::collections::HashSet::new();
        for outcome in outcomes {
            if !seen.insert(outcome.index)
                || !order.items.iter().any(|item| item.index == outcome.index)
            {
                return Err(Error::InvalidInput);
            }
        }
        let cancelling = order.state == OrderState::CancelRequested;
        let mut refused = false;
        for item in &mut order.items {
            if let Some(outcome) = outcomes.iter().find(|outcome| outcome.index == item.index) {
                let next = item_decision(item, outcome.decision)?;
                refused |= matches!(outcome.decision, ItemDecision::CancelRefused);
                item.state = next;
                item.source_code = outcome.code.clone();
            } else {
                item.state = item_decision(item, ItemDecision::Uncertain)?;
                item.source_code =
                    SourceText::new("exchange-answer-missing").map_err(|_| Error::InvalidInput)?;
            }
            item.updated_at = now;
            write_item(&tx, item, limit)?;
        }
        let mut state = aggregate(&order.items);
        if cancelling
            && refused
            && !order
                .items
                .iter()
                .any(|item| item.state == OrderState::Uncertain)
        {
            let previous: String = tx.query_row("SELECT evidence FROM machine_order_history WHERE machine_order=?1 AND json_extract(evidence,'$.state') IN ('Sent','Accepted') ORDER BY sequence DESC LIMIT 1", [id], |row| row.get(0))?;
            state = serde_json::from_str::<MachineOrder>(&previous)?.state;
        }
        // Aggregation is a distinct rule: mixed item outcomes may produce Uncertain
        // even when one item is terminal. It never manufactures a Sent event.
        order.state = state;
        order.updated_at = now;
        let prefix = if refused {
            "CancelRefused"
        } else {
            "item-outcomes"
        };
        let summary = order
            .items
            .iter()
            .map(|item| {
                format!(
                    "{}={:?}:{}",
                    item.index,
                    item.state,
                    item.source_code.as_str()
                )
            })
            .collect::<Vec<_>>()
            .join("; ");
        order.source_code =
            SourceText::new(&format!("{prefix}: {summary}")).map_err(|_| Error::InvalidInput)?;
        tx.execute(
            "UPDATE machine_orders SET state=?1,updated_at=?2,source_code=?3 WHERE id=?4",
            params![key(&order.state)?, now, order.source_code.as_str(), id],
        )?;
        append(&tx, &order, Some(&build(&order)?), limit)?;
        tx.commit()?;
        Ok(order)
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
        if audit.is_none()
            && record.items.len() > 1
            && !matches!(to, OrderState::CancelRequested | OrderState::Uncertain)
        {
            return Err(Error::Conflict);
        }
        record.state = record.state.transition(to)?;
        for item in &mut record.items {
            if audit.is_none() {
                item.state.transition(to)?;
            }
            if to == OrderState::CancelRequested {
                item.cancel_from = Some(item.state);
            }
            item.state = to;
            item.updated_at = now;
            item.source_code = code.clone();
            write_item(&tx, item, limit)?;
        }
        record.updated_at = now;
        record.source_code = code;
        tx.execute(
            "UPDATE machine_orders SET state=?1,updated_at=?2,source_code=?3 WHERE id=?4",
            params![key(&record.state)?, now, record.source_code.as_str(), id],
        )?;
        append(&tx, &record, Some(&build(&record)?), limit)?;
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
    let mut record: MachineOrder = serde_json::from_str(&evidence)?;
    record.items = load_items(db, id)?;
    Ok(record)
}

fn append(
    tx: &Transaction<'_>,
    record: &MachineOrder,
    event: Option<&OrderEventDelivery>,
    limit: usize,
) -> Result<(), Error> {
    let evidence = key(record)?;
    if evidence.len() > limit {
        return Err(Error::InvalidInput);
    }
    tx.execute(
        "INSERT INTO machine_order_history(machine_order,evidence) VALUES (?1,?2)",
        params![record.id, evidence],
    )?;
    let Some(event) = event else {
        return Ok(());
    };
    if event.payload.len() > limit
        || event.destination != record.key.destination
        || event.order_id != record.key.order
        || event.contract_version != 1
    {
        return Err(Error::InvalidInput);
    }
    tx.execute("INSERT INTO outbox(id,kind,order_id,destination,ordering_key,payload,digest,contract,state,next_attempt,enqueued_at) VALUES (?1,'order_event',?2,?3,?2,?4,?5,?6,'pending',0,?7)",params![key(&event.id)?,event.order_id.as_str(),event.destination.as_str(),event.payload,digest(&event.payload).0.as_slice(),event.contract_version,record.updated_at])?;
    Ok(())
}

fn load_items(db: &rusqlite::Connection, id: i64) -> Result<Vec<MachineOrderItem>, Error> {
    let mut query = db.prepare("SELECT h.evidence FROM machine_order_item_history h WHERE h.machine_order=?1 AND h.sequence=(SELECT MAX(sequence) FROM machine_order_item_history WHERE machine_order=h.machine_order AND item_index=h.item_index) ORDER BY h.item_index LIMIT 101")?;
    let rows = query
        .query_map([id], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    if rows.is_empty() || rows.len() > 100 {
        return Err(Error::Corruption);
    }
    rows.iter()
        .map(|row| Ok(serde_json::from_str(row)?))
        .collect()
}

fn create_items(
    tx: &Transaction<'_>,
    order: &MachineOrder,
    tests: Vec<NamespacedId>,
    shape: OrderItems<'_>,
    limit: usize,
) -> Result<Vec<MachineOrderItem>, Error> {
    if tests.is_empty() || tests.len() > 100 {
        return Err(Error::InvalidInput);
    }
    let groups = match shape {
        OrderItems::Together => vec![tests],
        OrderItems::PerTest { .. } => tests.into_iter().map(|test| vec![test]).collect(),
    };
    let mut items = Vec::new();
    for (offset, tests) in groups.into_iter().enumerate() {
        let index = u16::try_from(offset.checked_add(1).ok_or(Error::InvalidInput)?)
            .map_err(|_| Error::InvalidInput)?;
        let placer = format!("{}-{index}", order.id);
        if let OrderItems::PerTest {
            namespace,
            identity_bytes,
        } = shape
            && (namespace.as_str().is_empty()
                || placer
                    .len()
                    .checked_add(namespace.as_str().len())
                    .and_then(|n| n.checked_add(1))
                    .is_none_or(|n| n > identity_bytes))
        {
            return Err(Error::ItemIdentityTooLong);
        }
        let item = MachineOrderItem {
            machine_order: order.id,
            index,
            tests,
            coverage_known: true,
            cancel_from: None,
            placer,
            placer_namespace: match shape {
                OrderItems::Together => None,
                OrderItems::PerTest { namespace, .. } => Some(namespace.clone()),
            },
            state: order.state,
            updated_at: order.updated_at,
            source_code: order.source_code.clone(),
        };
        tx.execute("INSERT INTO machine_order_items(machine_order,item_index,tests,coverage_known,placer,placer_namespace,state) VALUES (?1,?2,?3,1,?4,?5,?6)", params![order.id,index,key(&item.tests)?,item.placer,item.placer_namespace.as_ref().map(Identifier::as_str),key(&item.state)?])?;
        write_item(tx, &item, limit)?;
        items.push(item);
    }
    Ok(items)
}

fn write_item(tx: &Transaction<'_>, item: &MachineOrderItem, limit: usize) -> Result<(), Error> {
    let evidence = key(item)?;
    if evidence.len() > limit {
        return Err(Error::InvalidInput);
    }
    tx.execute(
        "UPDATE machine_order_items SET state=?1 WHERE machine_order=?2 AND item_index=?3",
        params![key(&item.state)?, item.machine_order, item.index],
    )?;
    tx.execute("INSERT INTO machine_order_item_history(machine_order,item_index,evidence) VALUES (?1,?2,?3)", params![item.machine_order,item.index,evidence])?;
    Ok(())
}

fn item_decision(item: &MachineOrderItem, decision: ItemDecision) -> Result<OrderState, Error> {
    let next = match decision {
        ItemDecision::Sent => OrderState::Sent,
        ItemDecision::Accepted => OrderState::Accepted,
        ItemDecision::Rejected => OrderState::Rejected,
        ItemDecision::Uncertain => OrderState::Uncertain,
        ItemDecision::CancelConfirmed => OrderState::CancelConfirmed,
        ItemDecision::CancelRefused => {
            if item.state != OrderState::CancelRequested {
                return Err(Error::Conflict);
            }
            return item
                .cancel_from
                .filter(|state| matches!(state, OrderState::Sent | OrderState::Accepted))
                .ok_or(Error::Conflict);
        }
    };
    let valid = match item.state {
        OrderState::Sending => matches!(
            next,
            OrderState::Sent | OrderState::Accepted | OrderState::Rejected | OrderState::Uncertain
        ),
        OrderState::Sent => matches!(
            next,
            OrderState::Accepted | OrderState::Rejected | OrderState::Uncertain
        ),
        OrderState::Accepted => matches!(next, OrderState::Rejected | OrderState::Uncertain),
        OrderState::CancelRequested => {
            matches!(next, OrderState::CancelConfirmed | OrderState::Uncertain)
        }
        OrderState::Requested
        | OrderState::Done
        | OrderState::Rejected
        | OrderState::CancelConfirmed
        | OrderState::Uncertain => false,
    };
    if !valid {
        return Err(Error::OrderTransition(analyte_model::TransitionError {
            from: item.state,
            to: next,
        }));
    }
    Ok(next)
}

fn aggregate(items: &[MachineOrderItem]) -> OrderState {
    for state in [
        OrderState::Accepted,
        OrderState::Rejected,
        OrderState::CancelConfirmed,
        OrderState::Sent,
    ] {
        if !items.is_empty() && items.iter().all(|item| item.state == state) {
            return state;
        }
    }
    OrderState::Uncertain
}

#[cfg(test)]
mod item_transition_tests {
    use super::*;

    #[test]
    fn adr0003_rule8_every_illegal_item_decision_is_refused()
    -> Result<(), Box<dyn std::error::Error>> {
        use OrderState::{
            Accepted as A, CancelConfirmed as C, CancelRequested as Q, Done as D, Rejected as R,
            Requested as N, Sending as G, Sent as S, Uncertain as U,
        };
        let decisions = [
            ItemDecision::Sent,
            ItemDecision::Accepted,
            ItemDecision::Rejected,
            ItemDecision::Uncertain,
            ItemDecision::CancelConfirmed,
            ItemDecision::CancelRefused,
        ];
        // Columns: Sent, Accepted, Rejected, Uncertain, CancelConfirmed, CancelRefused.
        for (state, allowed) in [
            (N, [false; 6]),
            (G, [true, true, true, true, false, false]),
            (S, [false, true, true, true, false, false]),
            (A, [false, false, true, true, false, false]),
            (Q, [false, false, false, true, true, true]),
            (D, [false; 6]),
            (R, [false; 6]),
            (C, [false; 6]),
            (U, [false; 6]),
        ] {
            let item = MachineOrderItem {
                machine_order: 1,
                index: 1,
                tests: Vec::new(),
                coverage_known: true,
                cancel_from: Some(A),
                placer: "1-1".into(),
                placer_namespace: None,
                state,
                updated_at: 1,
                source_code: SourceText::new("test")?,
            };
            for (decision, expected) in decisions.into_iter().zip(allowed) {
                assert_eq!(
                    item_decision(&item, decision).is_ok(),
                    expected,
                    "{state:?} {decision:?}"
                );
            }
        }
        Ok(())
    }
}
