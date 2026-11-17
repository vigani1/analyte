//! Frames bounded bytes and checks checksums without I/O or message interpretation.
//! Feed each received byte to a decoder, consuming each event immediately.
//! Journal transport bytes before feeding: rejected/overlong input is reported in
//! bounded fragments, not retained indefinitely. Never log payloads or events.
#![forbid(unsafe_code)]

pub mod astm;
pub mod hl7;

/// Autolyser checksum envelopes.
pub mod autolyser;
/// VIDAS Literal packet framing.
pub mod vidas;

/// Incremental POCT XML root boundaries.
pub mod poct;

/// The byte-decoder contract used by every supported stack.
/// Feed one byte at a time and consume each event immediately, keeping allocations
/// and retained evidence bounded by the decoder's configuration. The host journals
/// bytes before calling this API: an error never authorizes dropping evidence.
/// `finish_stream` ends a connection/deadline and reports any incomplete evidence;
/// no implementation reads a clock or performs I/O.
pub trait Decode {
    /// A complete frame or bounded rejected evidence.
    type Event;
    /// A refusal that stops decoding; recoverable defects may instead be events.
    type Error: std::error::Error;
    /// Consumes exactly one byte, independent of transport read boundaries.
    /// # Errors
    /// Reports this protocol's size, sequence or stopped-decoder failures.
    fn push_byte(&mut self, byte: u8) -> Result<Option<Self::Event>, Self::Error>;
    /// Ends the stream, returning incomplete evidence or a specific failure.
    /// # Errors
    /// Reports an incomplete frame when the protocol cannot emit rejected evidence.
    fn finish_stream(&mut self) -> Result<Option<Self::Event>, Self::Error>;
}

/// LAW uses the HL7 MLLP decoder and exactly the same framing contract.
pub mod law {
    pub use crate::hl7::{Decoder, Error, Event, Limit};
}
