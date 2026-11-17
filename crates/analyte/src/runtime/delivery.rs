//! Owns delivery workers and bounds startup recovery.
use crate::runtime::Error;
use crate::{deadline, installation::config::Program};
use analyte_model::{Identifier, Reason};
use analyte_sink::{Destination, Limits};
use analyte_store::{Audit, Handle};
use std::time::Duration;

/// Allows the active HTTP attempt and its store outcome to finish on shutdown.
pub(crate) const SHUTDOWN_BUDGET: Duration = Duration::from_secs(5);

pub(crate) fn destinations(program: &Program, loopback: bool) -> Result<Vec<Destination>, Error> {
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
        #[cfg(feature = "test-hooks")]
        let base = program
            .hooks
            .loopback_destinations
            .get(settings.name.as_str())
            .map_or(settings.base_url.as_str(), String::as_str);
        #[cfg(feature = "test-hooks")]
        if !loopback && !program.hooks.loopback_destinations.is_empty() {
            return Err(Error::LoopbackOverridesRequireTheTestServiceEntryPoint);
        }
        #[cfg(feature = "test-hooks")]
        destinations.push(if loopback {
            Destination::for_loopback_test(
                settings.name.clone(),
                base,
                settings.name.clone(),
                limits,
            )?
        } else {
            Destination::from_url(
                settings.name.clone(),
                &settings.base_url,
                settings.name.clone(),
                limits,
            )?
        });
        #[cfg(not(feature = "test-hooks"))]
        {
            let _ = loopback;
            destinations.push(Destination::from_url(
                settings.name.clone(),
                &settings.base_url,
                settings.name.clone(),
                limits,
            )?);
        }
    }
    Ok(destinations)
}

pub(crate) async fn recover(store: &Handle) -> Result<(), Error> {
    crate::orders::sending::recover(store).await?;
    let until = deadline()?;
    loop {
        let count = store.request(until, |store| {
            let now = jiff::Timestamp::now().as_second();
            let audit = Audit {
                actor: Identifier::new("analyte-startup").map_err(analyte_store::Error::Text)?,
                action: Identifier::new("recover-interrupted-delivery").map_err(analyte_store::Error::Text)?,
                targets: Vec::new(),
                time: now,
                reason: Reason::new("uncertain (interrupted); ADR 0008 idempotent receiver permits retry with the same delivery ID and bytes").map_err(analyte_store::Error::Text)?,
            };
            Ok(store.recover_in_flight(now, &audit)?)
        }).await?;
        if count == 0 {
            return Ok(());
        }
    }
}

// HTTP and retry defaults are host policy, independent of analyzer dialog timers.
/// Bounds the complete outbound HTTP attempt.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(3);
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

pub(crate) fn limits(freshness: u64) -> Limits {
    Limits {
        request_timeout: REQUEST_TIMEOUT,
        store_timeout: STORE_TIMEOUT,
        backoff_min: BACKOFF_MIN,
        backoff_max: BACKOFF_MAX,
        freshness: Duration::from_secs(freshness),
        poll_interval: POLL_INTERVAL,
        page_size: PAGE_SIZE,
        max_pages: MAX_PAGES,
    }
}
