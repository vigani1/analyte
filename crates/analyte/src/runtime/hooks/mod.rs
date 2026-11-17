//! Owns feature-specific destination construction and service test overrides.
use crate::{installation::config::Program, runtime::Error};
use analyte_sink::{Destination, Limits};
use std::{sync::Arc, time::Duration};
#[cfg(feature = "test-hooks")]
mod enabled;
#[cfg(feature = "test-hooks")]
pub use enabled::serve_loopback_test;
#[cfg(feature = "test-hooks")]
pub(crate) use enabled::{destination, shutdown_bound, stall};
#[cfg(not(feature = "test-hooks"))]
mod disabled;
#[cfg(not(feature = "test-hooks"))]
pub(crate) use disabled::{destination, shutdown_bound, stall};

pub(crate) fn publish_store_probes(
    program: &Program,
    store: &Arc<analyte_store::Handle>,
) -> Result<(), crate::Error> {
    for machine in program.machines() {
        machine.hooks.publish(store)?;
    }
    Ok(())
}

#[derive(Clone, Copy)]
pub(crate) enum Operation {
    Lookup,
    Reserved,
    Cancel,
    CancelResult,
}

/// Selects production service policy or the feature-gated synthetic entry point.
#[derive(Clone, Copy)]
pub(crate) enum ServiceMode {
    Production,
    #[cfg(feature = "test-hooks")]
    LoopbackTest,
}
#[cfg(feature = "test-hooks")]
impl ServiceMode {
    pub(crate) fn is_loopback(self) -> bool {
        match self {
            Self::Production => false,
            #[cfg(feature = "test-hooks")]
            Self::LoopbackTest => true,
        }
    }
}
