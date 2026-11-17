//! Reads typed ASTM record fields.
mod header;
pub use header::Header;
mod patient;
pub use patient::Patient;
mod order;
pub use order::Order;
mod result;
pub use result::Result;
mod comment;
pub use comment::Comment;
mod query;
pub use query::Query;
mod terminator;
pub use terminator::Terminator;
mod scientific;
pub use scientific::Scientific;
mod manufacturer;
pub use manufacturer::Manufacturer;
