//! Keeps installation hook state absent unless test-hooks is enabled.
#[cfg(feature = "test-hooks")]
mod setup;
#[cfg(feature = "test-hooks")]
#[derive(Default)]
pub(crate) struct ProgramHooks {
    pub(crate) stall_store_on_shutdown: bool,
    pub(crate) loopback_destinations: std::collections::BTreeMap<String, String>,
}
#[cfg(not(feature = "test-hooks"))]
#[derive(Default)]
pub(crate) struct ProgramHooks {}
