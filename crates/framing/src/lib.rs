//! Bounded, byte-oriented framing without I/O (documented evidence level).
//! Feed each received byte to a decoder, consuming each event immediately.
//! Journal transport bytes before feeding: rejected/overlong input is reported in
//! bounded fragments, not retained indefinitely. Never log payloads or events.
#![forbid(unsafe_code)]

pub mod astm;
pub mod mllp;

/// Autolyser checksum envelopes.
pub mod autolyser;
/// VIDAS Literal packet framing.
pub mod vidas;
