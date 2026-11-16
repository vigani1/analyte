//! Service startup, delivery workers and shutdown own the running host lifecycle.
pub(crate) mod delivery;
pub mod service;
pub(crate) mod shutdown;
pub(crate) mod space;
#[cfg(feature = "test-hooks")]
pub mod test_hooks;
#[cfg(test)]
pub(crate) mod test_tracing;
#[cfg(windows)]
pub mod windows_service;
