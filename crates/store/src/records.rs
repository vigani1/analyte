use analyte_model::{DeliveryId, Identifier, Reason, ResultContent, SampleId};
use serde::{Deserialize, Serialize};

/// Reproducible interpretation with exact effective config and typed result contents.
#[derive(Serialize, Deserialize)]
pub struct Interpretation {
    /// Driver name and version.
    pub driver: analyte_model::NamespacedId,
    /// Effective configuration version.
    pub config_version: Identifier,
    /// Exact effective configuration, excluding secrets.
    pub config: Vec<u8>,
    /// Typed contents or a specific failure.
    pub outcome: Reading,
}

/// Parsing never silently substitutes an empty successful result.
#[derive(Serialize, Deserialize)]
pub enum Reading {
    /// Typed results retained as JSON.
    Results(Vec<ResultContent>),
    /// Uninterpretable input is retained and parked.
    Failed(Reason),
}

/// Immutable delivery input; destination and ordering key are explicit.
pub struct Delivery {
    /// Stable delivery identity.
    pub id: DeliveryId,
    /// Configured app destination.
    pub destination: Identifier,
    /// Host-assigned machine and observation key, never based on value.
    pub ordering_key: OrderingKey,
    /// Exact wire bytes.
    pub payload: Vec<u8>,
    /// Wire contract version.
    pub contract_version: u32,
}

/// One claimed delivery; cancellation leaves it in flight for reconciliation.
pub struct Claim {
    /// Endpoint category retained at enqueue.
    pub kind: DeliveryKind,
    /// Number of durably recorded earlier attempts.
    pub earlier_attempts: u64,
    /// Identity reused on every retry.
    pub id: DeliveryId,
    /// Exact stored bytes.
    pub payload: Vec<u8>,
    /// Configured app destination for these bytes.
    pub destination: Identifier,
    /// Contract version captured at enqueue, independent of active configuration.
    pub contract_version: u32,
}

/// A machine's observation is independent of its value and revision.
#[derive(Serialize, Deserialize)]
pub struct OrderingKey {
    /// Source machine owning the observation.
    pub machine: Identifier,
    /// Trusted observation identity; corrections share this key.
    pub observation: analyte_model::ObservationIdentity,
}

/// Each attempt records its recovery decision and reason.
#[derive(Serialize)]
pub enum Attempt {
    /// App durably accepted or already received the delivery.
    Done,
    /// Uncertain or transient outcome, retry with the same bytes after this Unix second.
    RetryAt(i64),
    /// App rejected clinical delivery.
    Parked(Reason),
    /// Destination setup must be repaired.
    Paused(Reason),
    /// Same delivery identity had conflicting contents.
    Conflict(Reason),
}

/// Operator evidence linked to named identities.
#[derive(Serialize)]
pub struct Parked {
    /// Reason category, including unknown tests and unresolved identity.
    pub kind: Identifier,
    /// Specific explanation.
    pub reason: Reason,
    /// Linked receipt, delivery or result IDs.
    pub targets: Vec<Identifier>,
}

/// Permanent operator action, including resolution details.
#[derive(Serialize)]
pub struct Audit {
    /// Person or service responsible.
    pub actor: Identifier,
    /// Action performed.
    pub action: Identifier,
    /// Target identities.
    pub targets: Vec<Identifier>,
    /// Unix second of action.
    pub time: i64,
    /// Why and how the action was performed.
    pub reason: Reason,
}

/// Cached order evidence; expiry and tombstones never imply machine acceptance.
#[derive(Serialize, Deserialize)]
pub struct CachedOrder {
    /// App's order identity.
    pub id: Identifier,
    /// Exact sample ID.
    pub sample: SampleId,
    /// Full versioned order document, retained without interpretation.
    pub document: String,
    /// Deleted or withdrawn order.
    pub tombstone: bool,
    /// Expiry as a Unix second.
    pub expiry: i64,
}

/// Feed page and cursor form one atomic write.
#[derive(Serialize)]
pub struct OrderPage {
    /// App destination.
    pub destination: Identifier,
    /// Opaque next cursor.
    pub cursor: Identifier,
    /// Last snapshot page reached.
    pub complete: bool,
    /// Last successful poll Unix second.
    pub polled: i64,
    /// Changes, including tombstones.
    pub orders: Vec<CachedOrder>,
}

/// Lookup failure is distinct from a trustworthy empty answer.
pub enum Lookup {
    /// Snapshot is complete and fresh, with current orders.
    Orders(Vec<CachedOrder>),
    /// Snapshot is complete and fresh with no current orders.
    NoOrders,
    /// No safe answer can be given.
    NotTrusted,
}

/// Disjoint endpoint categories; their ordering domains never overlap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryKind {
    /// Clinical result tied to an interpretation.
    Result,
    /// Order lifecycle evidence tied to an app order.
    OrderEvent,
}

/// Immutable order lifecycle delivery, independent of result evidence.
pub struct OrderEventDelivery {
    /// Stable delivery identity.
    pub id: DeliveryId,
    /// Configured app destination.
    pub destination: Identifier,
    /// App order identity, also its ordering key.
    pub order_id: Identifier,
    /// Exact contract bytes.
    pub payload: Vec<u8>,
    /// Captured wire contract version.
    pub contract_version: u32,
}

/// Durable destination status. Cache freshness is evaluated by the host.
#[derive(Debug)]
pub struct Health {
    /// Unresolved deliveries, including paused, parked and in-flight items.
    pub pending_count: u64,
    /// Exact bytes held by unresolved deliveries.
    pub pending_bytes: u64,
    /// Seconds since oldest enqueue; absent when empty or clock moved backwards.
    pub oldest_pending_age: Option<u64>,
    /// At least one delivery has paused this destination.
    pub paused: bool,
    /// Last committed successful page time.
    pub last_successful_poll: Option<i64>,
    /// A complete snapshot has been committed.
    pub snapshot_complete: bool,
    /// Store trusts the snapshot; the host must also check freshness.
    pub cache_trusted: bool,
}
