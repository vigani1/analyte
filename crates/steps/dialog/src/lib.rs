//! Runs protocol states and deadlines without I/O, storage or clinical interpretation.
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

mod clock;
pub use clock::Clock;
/// Failures applying connection time.
pub use clock::Error as ClockError;

mod exchange;
pub use exchange::ExchangeId;

mod mllp;
pub use mllp::Write;
