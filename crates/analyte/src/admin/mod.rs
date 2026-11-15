//! Protected local administration; credentials remain owned by the host.
mod token;

pub use token::{Token, TokenError, rotate_token};
