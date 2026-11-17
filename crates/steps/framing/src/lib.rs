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

// Decoders expose one byte entry point and an explicit end-of-stream operation.
// Infallible ASTM/MLLP framing reports defects as events; other framers return errors.
