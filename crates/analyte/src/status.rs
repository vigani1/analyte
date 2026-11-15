//! Nonclinical operator health snapshots.
use crate::{Error, config::Program, deadline};
use analyte_store::Handle;
use std::collections::BTreeMap;

/// Reads bounded pages of metadata and destination health without reading payloads.
/// An absent last receipt is printed as unknown rather than invented.
/// # Errors
/// Reports destination construction, storage or deadline failures.
pub async fn snapshot(program: &Program, store: &Handle) -> Result<Vec<String>, Error> {
    let now = jiff::Timestamp::now().as_second();
    let mut lines = Vec::new();
    for destination in crate::delivery::destinations(program, false)? {
        let health = destination.health(store, now).await?;
        lines.push(format!("destination={} pending_count={} pending_bytes={} oldest_pending_age_seconds={:?} paused={} last_poll={:?} cache_trusted={}",
            destination.id().as_str(), health.pending_count, health.pending_bytes, health.oldest_pending_age,
            health.paused, health.last_successful_poll, health.cache_trusted));
    }
    let until = deadline();
    let mut after = String::new();
    let mut last = BTreeMap::new();
    loop {
        let page = store
            .request(until, move |store| store.receipts(&after))
            .await?;
        if page.is_empty() {
            break;
        }
        for receipt in &page {
            if program
                .machines()
                .iter()
                .any(|m| m.id() == &receipt.machine)
            {
                last.entry(receipt.machine.as_str().to_owned())
                    .and_modify(|time: &mut jiff::Timestamp| {
                        *time = (*time).max(receipt.received_at);
                    })
                    .or_insert(receipt.received_at);
            }
        }
        // The store's cursor is its JSON UUID key; reconstruct it without a payload read.
        after = format!(
            "\"{}\"",
            uuid::Uuid::from(
                page.last()
                    .ok_or_else(|| crate::invalid("receipt page lost cursor"))?
                    .id
            )
        );
    }
    for machine in program.machines() {
        let time = last
            .get(machine.id().as_str())
            .map_or_else(|| "unknown".to_owned(), ToString::to_string);
        lines.push(format!(
            "machine={} last_receipt={time}",
            machine.id().as_str()
        ));
    }
    Ok(lines)
}

/// Opens a local store, prints health, then joins its owner even on read failure.
/// # Errors
/// Reports configuration, protected store, read or output failures.
pub async fn print(program: &Program) -> Result<(), Error> {
    let mut store = crate::open_system_store(program).await?;
    let result = snapshot(program, &store).await;
    store.shutdown(deadline()).await?;
    for line in result? {
        crate::output::line(&line)?;
    }
    Ok(())
}
