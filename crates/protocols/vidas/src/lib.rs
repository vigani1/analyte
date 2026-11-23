//! Parses VIDAS Literal messages without framing, I/O or model mapping.
//! Evidence level: documented; no vendor manual, simulator or machine validation.
//! Hosts must journal every incoming byte before decoding, including rejected input.
#![forbid(unsafe_code)]

pub mod syntax;

mod encode;

/// Maximum number of request packets in one host download session.
// Analyte Contract v1 bounds an app order at 100 tests. A VIDAS request
// carries one assay, so the batch uses the same host-side bound.
pub const MAX_DOWNLOAD_REQUESTS: usize = 100;
