//! Parses bounded HL7 messages and LAW grammar without I/O or model mapping.
#![forbid(unsafe_code)]

mod ack;
mod datatypes;
mod syntax;
mod version;
mod views;
mod wire;
pub use ack::*;
pub use syntax::*;
pub use version::Version;
pub use views::*;

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
