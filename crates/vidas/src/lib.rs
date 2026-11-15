//! Syntax layer: owns VIDAS Literal message grammar, without framing, I/O or model mapping.
//! Evidence level: documented; no vendor manual, simulator or machine validation.
//! Hosts must journal every incoming byte before decoding, including rejected input.
#![forbid(unsafe_code)]

pub mod syntax;
