//! Describes the version 1 request and answer bodies as one JSON Schema (draft 2020-12), derived from the contract types (ADR 0033).
//!
//! The schema states what a lab app can check without Analyte's code: types,
//! closed enums, required fields, list and length bounds, text patterns,
//! `decoded_text` only beside text and order tests nonempty except on a
//! tombstone. Three rules need comparing two values or counting bytes, which
//! JSON Schema cannot do, so they stay in the Rust validators and in
//! docs/contract/v1.md: byte limits (`maxLength` counts characters, so it is
//! an upper bound), each result's receipt equal to the envelope's, and a unit
//! mapping's source equal to the original unit.
use crate::{Delivery, ErrorBody, OrderEvent, OrdersPage, ReviewNotice, StatusBody};
use schemars::{JsonSchema, Schema, SchemaGenerator, generate::SchemaSettings, json_schema};
use std::borrow::Cow;

/// Text without Unicode control characters (`char::is_control`: U+0000 to
/// U+001F and U+007F to U+009F).
const NO_CONTROLS: &str = "^[^\\u0000-\\u001F\\u007F-\\u009F]*$";
/// Text without NUL.
const NO_NUL: &str = "^[^\\u0000]*$";

/// The definition names of the bodies a lab app sends or receives, in the
/// order the calls use them.
pub const BODIES: [&str; 6] = [
    "Delivery",
    "OrderEvent",
    "ReviewNotice",
    "StatusBody",
    "ErrorBody",
    "OrdersPage",
];

/// The contract v1 schema: every body in [`BODIES`] and the types they use,
/// under `$defs`. Its bytes are what `docs/contract/contract-v1.schema.json`
/// holds.
#[must_use]
pub fn bundle() -> serde_json::Value {
    let mut generator = SchemaSettings::draft2020_12().into_generator();
    generator.subschema_for::<Delivery>();
    generator.subschema_for::<OrderEvent>();
    generator.subschema_for::<ReviewNotice>();
    generator.subschema_for::<StatusBody>();
    generator.subschema_for::<ErrorBody>();
    generator.subschema_for::<OrdersPage>();
    let definitions = generator.take_definitions(true);
    serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "Analyte app contract version 1",
        "description": "Request and answer bodies of contract version 1 (docs/contract/v1.md). Generated from the Rust contract types; do not edit by hand. Unknown fields are allowed and ignored.",
        "$defs": definitions,
    })
}

/// A string bounded to `bytes` UTF-8 bytes, nonempty when `required`, and
/// matching `pattern` when given.
fn string(bytes: usize, required: bool, pattern: Option<&str>, description: &str) -> Schema {
    let mut schema = json_schema!({
        "type": "string",
        "maxLength": bytes,
        "description": format!(
            "{description} The limit is {bytes} UTF-8 bytes; maxLength counts characters, so it is an upper bound."
        ),
    });
    if required {
        schema.insert("minLength".into(), 1.into());
    }
    if let Some(pattern) = pattern {
        schema.insert("pattern".into(), pattern.into());
    }
    schema
}

/// The schema of one bounded text type of `document`.
pub(crate) fn text(bytes: usize, required: bool, controls: bool, description: &str) -> Schema {
    string(
        bytes,
        required,
        (!controls).then_some(NO_CONTROLS),
        description,
    )
}

/// Envelope text: nonempty, at most 4096 UTF-8 bytes, without NUL.
pub(crate) fn envelope_text() -> Schema {
    string(
        4096,
        true,
        Some(NO_NUL),
        "Envelope text, kept exactly; never trimmed or normalized.",
    )
}

macro_rules! marker {
    ($(#[$doc:meta])* $name:ident, $schema:expr) => {
        $(#[$doc])*
        pub(crate) struct $name;
        impl JsonSchema for $name {
            fn schema_name() -> Cow<'static, str> {
                stringify!($name).into()
            }
            fn json_schema(_: &mut SchemaGenerator) -> Schema {
                $schema
            }
        }
    };
}

marker!(
    /// `contract_version`, which is always 1.
    VersionOne,
    json_schema!({ "type": "integer", "const": 1 })
);
marker!(
    /// An RFC 9562 version 7 UUID in its hyphenated form.
    Uuid7,
    json_schema!({
        "type": "string",
        "pattern": "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$",
        "description": "An RFC 9562 UUID version 7, hyphenated.",
    })
);
marker!(
    /// A timestamp with an explicit offset.
    Timestamp,
    json_schema!({
        "type": "string",
        "format": "date-time",
        "description": "An RFC 3339 timestamp with an explicit offset.",
    })
);
marker!(
    /// A local date and time without an offset, as a source wrote it.
    LocalDateTime,
    json_schema!({
        "type": "string",
        "pattern": "^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}(\\.[0-9]{1,9})?$",
        "description": "A local date and time without an offset; no time zone is implied.",
    })
);
marker!(
    /// A SHA-256 digest as 64 hex digits.
    Sha256,
    json_schema!({
        "type": "string",
        "pattern": "^[0-9a-fA-F]{64}$",
        "description": "A SHA-256 digest as 64 hex digits.",
    })
);
marker!(
    /// An opaque order-feed cursor.
    Cursor,
    string(256, true, Some(NO_NUL), "An opaque order-feed cursor.")
);

/// Gives a validated wrapper the schema of the fields it validates, under
/// its own name, with `extend` adding its description or rules.
macro_rules! wrapper {
    ($wrapper:ident, $fields:ty) => {
        $crate::schema::wrapper!($wrapper, $fields, |_| {});
    };
    ($wrapper:ident, $fields:ty, $extend:expr) => {
        impl schemars::JsonSchema for $wrapper {
            fn schema_name() -> std::borrow::Cow<'static, str> {
                stringify!($wrapper).into()
            }
            fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
                let mut schema = <$fields as schemars::JsonSchema>::json_schema(generator);
                let extend: fn(&mut schemars::Schema) = $extend;
                extend(&mut schema);
                schema
            }
        }
    };
}
pub(crate) use wrapper;

fn describe(schema: &mut Schema, description: &str) {
    schema.insert("description".into(), description.into());
}

/// `POST /results`.
pub(crate) fn delivery(schema: &mut Schema) {
    describe(
        schema,
        "The body of POST /results: the results read from one machine message, with its receipt. Each result's receipt equals receipt.id.",
    );
}

/// `POST /order-events`.
pub(crate) fn order_event(schema: &mut Schema) {
    describe(
        schema,
        "The body of POST /order-events: one change of an order's state on a machine.",
    );
}

/// `POST /review-notices` (ADR 0037).
pub(crate) fn review_notice(schema: &mut Schema) {
    describe(
        schema,
        "The body of POST /review-notices: an item from a machine waits for review in Analyte (opened), or staff settled it there (closed). Optional for a lab app; the decision stays in Analyte. It never carries a raw message or a value.",
    );
}

/// One order: tests may be empty only on a tombstone.
pub(crate) fn order(schema: &mut Schema) {
    describe(
        schema,
        "One order version from the order feed. tests holds 1 to 100 tests, and may be empty only on a tombstone.",
    );
    schema.insert(
        "if".into(),
        serde_json::json!({ "properties": { "tombstone": { "const": false } } }),
    );
    schema.insert(
        "then".into(),
        serde_json::json!({ "properties": { "tests": { "minItems": 1 } } }),
    );
}

/// `GET /orders`.
pub(crate) fn orders_page(schema: &mut Schema) {
    describe(
        schema,
        "The answer to GET /orders: one page of the order feed.",
    );
}

/// A value: `decoded_text` only beside a text interpretation.
pub(crate) fn value(schema: &mut Schema) {
    describe(
        schema,
        "A value as sent (original, authoritative) with its interpretation. decoded_text is given only beside the text interpretation.",
    );
    schema.insert(
        "if".into(),
        serde_json::json!({
            "properties": { "decoded_text": { "type": "string" } },
            "required": ["decoded_text"],
        }),
    );
    schema.insert(
        "then".into(),
        serde_json::json!({ "properties": { "parsed": { "const": "text" } } }),
    );
}

/// Units: a mapping's source is exactly the original unit.
pub(crate) fn units(schema: &mut Schema) {
    describe(
        schema,
        "A unit as sent, with an explicit UCUM mapping when one is set up; mapping.source equals original.",
    );
}
