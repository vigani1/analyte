//! VIDAS PC Literal support, built from research notes only.
//! Evidence level: documented; no vendor manual, simulator or machine validation.
//! Hosts must journal every incoming byte before decoding, including rejected input.
#![forbid(unsafe_code)]

pub mod dialog;
pub mod framing;
pub mod syntax;
