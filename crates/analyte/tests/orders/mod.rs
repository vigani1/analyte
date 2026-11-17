//! Tests orders integration behavior with synthetic peers.
#[path = "../support/capture.rs"]
pub mod capture;
#[path = "../support/mod.rs"]
mod support;
use capture::*;

#[path = "autolyser.rs"]
mod autolyser_cases;
#[path = "hl7/query.rs"]
mod hl7_order_cases;
#[path = "hl7/send.rs"]
mod hl7_send_cases;
#[path = "cases.rs"]
mod order_cases;
#[path = "timeout.rs"]
mod order_timeout;
#[path = "vidas.rs"]
mod vidas_cases;
