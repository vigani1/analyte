//! Bounded, lossless HL7 v2 syntax. Evidence level: documented.
#![forbid(unsafe_code)]

mod ack;
mod datatypes;
mod syntax;
mod views;
pub use ack::*;
pub use syntax::*;
pub use views::*;

/// Pure IHE LAW query views; see module documentation for current scope.
pub mod law;

/// Legacy deferred display query views.
pub mod query;

/// Pure legacy laboratory order builders and correlated responses.
pub mod order;
