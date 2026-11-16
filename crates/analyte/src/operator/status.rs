//! Reads nonclinical operator health snapshots.
use crate::{Error, deadline, installation::config::Program};
use analyte_store::Handle;
use std::collections::BTreeMap;

/// Reads bounded pages of metadata and destination health without reading payloads.
/// An absent last receipt is printed as unknown rather than invented.
/// # Errors
/// Reports destination construction, storage or deadline failures.
pub async fn snapshot(program: &Program, store: &Handle) -> Result<Vec<String>, Error> {
    let now = jiff::Timestamp::now().as_second();
    let paused = store
        .request(deadline(), |store| store.restore_unreconciled())
        .await?;
    let mut lines = vec![format!("order_sending_paused_after_restore={paused}")];
    for endpoint in store
        .request(deadline(), |store| store.alert_endpoint_status())
        .await?
    {
        lines.push(crate::operator::notification_status::describe(
            &endpoint, now,
        ));
    }
    for alert in crate::operator::alerts::active(store).await? {
        let (kind, machine) = crate::operator::alerts::signal(&alert.key);
        lines.push(format!(
            "alert={kind} machine={machine:?} responder={:?} raised_age_seconds={} last_value={:?}",
            alert.responder,
            crate::operator::alerts::age(&alert, now),
            alert.value
        ));
    }
    for destination in crate::runtime::delivery::destinations(program, false)? {
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
        if machine.capture {
            let marker = program
                .data_directory
                .join(format!("capture-{}.failure", machine.id().as_str()));
            let capture = match std::fs::File::open(marker) {
                Ok(file) => {
                    use std::io::Read as _;
                    let mut text = String::new();
                    file.take(4096).read_to_string(&mut text)?;
                    text.trim().to_owned()
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    "enabled; no recorded failure (capture completeness unknown)".into()
                }
                Err(error) => format!("status unavailable: {}", error.kind()),
            };
            lines.push(format!(
                "machine={} capture={capture}",
                machine.id().as_str()
            ));
        }
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
        crate::operator::output::line(&line)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Space;
    impl analyte_store::FreeSpace for Space {
        fn available(&self, _: &std::path::Path) -> std::io::Result<u64> {
            Ok(u64::MAX)
        }
    }

    // Plan section 11: status shows every active page, names responders and omits cleared alerts.
    #[tokio::test]
    async fn plan11_status_active_alerts_responder_age_and_read_only() -> Result<(), Error> {
        let directory = tempfile::tempdir()?;
        let mut store = Handle::start(
            directory.path().join("alerts.db"),
            analyte_store::Limits {
                reserve_bytes: 1,
                database_bytes: 100_000_000,
                record_bytes: 2_097_152,
                page_rows: 1,
                busy_timeout: std::time::Duration::from_millis(100),
            },
            Box::new(Space),
            4,
            crate::test_startup_deadline()?,
        )
        .await?;
        store
            .request(deadline(), |store| {
                store.evaluate_alert(
                    &analyte_store::AlertKey::RestorePaused,
                    "lab duty officer",
                    true,
                    Some(1),
                    100,
                )?;
                store.evaluate_alert(
                    &analyte_store::AlertKey::MachineSilent(
                        analyte_model::Identifier::new("busy")
                            .map_err(|_| analyte_store::Error::InvalidInput)?,
                    ),
                    "analyzer operator",
                    true,
                    None,
                    100,
                )?;
                store.evaluate_alert(
                    &analyte_store::AlertKey::ParkedItems,
                    "cleared officer",
                    true,
                    Some(1),
                    100,
                )?;
                store.evaluate_alert(
                    &analyte_store::AlertKey::ParkedItems,
                    "cleared officer",
                    false,
                    Some(0),
                    101,
                )?;
                Ok(())
            })
            .await?;
        let program = Program::load(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/analyte.toml"),
        )?;
        assert!(program.alerts().is_empty());
        let lines = snapshot(&program, &store).await?;
        let alerts: Vec<_> = lines
            .iter()
            .filter(|line| line.starts_with("alert="))
            .collect();
        assert_eq!(alerts.len(), 2);
        assert!(
            alerts
                .iter()
                .any(|line| line.contains("restore_paused") && line.contains("lab duty officer"))
        );
        assert!(alerts.iter().any(|line| line.contains("machine_silent")
            && line.contains("busy")
            && line.contains("analyzer operator")));
        assert!(
            alerts
                .iter()
                .all(|line| line.contains("raised_age_seconds=")
                    && !line.contains("raised_age_seconds=unknown"))
        );
        assert!(!lines.iter().any(|line| line.contains("cleared officer")));
        let state = store
            .request(deadline(), |store| {
                store.alert_state(&analyte_store::AlertKey::RestorePaused)
            })
            .await?
            .ok_or("alert disappeared")?;
        assert_eq!(state.evaluated_at, 100);
        assert_eq!(state.raised_at, Some(100));
        store.shutdown(deadline()).await?;
        Ok(())
    }
}
