//! Laboratory ORM/O01–ORR/O02 (both editions) and non-LAW OML/O21–ORL/O22 (2.5.1).
//! Evidence: documented.
//!
//! These pure functions never store, transmit, resend or change order state.
//! A returned order decision reports the peer's words, not a host lifecycle change.
use crate::{Field, Message, Segment};
use thiserror::Error;

mod laboratory;
mod response;
mod types;
mod wire;

pub use laboratory::SpecimenResponse;
pub use response::{Decision, OrderResponse, Outcome};
pub use types::*;
pub use wire::{context, oml, orm};

/// Complete encoded EI field limit for ORC-2, ORC-3 and ORC-4 in both supported editions.
/// HL7 2.3.1 section 4.3.1 and 2.5.1 section 4.5.1 attribute tables.
pub const EI_MAX_BYTES: usize = 22;

/// A located failure that never includes patient/order payloads in its display.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum Error {
    /// Underlying bounded HL7 syntax or control validation failed.
    #[error(transparent)]
    Syntax(#[from] crate::Error),
    /// A shared field-rule error retains its component/authority cause unchanged.
    #[error(transparent)]
    FieldRule(crate::law::Error),
    /// Missing, malformed or unsupported content at its owning boundary.
    #[error("invalid order content at {segment}-{field}")]
    Content {
        /// Segment or datatype name.
        segment: String,
        /// One-based field/component, zero for structure.
        field: usize,
    },
    /// A known or unknown control cannot serve the current request/response role.
    #[error("unexpected order control at ORC-1")]
    Control,
    /// Message or order identities do not match the sent request.
    #[error("order correlation failed at {segment}-{field}")]
    Correlation {
        /// Segment identifier.
        segment: &'static str,
        /// One-based field.
        field: usize,
    },
    /// The same order appears twice; no duplicate is silently folded away.
    #[error("duplicate order at ORC-{field}")]
    Duplicate {
        /// Identity field shared by two orders (placer or filler).
        field: usize,
    },
    /// Output would exceed the caller's configured ceiling.
    #[error("order message limit reached")]
    Limit,
}

fn invalid(segment: &str, field: usize) -> Error {
    Error::Content {
        segment: segment.to_owned(),
        field,
    }
}

fn raw(segment: Segment<'_>, field: usize) -> &[u8] {
    segment.field(field).map_or(b"", Field::raw)
}

/// Supported order controls from table 0119; notifications are not acknowledgments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    /// NW: request new work.
    New,
    /// CA: request cancellation; this does not confirm cancellation.
    Cancel,
    /// OK: filler accepted the new order.
    Accepted,
    /// UA: filler could not accept the new order.
    Rejected,
    /// CR: filler confirms requested cancellation.
    Cancelled,
    /// UC: filler could not cancel the order.
    UnableToCancel,
}

impl Control {
    /// Reads only the supported controls; every other code is a located error.
    /// # Errors
    /// Returns `Error::Control` at ORC-1 for unknown/unsupported controls.
    pub fn parse(value: &[u8]) -> Result<Self, Error> {
        match value {
            b"NW" => Ok(Self::New),
            b"CA" => Ok(Self::Cancel),
            b"OK" => Ok(Self::Accepted),
            b"UA" => Ok(Self::Rejected),
            b"CR" => Ok(Self::Cancelled),
            b"UC" => Ok(Self::UnableToCancel),
            _ => Err(Error::Control),
        }
    }

    /// Returns the exact standard wire code.
    #[must_use]
    pub fn wire(self) -> &'static [u8] {
        match self {
            Self::New => b"NW",
            Self::Cancel => b"CA",
            Self::Accepted => b"OK",
            Self::Rejected => b"UA",
            Self::Cancelled => b"CR",
            Self::UnableToCancel => b"UC",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Family {
    Orm,
    Oml,
}

impl Family {
    fn request(self) -> &'static [u8] {
        match self {
            Self::Orm => b"ORM^O01^ORM_O01",
            Self::Oml => b"OML^O21^OML_O21",
        }
    }

    fn response(self) -> [&'static [u8]; 3] {
        match self {
            Self::Orm => [b"ORR", b"O02", b"ORR_O02"],
            Self::Oml => [b"ORL", b"O22", b"ORL_O22"],
        }
    }
}

/// A validated ORM/O01 or OML/O21; its message identity is host supplied.
#[derive(Debug)]
pub struct OrderMessage {
    message: Message,
    family: Family,
}

impl OrderMessage {
    /// Returns the message for transport and protected retention; no send occurs here.
    #[must_use]
    pub fn message(&self) -> &Message {
        &self.message
    }
}
