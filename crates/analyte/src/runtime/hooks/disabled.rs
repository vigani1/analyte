//! Uses verified HTTPS and normal shutdown budgets without synthetic state.
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
pub(crate) fn shutdown_bound(
    _: &Program,
    _: ServiceMode,
    _: Option<&crate::runtime::shutdown::Watchdog>,
    bound: Duration,
) -> Result<Duration, crate::Error> {
    Ok(bound)
}
pub(crate) async fn stall(_: &Program, _: &Arc<analyte_store::Handle>) -> Result<(), crate::Error> {
    std::future::ready(Ok(())).await
}
