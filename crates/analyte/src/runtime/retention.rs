//! Runs supervised, cancellable retention batches without machine I/O or clinical logging.
use crate::installation::config::Program;
use crate::runtime::Error;
use analyte_model::Identifier;
use analyte_store::Handle;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::watch;

pub(crate) fn policies(program: &Program) -> Vec<(Identifier, Option<u16>)> {
    program
        .machines()
        .iter()
        .map(|machine| {
            let days = match machine.snapshot().effective().site().retention {
                analyte_config::Retention::Keep => None,
                analyte_config::Retention::Days(days) => Some(days.get()),
            };
            (machine.id().clone(), days)
        })
        .collect()
}
pub(crate) async fn run(
    store: Arc<Handle>,
    policies: Vec<(Identifier, Option<u16>)>,
    mut stop: watch::Receiver<bool>,
) -> Result<(), Error> {
    let mut cursors = vec![(None, None); policies.len()];
    loop {
        if *stop.borrow() {
            return Ok(());
        }
        for ((machine, days), cursor) in policies.iter().zip(&mut cursors) {
            if *stop.borrow() {
                return Ok(());
            }
            let order_machine = machine.clone();
            let machine = machine.clone();
            let days = *days;
            let after = cursor.0;
            let now = jiff::Timestamp::now().as_second();
            let batch_deadline = Instant::now()
                .checked_add(Duration::from_millis(250))
                .ok_or_else(|| Error::RetentionDeadlineOverflow)?;
            let outcome = store
                .request(crate::deadline()?, move |store| {
                    store.purge_batch(&machine, days, after, now, batch_deadline)
                })
                .await;
            match outcome {
                Ok(run) => {
                    cursor.0 = run.after;
                }
                Err(analyte_store::Error::Timeout) => {
                    tracing::warn!(
                        error_kind = "retention_deadline",
                        "retention batch rolled back; retrying on next interval"
                    );
                }
                Err(error) => return Err(error.into()),
            }
            if days.is_some() {
                let order_after = cursor.1;
                let run = store
                    .request(crate::deadline()?, move |store| {
                        let until = Instant::now()
                            .checked_add(Duration::from_millis(250))
                            .ok_or(analyte_store::Error::InvalidHostInput)?;
                        store.purge_order_batch(&order_machine, days, order_after, now, until)
                    })
                    .await;
                match run {
                    Ok(run) => cursor.1 = run.order_after,
                    Err(analyte_store::Error::Timeout) => {}
                    Err(error) => return Err(error.into()),
                }
                store
                    .request(crate::deadline()?, |store| {
                        let until = Instant::now()
                            .checked_add(Duration::from_millis(250))
                            .ok_or(analyte_store::Error::InvalidHostInput)?;
                        match store.reclaim_retention_space(until) {
                            Ok(())
                            | Err(
                                analyte_store::Error::MaintenanceBusy
                                | analyte_store::Error::Timeout,
                            ) => Ok(()),
                            Err(error) => Err(error),
                        }
                    })
                    .await?;
            }
        }
        tokio::select! {
            _ = crate::machine::stopping(&mut stop) => return Ok(()),
            () = tokio::time::sleep(Duration::from_secs(60)) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;
    struct Space;
    impl analyte_store::FreeSpace for Space {
        fn available(&self, _: &std::path::Path) -> std::io::Result<u64> {
            Ok(u64::MAX)
        }
    }
    #[tokio::test]
    async fn adr0013_item4_background_run_is_audited_and_shutdown_is_bounded() -> Result<(), Error>
    {
        let temp = tempfile::tempdir()?;
        let store = Arc::new(
            Handle::start(
                temp.path().join("store.db"),
                analyte_store::Limits {
                    reserve_bytes: 1024,
                    database_bytes: 100_000_000,
                    record_bytes: 65536,
                    page_rows: 100,
                    busy_timeout: Duration::from_millis(20),
                },
                Box::new(Space),
                2,
                crate::test_startup_deadline()?,
            )
            .await?,
        );
        store
            .request(crate::deadline()?, |store| {
                store.configure_retention_key(Some(analyte_store::DigestKey::new([1; 32])), true)
            })
            .await?;
        let (stop, stopped) = watch::channel(false);
        let owner = tokio::spawn(run(
            store.clone(),
            vec![(Identifier::new("synthetic")?, Some(90))],
            stopped,
        ));
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if store
                    .request(crate::deadline()?, |store| store.last_purge_run())
                    .await?
                    .is_some()
                {
                    break;
                }
                tokio::task::yield_now().await;
            }
            Ok::<_, Error>(())
        })
        .await??;
        stop.send_replace(true);
        tokio::time::timeout(Duration::from_secs(5), owner).await???;
        let mut store = Arc::try_unwrap(store)
            .map_err(|_| crate::invalid("retention retained store ownership"))?;
        store.shutdown(crate::deadline()?).await?;
        Ok(())
    }
}
