//! Parses POCT Basic Profile messages without framing, I/O or custody decisions.
//!
//! Pure message syntax and reviewer builders.
//! The host owns transport, clocks, protected storage and trusted identity resolution.
//! No acceptance ACK is emitted until the host reports durable custody.
//! See the crate README for the host contract and checklist-to-test mapping.
#![forbid(unsafe_code)]

/// Bounded XML grammar and its module-local errors.
pub mod syntax;

pub use syntax::{Document, Element, Error, Limits};

mod views;
pub use views::data::{Code, DataError, Null, Nullable, Quantity, Timestamp, Value};
pub use views::message::{
    AckKind, Body, Capabilities, ControlId, Header, Message, MessageError, Observation, Profile,
    Role, Service, hello_error_reference,
};

mod encode;
pub use encode::{ApplicationError, EscapeReason, Outbound, TerminationReason, encode};

pub use syntax::structure::{child, children, literal};

pub use views::data::{EncapsulatedData, Encoding};
