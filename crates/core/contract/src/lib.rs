//! Defines versioned integration documents and HTTP signatures without clinical interpretation.
#![forbid(unsafe_code)]
pub mod document;
mod response;
pub mod schema;
pub use document::ResultDocument;
mod signing;
pub use response::{DeliveryStatus, ReplyProblem, ResponseAction, StatusBody, classify_response};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub use signing::{SignatureError, SigningSecret, api_key_valid};
/// Maximum document size, including unknown fields.
pub const MAX_DOCUMENT_BYTES: usize = 1_048_576;
/// Rejected document, without input values in its display text.
#[derive(Debug, thiserror::Error)]
pub enum DocumentError {
    /// A host-only reservation is not an app lifecycle event.
    #[error("internal Sending state cannot be represented by contract v1")]
    InternalOrderState,
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

impl schemars::JsonSchema for Text {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Text".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schema::envelope_text()
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
    ($name:ident { $($(#[$attr:meta])* $field:ident : $ty:ty),* $(,)? }) => {
        #[doc = concat!("Version 1 ", stringify!($name), "; unknown fields are ignored.")]
        #[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
        pub struct $name {
            $(#[doc = concat!("The ", stringify!($field), " value, defined in docs/contract/v1.md.")]
            $(#[$attr])* pub $field: $ty),*
        }
    };
}

macro_rules! choices {
    ($name:ident { $($variant:ident),* }) => {
        #[doc = concat!("Explicit ", stringify!($name), " alternatives.")]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
        #[serde(rename_all = "snake_case")]
        pub enum $name { $(#[doc = stringify!($variant)] $variant),* }
    };
}

choices!(Priority {
    Routine,
    Urgent,
    Stat
});
// ADR 0033: `model` is the machine definition's vendor/model folder key, the
// key lab apps file their per-model test and specimen tables under.
wire!(Source {
    analyzer: Text,
    connection: Text,
    #[schemars(
        description = "The machine model: the definition's vendor/model folder key (ADR 0033). Key per-model test and specimen tables on it."
    )]
    model: document::ModelId
});
wire!(Receipt {
    #[schemars(with = "schema::Uuid7")]
    id: Text,
    #[schemars(with = "schema::Timestamp")]
    received_at: Text,
    #[schemars(with = "schema::Sha256")]
    raw_sha256: Text,
    driver: document::NamespacedId,
    config_version: document::Identifier,
    #[schemars(with = "schema::Sha256")]
    config_sha256: Text
});
wire!(DeliveryFields {
    #[schemars(with = "schema::VersionOne")]
    contract_version: u8,
    #[schemars(with = "schema::Uuid7")]
    delivery_id: Text,
    #[schemars(with = "schema::Timestamp")]
    created_at: Text,
    source: Source,
    receipt: Receipt,
    #[schemars(length(min = 1, max = 1000))]
    results: Vec<ResultDocument>
});
wire!(EventFields {
    #[schemars(with = "schema::VersionOne")]
    contract_version: u8,
    #[schemars(with = "schema::Uuid7")]
    delivery_id: Text,
    #[schemars(with = "schema::Timestamp")]
    created_at: Text,
    source: Source,
    order_id: Text,
    order_version: std::num::NonZeroU64,
    state: document::OrderState,
    #[schemars(with = "schema::Timestamp")]
    occurred_at: Text,
    source_status: document::SourceText,
    reason: Option<Text>
});
wire!(Patient {
    id: document::NamespacedId
});
// ADR 0029: the code is the machine's own, in its code system; the optional
// name is the lab app's name for it, written only where a protocol carries one.
wire!(OrderTest {
    namespace: document::Identifier, value: document::Identifier,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<Text>
});
wire!(OrderFields {
    id: Text, version: std::num::NonZeroU64, state: document::OrderState, sample_id: Text,
    #[schemars(length(max = 100))]
    tests: Vec<OrderTest>,
    priority: Priority,
    #[schemars(length(min = 1, max = 100))]
    target_analyzers: Option<Vec<Text>>,
    #[schemars(with = "Option<schema::Timestamp>")]
    expires_at: Option<Text>,
    tombstone: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    position: Option<document::SamplePosition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    specimen_type: Option<document::NamespacedId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    patient: Option<Patient>
});
wire!(OrdersPageFields {
    #[schemars(with = "schema::VersionOne")]
    contract_version: u8,
    #[schemars(length(max = 100))]
    orders: Vec<Order>,
    #[schemars(with = "schema::Cursor")]
    next_cursor: Text,
    complete: bool
});
wire!(ErrorBody {
    #[schemars(with = "schema::VersionOne")]
    contract_version: u8,
    error: Text,
    detail: Text
});
/// Delivery validated at construction, including deserialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(try_from = "DeliveryFields")]
pub struct Delivery(DeliveryFields);
schema::wrapper!(Delivery, DeliveryFields, schema::delivery);
impl TryFrom<DeliveryFields> for Delivery {
    type Error = DocumentError;
    fn try_from(fields: DeliveryFields) -> Result<Self, Self::Error> {
        envelope(
            fields.contract_version,
            &fields.delivery_id,
            &fields.created_at,
        )?;
        timestamp(&fields.receipt.received_at)?;
        digest_field(&fields.receipt.raw_sha256, "raw_sha256")?;
        digest_field(&fields.receipt.config_sha256, "config_sha256")?;
        list(&fields.results, 1000, false)?;
        let receipt_id = document::ReceiptId::try_from(
            uuid::Uuid::parse_str(fields.receipt.id.as_str())
                .map_err(|_| DocumentError::Field("receipt_id"))?,
        )
        .map_err(|_| DocumentError::Field("receipt_id"))?;
        for result in &fields.results {
            if result.receipt != receipt_id {
                return Err(DocumentError::Field("receipt_id_mismatch"));
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
schema::wrapper!(OrderEvent, EventFields, schema::order_event);
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
    if uuid.get_version_num() != 7 || uuid.get_variant() != uuid::Variant::RFC4122 {
        return Err(DocumentError::Field("delivery_id"));
    }
    timestamp(time)
}
fn digest_field(value: &Text, field: &'static str) -> Result<(), DocumentError> {
    if value.as_str().len() != 64 || !value.as_str().bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(DocumentError::Field(field));
    }
    Ok(())
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
schema::wrapper!(Order, OrderFields, schema::order);
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
schema::wrapper!(OrdersPage, OrdersPageFields, schema::orders_page);
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

impl Delivery {
    /// Copies every result field into independent wire types, checking receipt ownership.
    /// `model` is the machine definition's model (ADR 0033).
    /// This represents evidence; the sender must separately enforce production eligibility.
    ///
    /// # Errors
    /// Rejects mismatched receipt identity, invalid envelope fields or oversized documents.
    pub fn from_result(
        result: &analyte_model::ResultContent,
        receipt: &analyte_model::ReceiptMetadata,
        model: &document::ModelId,
        delivery_id: analyte_model::DeliveryId,
        created_at: jiff::Timestamp,
    ) -> Result<Self, DocumentError> {
        let text = |s: String| Text::try_from(s);
        let hex = |bytes: &[u8]| bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
        let delivery = Self::try_from(DeliveryFields {
            contract_version: 1,
            delivery_id: text(uuid::Uuid::from(delivery_id).to_string())?,
            created_at: text(created_at.to_string())?,
            source: Source {
                analyzer: text(receipt.machine.as_str().to_owned())?,
                connection: text(receipt.connection.as_str().to_owned())?,
                model: model.clone(),
            },
            receipt: Receipt {
                id: text(uuid::Uuid::from(receipt.id).to_string())?,
                received_at: text(receipt.received_at.to_string())?,
                raw_sha256: text(hex(&receipt.raw_digest.0))?,
                driver: (&receipt.driver).into(),
                config_version: (&receipt.config_version).into(),
                config_sha256: text(hex(&receipt.config_digest.0))?,
            },
            results: vec![result.into()],
        })?;
        if serde_json::to_vec(&delivery)?.len() > MAX_DOCUMENT_BYTES {
            return Err(DocumentError::TooLarge);
        }
        Ok(delivery)
    }
}
