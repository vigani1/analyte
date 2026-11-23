//! Tests service integration behavior with synthetic peers.
#[path = "../support/capture.rs"]
pub mod capture;
#[path = "../support/mod.rs"]
mod support;
use capture::*;

#[path = "capture.rs"]
mod capture_cases;
#[path = "gazelle.rs"]
mod gazelle_cases;
#[path = "machines.rs"]
mod machine_definitions;
#[cfg(feature = "test-hooks")]
#[path = "recovery.rs"]
mod service_recovery;

mod deadlines;
mod record_display;
mod removed;
mod startup;
