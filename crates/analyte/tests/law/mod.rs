//! Tests law integration behavior with synthetic peers.
#[path = "../support/capture.rs"]
pub mod capture;
#[path = "../support/mod.rs"]
mod support;
use capture::*;

#[path = "queries/mod.rs"]
mod law_queries;
#[path = "results/mod.rs"]
mod law_results;
