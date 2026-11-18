//! Owns delivery workers and bounds startup recovery.
use crate::runtime::Error;
use crate::{deadline, installation::config::Program};
use analyte_model::{Identifier, Reason};
use analyte_sink::{Destination, Limits};
use analyte_store::{Audit, Handle};
use std::time::Duration;

/// Allows the active HTTP attempt and its store outcome to finish on shutdown.
pub(crate) const SHUTDOWN_BUDGET: Duration = Duration::from_secs(5);

pub(crate) fn destinations(
    program: &Program,
    mode: super::hooks::ServiceMode,
) -> Result<Vec<Destination>, Error> {
    let mut destinations = Vec::new();
    for machine in program.machines() {
        let settings = &machine.snapshot().effective().site().destination;
        if destinations
            .iter()
            .any(|d: &Destination| d.id() == &settings.name)
        {
            continue;
        }
        let limits = machine.destination_limits.clone();
        destinations.push(super::hooks::destination(
            &program.hooks,
            settings,
            limits,
            mode,
        )?);
    }
    Ok(destinations)
}

pub(crate) async fn recover(store: &Handle) -> Result<(), Error> {
    crate::orders::sending::recover(store).await?;
    let until = deadline()?;
    loop {
        let count = store.request::<_, analyte_store::DeliveryError>(until, |store| {
            let now = jiff::Timestamp::now().as_second();
            let audit = Audit {
                actor: Identifier::new("analyte-startup").map_err(analyte_store::StorageError::Text)?,
                action: Identifier::new("recover-interrupted-delivery").map_err(analyte_store::StorageError::Text)?,
                targets: Vec::new(),
                time: now,
                reason: Reason::new("uncertain (interrupted); ADR 0008 idempotent receiver permits retry with the same delivery ID and bytes").map_err(analyte_store::StorageError::Text)?,
                                };
            store.recover_in_flight(now, &audit)
        }).await?;
        if count == 0 {
            return Ok(());
        }
    }
}

// Retry and polling defaults are host policy, independent of analyzer dialog timers.
// The HTTP request deadline is a site setting: it depends on the link and the app.
/// Reserves time to persist an accepted or uncertain delivery outcome.
const STORE_TIMEOUT: Duration = Duration::from_secs(2);
/// Initial retry delay matches the store's one-second scheduling precision.
const BACKOFF_MIN: Duration = Duration::from_secs(1);
/// Caps outage traffic and Retry-After waits.
const BACKOFF_MAX: Duration = Duration::from_secs(60);
/// Bounds load on order feeds; freshness remains site-configured.
const POLL_INTERVAL: Duration = Duration::from_secs(5);
/// Contract v1 permits at most 100 orders per page.
const PAGE_SIZE: u16 = 100;
/// Bounds a single paged snapshot even when the peer never finishes it.
const MAX_PAGES: u16 = 100;

/// Turns validated site destination settings into the sink's operational bounds.
pub(crate) fn limits(settings: &analyte_config::Destination) -> Limits {
    Limits {
        request_timeout: Duration::from_secs(settings.request_timeout_seconds),
        store_timeout: STORE_TIMEOUT,
        backoff_min: BACKOFF_MIN,
        backoff_max: BACKOFF_MAX,
        freshness: Duration::from_secs(settings.freshness_seconds),
        poll_interval: POLL_INTERVAL,
        page_size: PAGE_SIZE,
        max_pages: MAX_PAGES,
    }
}
