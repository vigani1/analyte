//! Tests HTTP delivery retries and stable delivery identities.
#[path = "../support/capture.rs"]
pub mod capture;
#[path = "../support/mod.rs"]
mod support;
use capture::*;

mod credentials;
mod retries;
