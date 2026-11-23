//! Owns feature-specific destination construction and service test overrides.
use crate::{installation::config::Program, runtime::Error};
use analyte_sink::{Destination, Limits};
use std::{sync::Arc, time::Duration};
#[cfg(feature = "test-hooks")]
mod enabled;
#[cfg(not(feature = "test-hooks"))]
pub(crate) use disabled::{destination, shutdown_budgets, stall};
#[cfg(feature = "test-hooks")]
pub use enabled::serve_loopback_test;
#[cfg(feature = "test-hooks")]
pub(crate) use enabled::{destination, shutdown_budgets, stall};

pub(crate) fn publish_store_probes(
    program: &Program,
    store: &Arc<analyte_store::Handle>,
) -> Result<(), Error> {
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

/// Uses verified HTTPS and normal shutdown budgets without synthetic state.
#[cfg(not(feature = "test-hooks"))]
mod disabled {
    use super::{Arc, Destination, Duration, Error, Limits, Program, ServiceMode};
    pub(crate) fn destination(
        _: &crate::installation::config::ProgramHooks,
        settings: &analyte_config::Destination,
        limits: Limits,
        _: ServiceMode,
    ) -> Result<Destination, Error> {
        Ok(Destination::from_url(
            settings.name.clone(),
            &settings.base_url,
            settings.name.clone(),
            limits,
        )?)
    }
    pub(crate) fn shutdown_budgets(
        _: &Program,
        _: ServiceMode,
        _: Option<&crate::runtime::shutdown::Watchdog>,
        bound: Duration,
    ) -> Result<(Duration, Option<Duration>), Error> {
        Ok((bound, None))
    }
    pub(crate) async fn stall(_: &Program, _: &Arc<analyte_store::Handle>) -> Result<(), Error> {
        std::future::ready(Ok(())).await
    }
}
