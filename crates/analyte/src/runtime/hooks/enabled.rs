//! Applies explicit loopback overrides and bounded synthetic shutdown failures.
use super::{Arc, Destination, Duration, Error, Limits, Program, ServiceMode};
use crate::{
    deadline,
    runtime::service::{SHUTDOWN_MIN, Supervision, serve_with},
};
use analyte_store::FreeSpace;
use std::future::Future;

pub(crate) fn destination(
    hooks: &crate::installation::config::ProgramHooks,
    settings: &analyte_config::Destination,
    limits: Limits,
    mode: ServiceMode,
) -> Result<Destination, Error> {
    let loopback = mode.is_loopback();
    if !loopback && !hooks.loopback_destinations.is_empty() {
        return Err(Error::LoopbackOverridesRequireTheTestServiceEntryPoint);
    }
    if loopback {
        let base = hooks
            .loopback_destinations
            .get(settings.name.as_str())
            .map_or(settings.base_url.as_str(), String::as_str);
        Ok(Destination::for_loopback_test(
            settings.name.clone(),
            base,
            settings.name.clone(),
            limits,
        )?)
    } else {
        Ok(Destination::from_url(
            settings.name.clone(),
            &settings.base_url,
            settings.name.clone(),
            limits,
        )?)
    }
}
pub(crate) fn shutdown_bound(
    program: &Program,
    mode: ServiceMode,
    watchdog: Option<&crate::runtime::shutdown::Watchdog>,
    bound: Duration,
) -> Result<Duration, crate::Error> {
    let loopback = mode.is_loopback();
    if program.hooks.stall_store_on_shutdown {
        if !loopback || watchdog.is_some() {
            return Err(Error::StuckShutdownHookRequiresTheLoopbackTestService.into());
        }
        Ok(Duration::from_millis(500))
    } else {
        Ok(bound)
    }
}
pub(crate) async fn stall(
    program: &Program,
    store: &Arc<analyte_store::Handle>,
) -> Result<(), crate::Error> {
    if program.hooks.stall_store_on_shutdown {
        // Child-process tests emulate a stuck OS flush; the watchdog owns exit.
        store
            .request::<(), analyte_store::WorkerError>(deadline()?, |_| {
                loop {
                    std::thread::park();
                }
            })
            .await?;
    }
    Ok(())
}
/// Runs synthetic localhost tests with injected host credentials and explicit URL overrides.
/// No production configuration can enable HTTP. All spawned tasks are joined.
/// An optional readiness signal fires after startup and owner creation succeed.
/// # Errors
/// Reports startup, supervision and bounded shutdown failures.
pub async fn serve_loopback_test(
    program: Program,
    space: Box<dyn FreeSpace>,
    stop: impl Future<Output = Result<(), crate::Error>>,
    secrets: crate::installation::secrets::Environment,
    ready: Option<tokio::sync::oneshot::Sender<()>>,
) -> Result<(), crate::Error> {
    serve_with(
        program,
        space,
        stop,
        secrets,
        Supervision {
            mode: ServiceMode::LoopbackTest,
            shutdown_bound: SHUTDOWN_MIN,
            watchdog: None,
            ready,
        },
    )
    .await
}
