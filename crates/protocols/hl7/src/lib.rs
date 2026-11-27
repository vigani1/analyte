//! Parses bounded HL7 messages and LAW grammar without I/O or model mapping.
#![forbid(unsafe_code)]

mod datatypes;
mod encode;
mod syntax;
mod version;
mod views;
pub use encode::ack::{
    AckError, OwnedAckError, Response, Responsibility, acknowledgment, rejection,
};
pub use syntax::{
    Charset, Component, ControlProfile, Delimiters, Error, EscapePolicy, Field, HeaderZone, Limits,
    Message, Options, Repetition, Segment, Terminator, TextEscape, decode_characters, decode_text,
    encode_text, inspect_text_escapes, is_null_text,
};
pub use version::Version;
pub use views::{
    AckCode, AckMode, Condition, ControlId, ErrView, Location, MessageCategory, Msa, Msh,
    ResultGrammarError, ResultMessage, WireCompatibility, location_error, syntax_location,
    validate_result_fields, wire_compatibility,
};

/// Pure IHE LAW query views; see module documentation for current scope.
pub mod law;

/// Legacy deferred display query views.
pub mod query;

/// Pure legacy laboratory order builders and correlated responses.
pub mod order;

/// Syntax permitted inside an already delimited HL7 field.
#[derive(Debug, Clone, Copy)]
pub enum FieldSyntax {
    /// Nonempty ASCII component with no encoding delimiters; explicit null is text here.
    Plain,
    /// Nonempty UTF-8 composite, excluding explicit null, fields and repetitions.
    Composite,
}

/// Checks field byte safety and a caller-owned byte bound without escaping or changing data.
/// Composite components and datatype rules still require their own protocol validation.
#[must_use]
pub fn is_safe_field(value: &[u8], maximum: usize, syntax: FieldSyntax) -> bool {
    !value.is_empty()
        && value.len() <= maximum
        && match syntax {
            FieldSyntax::Plain => {
                value.is_ascii()
                    && !value
                        .iter()
                        .any(|b| b.is_ascii_control() || b"|^~\\&".contains(b))
            }
            FieldSyntax::Composite => {
                value != b"\"\""
                    && std::str::from_utf8(value).is_ok()
                    && !value
                        .iter()
                        .any(|b| b.is_ascii_control() || matches!(b, b'|' | b'~'))
            }
        }
}

/// Application internal error from HL7 2.5.1 §2.15.5, table 0357 (ERR-3).
pub const APPLICATION_INTERNAL_ERROR: &[u8] = b"207";

/// Unsupported version id from HL7 2.5.1 §2.15.5, table 0357 (ERR-3): a
/// rejection for an MSH-12 the receiver does not accept.
pub const UNSUPPORTED_VERSION: &[u8] = b"203";
