//! Bounded, lossless HL7 v2 syntax. Evidence level: documented.
#![forbid(unsafe_code)]

mod ack;
mod syntax;
mod views;
pub use ack::*;
pub use syntax::*;
pub use views::*;

/// Pure IHE LAW query views; see module documentation for current scope.
pub mod law;
