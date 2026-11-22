//! Prepares every cached candidate once and records mapping refusals consistently.
use super::{Candidate, OrderError as Error};
use crate::installation::config::Machine;
use analyte_store::{CachedOrder, Handle};
use std::time::Instant;

pub(crate) enum Prepared<T, E> {
    Skipped,
    Rejected(E),
    Mapped(Candidate, T),
}

/// Decode and check eligibility before the protocol mapper; retain rejected work.
/// The protocol's own error type carries its mapping failure.
pub(crate) async fn prepare<T, E: From<Error>>(
    store: &Handle,
    machine: &Machine,
    cached: CachedOrder,
    until: Instant,
    reason: impl FnOnce(&E) -> String,
    map: impl FnOnce(&analyte_contract::Order) -> Result<T, E>,
) -> Result<Prepared<T, E>, E> {
    let order: analyte_contract::Order =
        analyte_contract::decode(cached.document.as_bytes()).map_err(Error::from)?;
    if !super::eligible(&order, machine) {
        return Ok(Prepared::Skipped);
    }
    let candidate = super::reservation::pending(machine, cached.id, order.fields().version)?;
    match map(&order) {
        Ok(mapped) => Ok(Prepared::Mapped(candidate, mapped)),
        Err(error) => {
            reject(store, machine, until, candidate, &reason(&error)).await?;
            Ok(Prepared::Rejected(error))
        }
    }
}

/// A superseding update wins over a stale rejection in every protocol.
pub(crate) async fn reject(
    store: &Handle,
    machine: &Machine,
    until: Instant,
    candidate: Candidate,
    reason: &str,
) -> Result<(), Error> {
    let result = super::reservation::reject(
        store,
        &machine.model,
        until,
        candidate.identity,
        candidate.version,
        machine.destination_limits.freshness.as_secs(),
        analyte_model::SourceText::new(reason)?,
    )
    .await?;
    if let Err(refusal) = result {
        tracing::info!(reason=%refusal, "mapping rejection superseded before recording");
    }
    Ok(())
}
