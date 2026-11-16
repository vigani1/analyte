//! Independent version-1 evidence types; never serialize model types as the wire contract.
mod text;
pub use text::*;
mod value;
pub use value::*;
mod sample;
pub use sample::*;
mod result;
pub use result::*;
mod time;
pub use time::*;
mod collections;
pub use collections::*;
mod identity;
pub use identity::*;
mod lifecycle;
pub use lifecycle::*;
