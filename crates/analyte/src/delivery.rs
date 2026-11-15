//! Delivery ownership and bounded startup recovery.
use crate::{Error, config::Program, deadline, invalid};
use analyte_model::{Identifier, Reason};
use analyte_sink::{Destination, Limits};
use analyte_store::{Audit, Handle};
use std::time::Duration;

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
        let base = program
            .loopback_destinations
            .get(settings.name.as_str())
            .map_or(settings.base_url.as_str(), String::as_str);
        if !loopback && !program.loopback_destinations.is_empty() {
            return Err(invalid(
                "loopback overrides require the test service entry point",
            ));
        }
        destinations.push(if loopback {
            Destination::for_loopback_test(
                settings.name.clone(),
                base,
                settings.name.clone(),
                limits,
            )?
        } else {
            Destination::new(settings.name.clone(), base, settings.name.clone(), limits)?
        });
    }
    Ok(destinations)
}

pub(crate) async fn recover(store: &Handle) -> Result<(), Error> {
    crate::orders::sending::recover(store).await?;
    let until = deadline();
    loop {
        let count = store.request(until, |store| {
            let now = jiff::Timestamp::now().as_second();
            let audit = Audit {
                actor: Identifier::new("analyte-startup").map_err(|_| analyte_store::Error::InvalidInput)?,
                action: Identifier::new("recover-interrupted-delivery").map_err(|_| analyte_store::Error::InvalidInput)?,
                targets: Vec::new(),
                time: now,
                reason: Reason::new("uncertain (interrupted); ADR 0008 idempotent receiver permits retry with the same delivery ID and bytes").map_err(|_| analyte_store::Error::InvalidInput)?,
            };
            store.recover_in_flight(now, &audit)
        }).await?;
        if count == 0 {
            return Ok(());
        }
    }
}

pub(crate) fn limits(freshness: u64) -> Limits {
    Limits {
        request_timeout: Duration::from_secs(3),
        store_timeout: Duration::from_secs(2),
        backoff_min: Duration::from_secs(1),
        backoff_max: Duration::from_secs(60),
        freshness: Duration::from_secs(freshness),
        poll_interval: Duration::from_secs(5),
        page_size: 100,
        max_pages: 100,
    }
}
