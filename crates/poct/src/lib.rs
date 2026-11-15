//! Bounded XML syntax foundation for POCT01-A2. Evidence level: documented.
//!
//! This module does not validate DML message structures or authorize ACKs.
//! The host must retain rejected bytes in its protected store, too.
#![forbid(unsafe_code)]

mod xml;

pub use xml::{Document, Element, Error, Limits};

mod data;
mod message;
mod structure;
pub use data::{Code, DataError, Null, Nullable, Quantity, Timestamp, Value};
pub use message::{
    AckKind, Body, Capabilities, ControlId, Header, Message, MessageError, Observation, Profile,
    Role, Service,
};

mod build;
pub use build::{ApplicationError, EscapeReason, Outbound, TerminationReason, build};
