//! Typed ASTM record views grouped by the records they interpret.
mod header;
pub use header::*;
mod patient;
pub use patient::*;
mod order;
pub use order::*;
mod result;
pub use result::*;
mod comment;
pub use comment::*;
mod query;
pub use query::*;
mod terminator;
pub use terminator::*;
mod scientific;
pub use scientific::*;
mod manufacturer;
pub use manufacturer::*;
