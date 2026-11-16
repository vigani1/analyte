//! Syntax layer: owns POCT Basic Profile messages, without framing, I/O or custody decisions.
//!
//! Pure message syntax, reviewer builders, duplicate comparison.
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

mod messages;
pub use messages::{ApplicationError, EscapeReason, Outbound, TerminationReason, encode};

mod duplicate;
pub use duplicate::{
    DuplicateError, DuplicateIndex, DuplicateStatus, ObservationEvidence, ObservationKey,
    RecordedObservation, observation_evidence,
};

pub use syntax::structure::{child, children, literal};

pub use views::data::{EncapsulatedData, Encoding};
