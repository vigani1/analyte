//! Deterministic LIS01-A2 dialogs. Evidence level: documented.
#![forbid(unsafe_code)]

pub mod astm;
pub mod hl7;

/// IHE LAW connection and work-response state machines.
pub mod law;

/// Autolyser host conversation.
pub mod autolyser;
/// POCT reviewer conversation.
pub mod poct;
/// VIDAS Literal conversation.
pub mod vidas;
