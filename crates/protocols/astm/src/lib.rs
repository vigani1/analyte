//! Parses bounded ASTM records and exposes typed fields without I/O or model mapping.
#![forbid(unsafe_code)]

mod syntax;
pub use syntax::*;

mod views;
pub use views::*;

mod codes;
pub use codes::*;
