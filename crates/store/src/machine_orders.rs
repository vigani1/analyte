//! Durable machine command reservations; a reserved identity is never sent twice.
pub(crate) mod ownership;
use crate::sql_types::SqlOrderState;
use ownership::OrderOwner;
mod cancel;
mod eligibility;
mod items;
mod listing;
mod reserve;
use crate::{Audit, CachedOrder, Error, OrderEventDelivery, Store, digest, key};
use analyte_model::{Identifier, NamespacedId, OrderState, OrderTransition, SourceText};
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
    #[serde(skip)]
    owner: OrderOwner,
    /// Durable command items; legacy v4 coverage is explicitly unknown.
    #[serde(default)]
    pub items: Vec<MachineOrderItem>,
}

const RESERVATION_OWNER_PREFIX: &str = "host-reserved:";

impl MachineOrder {
    /// True only for the connection that admitted this cancellation request.
    #[must_use]
    pub fn cancel_requested_by(&self, owner: &Identifier) -> bool {
        self.state == OrderState::CancelRequested
            && matches!(&self.owner, OrderOwner::Cancellation { connection, .. } if connection.as_ref() == Some(owner))
    }
    /// True only while this connection's retained reservation is still Sending.
    /// Completed outcomes and another connection's reservation are never owned here.
    #[must_use]
    pub fn reserved_by(&self, owner: &Identifier) -> bool {
        self.state == OrderState::Sending
            && matches!(&self.owner, OrderOwner::Reservation(connection) if connection == owner)
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
    /// Store-assigned local command identity; the namespace is retained separately.
    pub placer: String,
    /// Retained logical namespace; configuration changes cannot alter cancellation identity.
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

/// Caller-selected command coverage; the store owns collision-free local identities.
#[derive(Debug, Clone, Copy)]
pub enum OrderItems<'a> {
    /// One command covers the complete, current cached test list.
    Together,
    /// Explicit nonempty groups must cover the cached tests exactly once.
    Groups {
        /// Caller-selected grouping and order, rechecked against the current cache.
        groups: &'a [Vec<NamespacedId>],
        /// Logical namespace retained with each command, when supplied.
        namespace: Option<&'a Identifier>,
        /// Maximum bytes in the generated local identity, after caller-owned encoding overhead.
        max_identity_bytes: usize,
    },
}

/// Specific refusal, separate from storage failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SendRefusal {
    /// Restored history cannot establish which commands reached machines.
    #[error("order sending paused after restore; operator reconciliation required")]
    RestoreUnreconciled,
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
    state: OrderState,
    tombstone: bool,
    target_analyzers: Option<Vec<Identifier>>,
    tests: Vec<NamespacedId>,
}

/// Options for one atomic command reservation.
/// Preflight runs after identity assignment and before commit; it must perform no I/O.
pub struct ReserveOptions<'a> {
    /// Connection retaining ownership when an interrupted request needs reconciliation.
    pub owner: Option<&'a Identifier>,
    /// Command grouping and identity limits selected by the caller.
    pub items: OrderItems<'a>,
    /// Optional pure check of exact assigned identities; failure rolls back all writes.
    pub prepare: Option<ReservationPreflight<'a>>,
}

impl Default for ReserveOptions<'_> {
    fn default() -> Self {
        Self {
            owner: None,
            items: OrderItems::Together,
            prepare: None,
        }
    }
}

/// Pure reservation preflight; failure prevents the transaction from committing.
pub type ReservationPreflight<'a> = &'a mut dyn FnMut(&MachineOrder) -> Result<(), Error>;
#[derive(Clone, Copy)]
enum ChangeEvidence<'a> {
    Machine,
    Operator(&'a Audit),
    AppCancel(NonZeroU64, u64, &'a Identifier),
}

impl Store {
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
        self.change_order(id, to, code, now, ChangeEvidence::Machine, build)
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
        self.change_order(
            id,
            outcome,
            code,
            audit.time,
            ChangeEvidence::Operator(audit),
            build,
        )
    }

    fn change_order(
        &mut self,
        id: i64,
        to: OrderState,
        code: SourceText,
        now: i64,
        evidence: ChangeEvidence<'_>,
        build: impl FnOnce(&MachineOrder) -> Result<OrderEventDelivery, Error>,
    ) -> Result<MachineOrder, Error> {
        self.capacity(self.limits.record_bytes)?;
        let limit = self.limits.record_bytes;
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut record = load(&tx, id)?;
        let (audit, cancel_version) = match evidence {
            ChangeEvidence::Machine => (None, None),
            ChangeEvidence::Operator(audit) => (Some(audit), None),
            ChangeEvidence::AppCancel(version, freshness, owner) => {
                record.owner = OrderOwner::Cancellation {
                    connection: Some(owner.clone()),
                    version,
                };
                (None, Some((version, freshness)))
            }
        };
        if let Some((version, freshness)) = cancel_version {
            if !Self::order_trust_on(&tx, &record.key.destination, now, freshness)?.is_trusted() {
                return Err(Error::Conflict);
            }
            let cached:String=tx.query_row("SELECT evidence FROM orders WHERE destination=?1 AND id=?2 AND generation=(SELECT generation FROM feed WHERE destination=?1)",params![record.key.destination.as_str(),record.key.order.as_str()],|row|row.get(0))?;
            let cached: CachedOrder = serde_json::from_str(&cached)?;
            let fields: CachedFields = serde_json::from_str(&cached.document)?;
            if fields.version != version
                || !fields
                    .state
                    .eligible_for(
                        OrderState::CancelRequested,
                        fields.tombstone,
                        fields
                            .target_analyzers
                            .as_ref()
                            .map(|targets| targets.iter().map(Identifier::as_str)),
                        record.key.machine.as_str(),
                    )
                    .is_ok()
            {
                return Err(Error::Conflict);
            }
            let prior:i64=tx.query_row("SELECT count(*) FROM machine_order_history WHERE machine_order=?1 AND cancel_version=?2",params![id,version.to_string()],|row|row.get(0))?;
            if prior != 0 {
                return Err(Error::Conflict);
            }
        }
        if now < record.updated_at || audit.is_some() && record.state != OrderState::Uncertain {
            return Err(Error::Conflict);
        }
        if audit.is_none()
            && record.items.len() > 1
            && !matches!(to, OrderState::CancelRequested | OrderState::Uncertain)
        {
            return Err(Error::Conflict);
        }
        record.state = record.state.transition(OrderTransition::Advance(to))?;
        for item in &mut record.items {
            item.state.transition(OrderTransition::Advance(to))?;
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
            params![
                SqlOrderState(record.state),
                now,
                record.source_code.as_str(),
                id
            ],
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
    let (evidence, owner): (String, OrderOwner) = db.query_row("SELECT evidence,owner_kind,owner_id,cancel_version FROM machine_order_history WHERE machine_order=?1 ORDER BY sequence DESC LIMIT 1", [id], |row| Ok((row.get(0)?, OrderOwner::read(row)?)))?;
    let mut record: MachineOrder = serde_json::from_str(&evidence)?;
    record.owner = owner;
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
    let (kind, owner, version) = record.owner.columns();
    tx.execute(
        "INSERT INTO machine_order_history(machine_order,evidence,owner_kind,owner_id,cancel_version) VALUES (?1,?2,?3,?4,?5)",
        params![record.id, evidence, kind, owner, version],
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
    tx.execute("INSERT INTO outbox(id,kind,order_id,destination,ordering_key,payload,digest,contract,state,next_attempt,enqueued_at) VALUES (?1,?8,?2,?3,?2,?4,?5,?6,?9,0,?7)",params![key(&event.id)?,event.order_id.as_str(),event.destination.as_str(),event.payload,digest(&event.payload).0.as_slice(),event.contract_version,record.updated_at,crate::DeliveryKind::OrderEvent,crate::DeliveryState::Pending])?;
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
        OrderItems::Groups { groups, .. } => {
            if groups.is_empty()
                || groups.len() > 100
                || groups
                    .iter()
                    .any(|group| group.is_empty() || group.len() > 100)
            {
                return Err(Error::InvalidInput);
            }
            let mut remaining = tests;
            for test in groups.iter().flatten() {
                let position = remaining
                    .iter()
                    .position(|candidate| candidate == test)
                    .ok_or(Error::InvalidInput)?;
                remaining.remove(position);
            }
            if !remaining.is_empty() {
                return Err(Error::InvalidInput);
            }
            groups.to_vec()
        }
    };
    let mut items = Vec::new();
    for (offset, tests) in groups.into_iter().enumerate() {
        let index = u16::try_from(offset.checked_add(1).ok_or(Error::InvalidInput)?)
            .map_err(|_| Error::InvalidInput)?;
        let placer = format!("{}-{index}", order.id);
        if let OrderItems::Groups {
            max_identity_bytes, ..
        } = shape
            && placer.len() > max_identity_bytes
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
                OrderItems::Groups { namespace, .. } => namespace.cloned(),
            },
            state: order.state,
            updated_at: order.updated_at,
            source_code: order.source_code.clone(),
        };
        tx.execute("INSERT INTO machine_order_items(machine_order,item_index,tests,coverage_known,placer,placer_namespace,state) VALUES (?1,?2,?3,1,?4,?5,?6)", params![order.id,index,key(&item.tests)?,item.placer,item.placer_namespace.as_ref().map(Identifier::as_str),SqlOrderState(item.state)])?;
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
        params![SqlOrderState(item.state), item.machine_order, item.index],
    )?;
    tx.execute("INSERT INTO machine_order_item_history(machine_order,item_index,evidence) VALUES (?1,?2,?3)", params![item.machine_order,item.index,evidence])?;
    Ok(())
}

fn item_decision(item: &MachineOrderItem, decision: ItemDecision) -> Result<OrderState, Error> {
    let change = match decision {
        ItemDecision::Sent => OrderTransition::Advance(OrderState::Sent),
        ItemDecision::Accepted => OrderTransition::Advance(OrderState::Accepted),
        ItemDecision::Rejected => OrderTransition::Advance(OrderState::Rejected),
        ItemDecision::Uncertain => OrderTransition::Advance(OrderState::Uncertain),
        ItemDecision::CancelConfirmed => OrderTransition::Advance(OrderState::CancelConfirmed),
        ItemDecision::CancelRefused => {
            OrderTransition::CancelRefused(item.cancel_from.ok_or(Error::Conflict)?)
        }
    };
    Ok(item.state.transition(change)?)
}

fn aggregate(states: &[OrderState]) -> OrderState {
    for state in [
        OrderState::Accepted,
        OrderState::Rejected,
        OrderState::CancelConfirmed,
        OrderState::Sent,
    ] {
        if !states.is_empty() && states.iter().all(|item| *item == state) {
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
        // ADR 0003 rule 8 / ORDERS.md: items use the same evidenced moves as orders.
        // finish_order_items separately requires a reserved exchange.
        // Columns: Sent, Accepted, Rejected, Uncertain, CancelConfirmed, CancelRefused.
        for (state, allowed) in [
            (N, [false, false, true, false, false, false]),
            (G, [true, true, true, true, false, false]),
            (S, [false, true, true, true, false, false]),
            (A, [false, false, true, true, false, false]),
            (Q, [false, false, true, true, true, true]),
            (D, [false; 6]),
            (R, [false; 6]),
            (C, [false; 6]),
            (U, [false, true, true, false, true, false]),
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
