//! Version 1 wire documents, independent of the clinical model.
#![forbid(unsafe_code)]
mod response;
mod signing;
pub use response::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub use signing::*;
/// Maximum document size, including unknown fields.
pub const MAX_DOCUMENT_BYTES: usize = 1_048_576;
/// Rejected document, without input values in its display text.
#[derive(Debug, thiserror::Error)]
pub enum DocumentError {
    /// Complete document exceeds the limit.
    #[error("document exceeds size limit")]
    TooLarge,
    /// Invalid JSON or field type.
    #[error("invalid document")]
    Json(#[from] serde_json::Error),
    /// Contract field constraint failed.
    #[error("invalid field: {0}")]
    Field(&'static str),
}
/// Nonempty text preserved exactly, limited to 4096 UTF-8 bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String")]
pub struct Text(String);
impl TryFrom<String> for Text {
    type Error = DocumentError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.is_empty() || value.len() > 4096 || value.contains('\0') {
            return Err(DocumentError::Field("text"));
        }
        Ok(Self(value))
    }
}
impl Text {
    /// Returns the exact source text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
macro_rules! wire {
    ($name:ident { $($field:ident : $ty:ty),* $(,)? }) => {
        #[doc = concat!("Version 1 ", stringify!($name), "; unknown fields are ignored.")]
        #[derive(Debug, Clone, Serialize, Deserialize)]
        pub struct $name {
            $(#[doc = concat!("The ", stringify!($field), " value, defined in docs/contract/v1.md.")]
            pub $field: $ty),*
        }
    };
}
macro_rules! choices {
    ($name:ident { $($variant:ident),* }) => {
        #[doc = concat!("Explicit ", stringify!($name), " alternatives.")]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum $name { $(#[doc = stringify!($variant)] $variant),* }
    };
}
choices!(SummaryStatus {
    Preliminary,
    Final,
    Corrected,
    Cancelled,
    Unknown
});
choices!(Applied { Yes, No, Unknown });
choices!(QcClass {
    Patient,
    Control,
    Calibration,
    Unknown
});
choices!(ProcessingMode {
    Production,
    Training,
    Debug
});
choices!(Comparator { Eq, Lt, Le, Gt, Ge });
choices!(OrderState {
    Requested,
    Sent,
    Accepted,
    Done,
    CancelRequested,
    Cancelled,
    Rejected,
    Uncertain
});
choices!(EventState {
    Sent,
    Accepted,
    Rejected,
    Done,
    CancelConfirmed,
    Uncertain
});
choices!(Priority {
    Routine,
    Urgent,
    Stat
});
choices!(IdSource {
    Barcode,
    Typed,
    Host,
    Analyzer,
    Unknown
});
/// Identity evidence; an unresolved result never receives an invented key.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Identity {
    /// Source-assigned identity with a positive revision.
    Resolved {
        /// Assigning system.
        namespace: Text,
        /// Source identity.
        id: Text,
        /// Positive source revision.
        revision: std::num::NonZeroU64,
    },
    /// Insufficient evidence to identify a result.
    Unresolved {
        /// Why identity cannot be resolved.
        reason: Text,
    },
}
wire!(Source {
    analyzer: Text,
    connection: Text
});
wire!(Receipt {
    id: Text,
    received_at: Text,
    raw_sha256: Text
});
wire!(Position { rack: Option<Text>, slot: Text });
wire!(Sample { id: Text, id_source: IdSource, position: Option<Position> });
wire!(Decimal {
    number: Text,
    comparator: Comparator
});
wire!(Value { text: Text, decimal: Option<Decimal> });
wire!(Dilution { factor: Option<Text>, applied: Applied });
wire!(MeasuredTime { text: Text, parsed: Option<Text> });
wire!(ResultDocument {
    identity: Identity, test: Text, source_status: Text, summary_status: SummaryStatus,
    value: Value, original_unit: Option<Text>, ucum_unit: Option<Text>, dilution: Dilution,
    flags: Vec<Text>, reference_range: Option<Text>, qc_class: QcClass,
    processing_mode: ProcessingMode, measured_at: Option<MeasuredTime>, comments: Vec<Text>
});
wire!(DeliveryFields {
    contract_version: u8, delivery_id: Text, created_at: Text, source: Source,
    receipt: Receipt, sample: Sample, results: Vec<ResultDocument>
});
wire!(EventFields {
    contract_version: u8, delivery_id: Text, created_at: Text, source: Source,
    order_id: Text, order_version: std::num::NonZeroU64, state: EventState,
    occurred_at: Text, source_status: Text, reason: Option<Text>
});
wire!(OrderFields {
    id: Text, version: std::num::NonZeroU64, state: OrderState, sample_id: Text,
    tests: Vec<Text>, priority: Priority, target_analyzers: Option<Vec<Text>>,
    expires_at: Option<Text>, tombstone: bool
});
wire!(OrdersPageFields { contract_version: u8, orders: Vec<Order>, next_cursor: Text, complete: bool });
wire!(ErrorBody {
    contract_version: u8,
    error: Text,
    detail: Text
});
/// Delivery validated at construction, including deserialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(try_from = "DeliveryFields")]
pub struct Delivery(DeliveryFields);
impl TryFrom<DeliveryFields> for Delivery {
    type Error = DocumentError;
    fn try_from(fields: DeliveryFields) -> Result<Self, Self::Error> {
        envelope(
            fields.contract_version,
            &fields.delivery_id,
            &fields.created_at,
        )?;
        timestamp(&fields.receipt.received_at)?;
        let hash = fields.receipt.raw_sha256.as_str();
        if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(DocumentError::Field("raw_sha256"));
        }
        list(&fields.results, 1000, false)?;
        for result in &fields.results {
            list(&result.flags, 100, true)?;
            list(&result.comments, 100, true)?;
            if let Some(decimal) = &result.value.decimal {
                exact_decimal(&decimal.number)?;
            }
            if let Some(factor) = &result.dilution.factor {
                exact_decimal(factor)?;
            }
            if let Some(time) = &result.measured_at
                && let Some(parsed) = &time.parsed
            {
                timestamp(parsed)?;
            }
            if result.processing_mode != ProcessingMode::Production {
                return Err(DocumentError::Field("processing_mode"));
            }
        }
        Ok(Self(fields))
    }
}
impl Delivery {
    /// Borrows validated fields without allowing mutation.
    #[must_use]
    pub fn fields(&self) -> &DeliveryFields {
        &self.0
    }
}
/// Event with validated version, version-7 UUID and timestamps.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(try_from = "EventFields")]
pub struct OrderEvent(EventFields);
impl TryFrom<EventFields> for OrderEvent {
    type Error = DocumentError;
    fn try_from(fields: EventFields) -> Result<Self, Self::Error> {
        envelope(
            fields.contract_version,
            &fields.delivery_id,
            &fields.created_at,
        )?;
        timestamp(&fields.occurred_at)?;
        Ok(Self(fields))
    }
}
impl OrderEvent {
    /// Borrows validated event fields.
    #[must_use]
    pub fn fields(&self) -> &EventFields {
        &self.0
    }
}
fn envelope(version: u8, id: &Text, time: &Text) -> Result<(), DocumentError> {
    if version != 1 {
        return Err(DocumentError::Field("contract_version"));
    }
    let uuid =
        uuid::Uuid::parse_str(id.as_str()).map_err(|_| DocumentError::Field("delivery_id"))?;
    if uuid.get_version_num() != 7 {
        return Err(DocumentError::Field("delivery_id"));
    }
    timestamp(time)
}
fn timestamp(time: &Text) -> Result<(), DocumentError> {
    time.as_str()
        .parse::<jiff::Timestamp>()
        .map_err(|_| DocumentError::Field("timestamp"))?;
    Ok(())
}
fn list<T>(items: &[T], max: usize, empty: bool) -> Result<(), DocumentError> {
    if items.len() > max || (!empty && items.is_empty()) {
        return Err(DocumentError::Field("list size"));
    }
    Ok(())
}
fn exact_decimal(value: &Text) -> Result<(), DocumentError> {
    let unsigned = value.as_str().strip_prefix('-').unwrap_or(value.as_str());
    let mut parts = unsigned.split('.');
    let whole = parts.next().unwrap_or("");
    let fraction = parts.next();
    if whole.is_empty()
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || fraction.is_some_and(|p| p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()))
        || parts.next().is_some()
    {
        return Err(DocumentError::Field("decimal"));
    }
    Ok(())
}
/// Decodes bounded JSON, ignoring unknown fields.
///
/// # Errors
/// Returns a size, JSON or field error.
pub fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, DocumentError> {
    if bytes.len() > MAX_DOCUMENT_BYTES {
        return Err(DocumentError::TooLarge);
    }
    Ok(serde_json::from_slice(bytes)?)
}
/// SHA-256 of exact bytes, without normalization.
#[must_use]
pub fn content_digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

/// Compares credential digests in constant time, hiding matching prefixes.
#[must_use]
pub fn credential_matches(left: &[u8], right: &[u8]) -> bool {
    use subtle::ConstantTimeEq as _;
    bool::from(content_digest(left).ct_eq(&content_digest(right)))
}

/// Order whose lists, expiry and version are validated once.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(try_from = "OrderFields")]
pub struct Order(OrderFields);
impl TryFrom<OrderFields> for Order {
    type Error = DocumentError;
    fn try_from(fields: OrderFields) -> Result<Self, Self::Error> {
        list(&fields.tests, 100, fields.tombstone)?;
        if let Some(targets) = &fields.target_analyzers {
            list(targets, 100, false)?;
        }
        if let Some(time) = &fields.expires_at {
            timestamp(time)?;
        }
        Ok(Self(fields))
    }
}
impl Order {
    /// Borrows validated order fields.
    #[must_use]
    pub fn fields(&self) -> &OrderFields {
        &self.0
    }
}

/// A validated version-1 page, bounded to 100 orders.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(try_from = "OrdersPageFields")]
pub struct OrdersPage(OrdersPageFields);
impl TryFrom<OrdersPageFields> for OrdersPage {
    type Error = DocumentError;
    fn try_from(fields: OrdersPageFields) -> Result<Self, Self::Error> {
        if fields.contract_version != 1 {
            return Err(DocumentError::Field("contract_version"));
        }
        if fields.next_cursor.as_str().len() > 256 {
            return Err(DocumentError::Field("cursor"));
        }
        list(&fields.orders, 100, true)?;
        Ok(Self(fields))
    }
}
impl OrdersPage {
    /// Borrows validated page fields.
    #[must_use]
    pub fn fields(&self) -> &OrdersPageFields {
        &self.0
    }
}
