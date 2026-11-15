//! POCT01-A2 Basic Profile Observation Reviewer. Evidence level: documented.
//!
//! Pure message syntax, reviewer builders, duplicate comparison.
//! The host owns transport, clocks, protected storage and trusted identity resolution.
//! No acceptance ACK is emitted until the host reports durable custody.
//! See the crate README for the host contract and checklist-to-test mapping.
#![forbid(unsafe_code)]

/// Bounded XML grammar and its module-local errors.
pub mod syntax;

pub use syntax::{Document, Element, Error, Limits};

mod data;
mod message;
mod structure;
pub use data::{Code, DataError, Null, Nullable, Quantity, Timestamp, Value};
pub use message::{
    AckKind, Body, Capabilities, ControlId, Header, Message, MessageError, Observation, Profile,
    Role, Service, hello_error_reference,
};

mod encode;
pub use encode::{ApplicationError, EscapeReason, Outbound, TerminationReason, encode};

mod duplicate;
pub use duplicate::{
    DuplicateError, DuplicateIndex, DuplicateStatus, ObservationEvidence, ObservationKey,
    RecordedObservation, observation_evidence,
};

pub use structure::{child, children, literal};

pub use data::{EncapsulatedData, Encoding};
