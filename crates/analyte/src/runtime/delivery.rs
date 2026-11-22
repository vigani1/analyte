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
        // A record-only machine sends nothing, so it gets no worker (ADR 0034).
        let Some(settings) = &machine.snapshot().effective().site().destination else {
            continue;
        };
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

/// Lab apps no set-up machine uses, built like a machine's destination.
/// Each gets a delivery-only worker and no order poller: no machine is left to
/// take its orders, so reading them would only fill the cache.
pub(crate) fn unused_lab_apps(
    program: &Program,
    mode: super::hooks::ServiceMode,
) -> Result<Vec<Destination>, Error> {
    program
        .unused_lab_apps()
        .map(|kept| {
            super::hooks::destination(&program.hooks, kept.settings(), kept.limits.clone(), mode)
        })
        .collect()
}

/// Logs each destination that holds results the app has not accepted while no
/// machine and no lab app defines it: Analyte never guesses an address,
/// so those items stay stored and the admin page names them.
pub(crate) async fn report_unconfigured(
    store: &Handle,
    configured: &[Identifier],
) -> Result<(), Error> {
    let open = store
        .request(deadline()?, |store| store.undelivered_by_destination())
        .await?;
    for open in open {
        if !configured.contains(&open.destination) {
            tracing::error!(
                destination = open.destination.as_str(),
                count = open.waiting(),
                "results wait for a lab app destination that is no longer configured; they stay stored"
            );
        }
    }
    Ok(())
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
/// A machine's lab-app limits. A record-only machine has no lab app (ADR
/// 0034): it gets the documented defaults, which nothing reads, because no
/// delivery worker, order poller or order path ever runs for it.
pub(crate) fn limits(settings: Option<&analyte_config::Destination>) -> Limits {
    let (request, freshness) = settings.map_or(
        (
            analyte_config::Destination::DEFAULT_REQUEST_TIMEOUT_SECONDS,
            analyte_config::Destination::DEFAULT_FRESHNESS_SECONDS,
        ),
        |settings| (settings.request_timeout_seconds, settings.freshness_seconds),
    );
    Limits {
        request_timeout: Duration::from_secs(request),
        store_timeout: STORE_TIMEOUT,
        backoff_min: BACKOFF_MIN,
        backoff_max: BACKOFF_MAX,
        freshness: Duration::from_secs(freshness),
        poll_interval: POLL_INTERVAL,
        page_size: PAGE_SIZE,
        max_pages: MAX_PAGES,
    }
}
