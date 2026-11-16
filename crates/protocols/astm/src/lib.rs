//! Syntax layer: owns bounded ASTM records and typed access, without I/O or model mapping.
#![forbid(unsafe_code)]

mod syntax;
pub use syntax::*;

mod views;
pub use views::*;

mod codes;
pub use codes::*;
