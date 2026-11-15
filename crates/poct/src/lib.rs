//! Bounded XML syntax foundation for POCT01-A2. Evidence level: documented.
//!
//! This module does not validate DML message structures or authorize ACKs.
//! The host must retain rejected bytes in its protected store, too.
#![forbid(unsafe_code)]

mod xml;

pub use xml::{Document, Element, Error, Limits};
