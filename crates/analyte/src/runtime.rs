//! Runtime ownership groups the program operations that share this role.
pub(crate) mod delivery;
pub mod service;
pub(crate) mod shutdown;
pub(crate) mod space;
#[cfg(feature = "test-hooks")]
pub mod test_hooks;
#[cfg(windows)]
pub mod windows_service;
