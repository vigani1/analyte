//! Parses bounded ASTM records and exposes typed fields without I/O or model mapping.
#![forbid(unsafe_code)]

mod syntax;
pub use syntax::*;

mod views;
pub use views::*;

mod codes;
pub use codes::*;

mod query;
pub use query::{QueryIntent, QuerySelectorError, query_intent};

mod encode;
pub use encode::{OrderTermination, encode_order_envelope};
