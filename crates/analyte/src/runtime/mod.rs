//! Starts service workers and coordinates bounded shutdown.
pub(crate) mod delivery;
#[cfg(test)]
pub(crate) mod log_capture;
pub mod service;
pub(crate) mod shutdown;
pub(crate) mod space;
pub mod test_hooks;
#[cfg(windows)]
pub mod windows_service;

pub(crate) mod retention;

pub(crate) mod alert_delivery;

mod recovery;

mod error;
pub use error::Error;

pub(crate) mod ticks;

pub(crate) mod hooks;

/// Audited command storage and input failures.
pub mod command;
