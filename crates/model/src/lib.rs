//! Shared, protocol-neutral evidence, without I/O or clinical interpretation.
//!
//! A receipt identifies what arrived. A delivery identifies one immutable payload
//! sent to an app; retries keep that ID. A result identifies an observation and
//! its revision: a correction changes the revision, a rerun the observation.
//! Hosts generate `UUIDv7` IDs once and persist them; this crate validates them.
#![forbid(unsafe_code)]

mod identity;
mod lifecycle;
mod result;
mod text;
mod value;

pub use identity::*;
pub use lifecycle::*;
pub use result::*;
pub use text::*;
pub use value::*;
