//! Evaluates bounded nonclinical alert policies in a cancellable worker.
use crate::operator::Error;
use crate::{
    deadline,
    installation::config::{Alert, Condition},
};
use analyte_model::Identifier;
use analyte_store::{AlertKey, AlertState, Handle, Undeliverable};
use std::{sync::Arc, time::Duration};
use tokio::sync::watch;

/// The lab app destinations this run can deliver to, fixed at startup like its
/// delivery workers: a configuration or key change applies after a restart.
#[derive(Debug, Clone, Default)]
pub(crate) struct Reach {
    /// Every destination a set-up machine uses or a kept destination defines.
    pub(crate) configured: Vec<Identifier>,
    /// Kept destinations among them whose keys could not be loaded.
    pub(crate) keyless: Vec<Identifier>,
}

impl Reach {
    fn problem(&self, destination: &Identifier) -> Option<Undeliverable> {
        undeliverable_reason(
            self.configured.contains(destination),
            self.keyless.contains(destination),
        )
    }
}

/// Why this run cannot deliver a destination's stored items, or none when it
/// can. Missing keys come first, then a destination that is not set up. The
/// `undeliverable_destination` alert and the admin pages both decide it here.
pub(crate) const fn undeliverable_reason(
    set_up: bool,
    keys_missing: bool,
) -> Option<Undeliverable> {
    if keys_missing {
        Some(Undeliverable::CredentialsMissing)
    } else if set_up {
        None
    } else {
        Some(Undeliverable::NotConfigured)
    }
}

/// The single condition a policy watches; none for a policy with one condition
/// per lab app destination.
fn identity(policy: &Alert) -> Option<AlertKey> {
    Some(match policy.condition() {
        Condition::OldestUndelivered(_) => AlertKey::OldestUndelivered,
        Condition::MachineSilent(machine, _) => AlertKey::MachineSilent(machine.clone()),
        Condition::DiskReserve { .. } => AlertKey::DiskReserve,
        Condition::OrderCacheStale(_) => AlertKey::OrderCacheStale,
        Condition::UncertainOrders(_) => AlertKey::UncertainOrders,
        Condition::ParkedItems(_) => AlertKey::ParkedItems,
        Condition::RestorePaused => AlertKey::RestorePaused,
        Condition::UndeliverableDestination(_) => return None,
    })
}

fn raised(policy: &Alert, value: Option<u64>) -> bool {
    let Some(value) = value else {
        return true;
    };
    match policy.condition() {
        Condition::RestorePaused => value != 0,
        Condition::DiskReserve { below, .. } => value < *below,
        Condition::UncertainOrders(threshold)
        | Condition::ParkedItems(threshold)
        | Condition::UndeliverableDestination(threshold) => value > *threshold,
        Condition::OldestUndelivered(threshold)
        | Condition::MachineSilent(_, threshold)
        | Condition::OrderCacheStale(threshold) => value >= *threshold,
    }
}

pub(crate) fn signal(key: &AlertKey) -> (&'static str, Option<&str>) {
    match key {
        AlertKey::OldestUndelivered => ("oldest_undelivered", None),
        AlertKey::MachineSilent(machine) => ("machine_silent", Some(machine.as_str())),
        AlertKey::DiskReserve => ("disk_reserve", None),
        AlertKey::OrderCacheStale => ("order_cache_stale", None),
        AlertKey::UncertainOrders => ("uncertain_orders", None),
        AlertKey::ParkedItems => ("parked_items", None),
        AlertKey::RestorePaused => ("restore_paused", None),
        AlertKey::UndeliverableDestination(..) => ("undeliverable_destination", None),
    }
}

/// The lab app destination a condition is about, if any.
fn destination(key: &AlertKey) -> Option<&str> {
    match key {
        AlertKey::UndeliverableDestination(destination, _) => Some(destination.as_str()),
        AlertKey::OldestUndelivered
        | AlertKey::MachineSilent(_)
        | AlertKey::DiskReserve
        | AlertKey::OrderCacheStale
        | AlertKey::UncertainOrders
        | AlertKey::ParkedItems
        | AlertKey::RestorePaused => None,
    }
}

/// The plain reason and fix for a condition whose kind alone does not say it.
/// Names and counts only, never payloads or sample IDs (safety rule 10).
pub(crate) fn message(state: &AlertState) -> Option<String> {
    let AlertKey::UndeliverableDestination(destination, reason) = &state.key else {
        return None;
    };
    let destination = destination.as_str();
    let waiting = match state.value {
        Some(1) => "1 result or order message waits".to_owned(),
        Some(count) => format!("{count} results or order messages wait"),
        None => "an unknown number of results or order messages wait".to_owned(),
    };
    Some(match reason {
        Undeliverable::NotConfigured => format!(
            concat!(
                "Cannot deliver: no machine and no lab app uses destination ",
                "{destination}; {waiting}. Add lab app {destination} on the Lab apps page ",
                "and restart, or restore its settings; the results are kept and will be sent ",
                "once it is configured."
            ),
            destination = destination,
            waiting = waiting
        ),
        Undeliverable::CredentialsMissing => format!(
            concat!(
                "Cannot deliver: the API key or signing key for lab app destination ",
                "{destination} is missing; {waiting}, paused. Enter the API key and signing ",
                "key for lab app {destination} on the Lab apps page or in the service ",
                "environment, then restart; the results are kept and will be sent once the ",
                "keys are set."
            ),
            destination = destination,
            waiting = waiting
        ),
    })
}

fn log_transition(state: &AlertState) {
    // Rule 10: responder text, evidence, sample IDs and payloads never reach this event.
    let (kind, machine) = signal(&state.key);
    tracing::info!(
        alert_kind = kind,
        machine,
        destination = destination(&state.key),
        active = state.active,
        value = state.value,
        "alert transition"
    );
}

/// Shares the same bounded read-only snapshot between CLI and admin.
/// Retained active policies remain visible even after configuration removal.
pub(crate) async fn active(store: &Handle) -> Result<Vec<AlertState>, Error> {
    let until = deadline()?;
    let mut cursor = String::new();
    let mut alerts = Vec::new();
    loop {
        let page = store
            .request(until, move |store| store.active_alerts(&cursor))
            .await?;
        let Some(last) = page.last() else {
            return Ok(alerts);
        };
        cursor = serde_json::to_string(&last.key)?;
        if alerts.len().saturating_add(page.len()) > 4096 {
            return Err(Error::AlertSnapshotTooLarge);
        }
        alerts.extend(page);
    }
}

/// A clock that precedes the raise is unknown, not a guessed zero age.
pub(crate) fn age(state: &AlertState, now: i64) -> String {
    state
        .raised_at
        .and_then(|time| now.checked_sub(time))
        .filter(|age| *age >= 0)
        .map_or_else(|| "unknown".into(), |age| age.to_string())
}

/// Reads have independent deadlines. Cancellation may leave an already-submitted
/// evaluation committed; durable active-state comparison prevents a duplicate raise.
async fn evaluate(
    store: &Handle,
    policies: &[Alert],
    destinations: &[(Identifier, u64)],
    reach: &Reach,
    now: i64,
) -> Result<(), Error> {
    for policy in policies {
        match identity(policy) {
            Some(key) => record(store, policy, key, true, destinations, now).await?,
            None => undeliverable(store, policy, reach, now).await?,
        }
    }
    Ok(())
}

/// Measures one condition and stores the result; `applies` false clears it
/// whatever the measurement.
async fn record(
    store: &Handle,
    policy: &Alert,
    key: AlertKey,
    applies: bool,
    destinations: &[(Identifier, u64)],
    now: i64,
) -> Result<(), Error> {
    let read_key = key.clone();
    let destinations = destinations.to_vec();
    let value = store
        .request(deadline()?, move |store| {
            store.alert_value(&read_key, now, &destinations)
        })
        .await?;
    let active = applies && raised(policy, value);
    let responder = policy.responder().to_owned();
    let transition = store
        .request(deadline()?, move |store| {
            store.evaluate_alert(&key, &responder, active, value, now)
        })
        .await?;
    if let Some(transition) = transition {
        log_transition(&transition);
    }
    Ok(())
}

/// One condition per destination and reason (ADR 0003 rule 2: such items stay
/// stored, so someone must be told). Raises for each destination that holds
/// items this run cannot deliver to; clears each retained raise whose items were
/// delivered, or whose destination is now configured or has another reason.
async fn undeliverable(
    store: &Handle,
    policy: &Alert,
    reach: &Reach,
    now: i64,
) -> Result<(), Error> {
    let open = store
        .request(deadline()?, |store| store.undelivered_by_destination())
        .await?;
    let mut conditions: Vec<(Identifier, Undeliverable)> = open
        .into_iter()
        .filter_map(|open| {
            reach
                .problem(&open.destination)
                .map(|reason| (open.destination, reason))
        })
        .collect();
    for state in active(store).await? {
        if let AlertKey::UndeliverableDestination(destination, reason) = state.key
            && !conditions.contains(&(destination.clone(), reason))
        {
            conditions.push((destination, reason));
        }
    }
    for (destination, reason) in conditions {
        let applies = reach.problem(&destination) == Some(reason);
        let key = AlertKey::UndeliverableDestination(destination, reason);
        record(store, policy, key, applies, &[], now).await?;
    }
    Ok(())
}

pub(crate) async fn run(
    store: Arc<Handle>,
    policies: Vec<Alert>,
    destinations: Vec<(Identifier, u64)>,
    reach: Reach,
    interval: Duration,
    mut stop: watch::Receiver<bool>,
) -> Result<(), Error> {
    let mut ticks = tokio::time::interval(interval);
    ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        if !crate::runtime::until_stop(&mut ticks, &mut stop).await {
            return Ok(());
        }
        tokio::select! {
            biased;
            _ = async { let _ = stop.wait_for(|&stopped| stopped).await; } => return Ok(()),
            result = evaluate(&store, &policies, &destinations, &reach, jiff::Timestamp::now().as_second()) => result?,
        }
    }
}

#[cfg(all(test, feature = "test-hooks"))]
mod tests {
    use super::*;
    use crate::Error;
    use std::sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    };

    use analyte_fixtures::AdjustableSpace;
    fn limits() -> analyte_store::Limits {
        {
            let mut limits = analyte_fixtures::store_limits();
            limits.record_bytes = 2_097_152;
            limits.busy_timeout = Duration::from_millis(20);
            limits
        }
    }
    fn policy(kind: &str) -> Result<Alert, Error> {
        let threshold = match kind {
            "restore_paused" => "",
            "uncertain_orders" | "parked_items" | "undeliverable_destination" => "threshold=0",
            _ => "threshold=10",
        };
        let machine = if kind == "machine_silent" {
            "machine='busy'"
        } else {
            ""
        };
        Alert::for_test(&format!(
            "kind='{kind}'\n{threshold}\n{machine}\nresponder='SAMPLE-PRIVATE-00012'"
        ))
    }

    // Plan section 11: real scalar reads raise/clear all kinds at controlled Unix times.
    // Plan §11
    #[tokio::test]
    async fn each_alert_kind_raises_and_clears_once() -> Result<(), Error> {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("alerts.db");
        let space = Arc::new(AtomicU64::new(1034));
        let mut store = Handle::start(
            path.clone(),
            limits(),
            Box::new(AdjustableSpace(space.clone())),
            8,
            crate::test_startup_deadline()?,
        )
        .await?;
        let db = rusqlite::Connection::open(&path)?;
        db.execute_batch(r#"INSERT INTO machine_activity VALUES('busy',90);
            INSERT INTO feed(destination,cursor,complete,trusted,polled) VALUES('lab','c',1,1,90);
            INSERT INTO outbox(id,kind,order_id,destination,ordering_key,payload,digest,contract,state,next_attempt,enqueued_at)
            VALUES('delivery','order_event','SAMPLE-PRIVATE-00012','lab','SAMPLE-PRIVATE-00012',x'00',zeroblob(32),1,'pending',0,90);
            INSERT INTO parked(evidence) VALUES('SAMPLE-PRIVATE-00012');
            INSERT INTO machine_orders(destination,order_id,machine,version,state,created_at,updated_at,source_code)
            VALUES('lab','SAMPLE-PRIVATE-00012','busy','1','Uncertain',90,90,'test');
            INSERT INTO audit(evidence) VALUES('{"action":"store-restore"}');"#)?;
        let policies = [
            "oldest_undelivered",
            "machine_silent",
            "disk_reserve",
            "order_cache_stale",
            "uncertain_orders",
            "parked_items",
            "restore_paused",
        ]
        .map(policy)
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;
        let destinations = [(Identifier::new("lab")?, 30)];
        // Disk is clear at reserve+margin; ages raise exactly at their threshold.
        assert!(!raised(&policies[2], Some(1034)));
        space.store(1033, Ordering::SeqCst);
        evaluate(&store, &policies, &destinations, &Reach::default(), 100).await?;
        let history = store.request(deadline()?, |s| s.alert_history(0)).await?;
        assert_eq!(history.len(), 7);
        assert!(
            history
                .iter()
                .all(|h| h.state.active && h.state.raised_at == Some(100))
        );
        drop(db);
        store.shutdown(deadline()?).await?;
        let mut store = Handle::start(
            path.clone(),
            limits(),
            Box::new(AdjustableSpace(space.clone())),
            8,
            crate::test_startup_deadline()?,
        )
        .await?;
        evaluate(&store, &policies, &destinations, &Reach::default(), 101).await?;
        assert_eq!(
            store
                .request(deadline()?, |s| s.alert_history(0))
                .await?
                .len(),
            7
        );
        let db = rusqlite::Connection::open(&path)?;
        db.execute_batch(
            r#"UPDATE machine_activity SET time=101;
            UPDATE feed SET polled=101;
            UPDATE outbox SET state='done';
            UPDATE parked SET resolution='resolved';
            UPDATE machine_orders SET state='Done';
            INSERT INTO audit(evidence) VALUES('{"action":"orders-resume-after-restore"}');"#,
        )?;
        space.store(1034, Ordering::SeqCst);
        evaluate(&store, &policies, &destinations, &Reach::default(), 101).await?;
        evaluate(&store, &policies, &destinations, &Reach::default(), 102).await?;
        let history = store.request(deadline()?, |s| s.alert_history(0)).await?;
        assert_eq!(history.len(), 14);
        assert!(
            history
                .iter()
                .skip(7)
                .all(|h| !h.state.active && h.state.cleared_at == Some(101))
        );
        for item in history {
            assert!(!format!("{:?}", item.state.value).contains("SAMPLE-PRIVATE-00012"));
        }
        drop(db);
        store.shutdown(deadline()?).await?;
        Ok(())
    }

    async fn undeliverable_states(store: &Handle) -> Result<Vec<AlertState>, Error> {
        Ok(active(store)
            .await?
            .into_iter()
            .filter(|state| destination(&state.key).is_some())
            .collect())
    }

    // ADR 0014; ADR 0003 rule 2: results no destination can take stay stored and
    // raise an alert naming the destination, the count, the reason and the fix.
    #[tokio::test]
    async fn undeliverable_destination_names_reason_and_fix_then_clears() -> Result<(), Error> {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("undeliverable.db");
        let store = open_store(path.clone()).await?;
        let db = rusqlite::Connection::open(&path)?;
        db.execute_batch(concat!(
            "INSERT INTO outbox(id,kind,order_id,destination,ordering_key,payload,digest,",
            "contract,state,next_attempt,enqueued_at) VALUES",
            "('a','order_event','SAMPLE-PRIVATE-00012','old-app','SAMPLE-PRIVATE-00012',",
            "CAST('SAMPLE-PRIVATE-00012' AS BLOB),zeroblob(32),1,'pending',0,90),",
            "('b','order_event','SAMPLE-PRIVATE-00013','old-app','SAMPLE-PRIVATE-00013',",
            "CAST('SAMPLE-PRIVATE-00013' AS BLOB),zeroblob(32),1,'paused',0,90),",
            "('c','order_event','SAMPLE-PRIVATE-00014','lab','SAMPLE-PRIVATE-00014',",
            "CAST('SAMPLE-PRIVATE-00014' AS BLOB),zeroblob(32),1,'pending',0,90);"
        ))?;
        let policies = [policy("undeliverable_destination")?];
        let old = Identifier::new("old-app")?;
        let lab = Identifier::new("lab")?;
        let configured = |names: &[&Identifier], keyless: &[&Identifier]| Reach {
            configured: names.iter().map(|&name| name.clone()).collect(),
            keyless: keyless.iter().map(|&name| name.clone()).collect(),
        };

        evaluate(&store, &policies, &[], &configured(&[&lab], &[]), 100).await?;
        let raised = undeliverable_states(&store).await?;
        assert_eq!(raised.len(), 1);
        assert_eq!(
            raised[0].key,
            AlertKey::UndeliverableDestination(old.clone(), Undeliverable::NotConfigured)
        );
        assert_eq!(raised[0].value, Some(2));
        assert_eq!(signal(&raised[0].key).0, "undeliverable_destination");
        let text = message(&raised[0]).ok_or("no reason and fix")?;
        assert!(text.contains(
            "no machine and no lab app uses destination old-app; 2 results or order messages wait"
        ));
        assert!(text.contains(concat!(
            "Add lab app old-app on the Lab apps page and restart, or restore its settings; ",
            "the results are kept and will be sent once it is configured."
        )));
        assert!(!text.contains("SAMPLE-PRIVATE"));

        // Configured again (kept destination with keys): the alert clears.
        evaluate(&store, &policies, &[], &configured(&[&lab, &old], &[]), 101).await?;
        assert!(undeliverable_states(&store).await?.is_empty());

        // Configured, but its keys are missing: the second reason with its own fix.
        let keyless = configured(&[&lab, &old], &[&old]);
        evaluate(&store, &policies, &[], &keyless, 102).await?;
        let raised = undeliverable_states(&store).await?;
        assert_eq!(raised.len(), 1);
        assert_eq!(
            raised[0].key,
            AlertKey::UndeliverableDestination(old.clone(), Undeliverable::CredentialsMissing)
        );
        let text = message(&raised[0]).ok_or("no reason and fix")?;
        assert!(
            text.contains("the API key or signing key for lab app destination old-app is missing")
        );
        assert!(text.contains(
            "Enter the API key and signing key for lab app old-app on the Lab apps page"
        ));

        // Delivered: the alert clears even though the keys are still missing.
        db.execute_batch("UPDATE outbox SET state='done' WHERE destination='old-app';")?;
        evaluate(&store, &policies, &[], &keyless, 103).await?;
        assert!(undeliverable_states(&store).await?.is_empty());
        let history = store.request(deadline()?, |s| s.alert_history(0)).await?;
        let transitions: Vec<_> = history
            .iter()
            .map(|h| (h.state.active, h.state.evaluated_at))
            .collect();
        assert_eq!(
            transitions,
            [(true, 100), (false, 101), (true, 102), (false, 103)]
        );
        drop(db);
        let mut store = Arc::try_unwrap(store).map_err(|_| "evaluator retained store")?;
        store.shutdown(deadline()?).await?;
        Ok(())
    }

    use crate::runtime::log_capture::Log;

    // Rule 10 and plan section 11: only scalar values and machine IDs reach transition logs.
    // Safety rule 10 (ADR 0003)
    #[test]
    fn alert_logs_exclude_responder_and_sample_ids() -> Result<(), Error> {
        crate::runtime::log_capture::init();
        let log = Log(Arc::new(Mutex::new(Vec::new())));
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .without_time()
            .with_writer(log.clone())
            .finish();
        tracing::subscriber::with_default(subscriber, || {
            for active in [true, false] {
                log_transition(&AlertState {
                    key: AlertKey::MachineSilent(Identifier::new("busy").expect("test machine")),
                    responder: "SAMPLE-PRIVATE-00012".into(),
                    active,
                    raised_at: Some(10),
                    cleared_at: (!active).then_some(20),
                    value: Some(10),
                    evaluated_at: 20,
                });
            }
        });
        let bytes = log
            .0
            .lock()
            .map_err(|_| std::io::Error::other("test log lock poisoned"))?
            .clone();
        let output = String::from_utf8(bytes)?;
        assert_eq!(output.lines().count(), 2);
        assert!(output.contains("machine_silent") && output.contains("busy"));
        assert!(output.contains("active=true") && output.contains("active=false"));
        assert!(!output.contains("SAMPLE-PRIVATE-00012"));
        Ok(())
    }

    async fn open_store(path: std::path::PathBuf) -> Result<Arc<Handle>, Error> {
        Ok(Arc::new(
            Handle::start(
                path,
                limits(),
                Box::new(AdjustableSpace(Arc::new(AtomicU64::new(u64::MAX)))),
                8,
                crate::test_startup_deadline()?,
            )
            .await?,
        ))
    }

    // Plan section 11 and rule 7: cancellation of a queued read releases the evaluator,
    // even while the separately owned storage thread is occupied.
    // Plan §11
    #[tokio::test]
    async fn evaluator_cancels_store_read_within_shutdown_bound() -> Result<(), Error> {
        let temp = tempfile::tempdir()?;
        let store = open_store(temp.path().join("cancel.db")).await?;
        let (entered, waiting) = tokio::sync::oneshot::channel();
        let (release, released) = std::sync::mpsc::channel();
        let owner = store.clone();
        let until = deadline()?;
        let blocking = tokio::spawn(async move {
            owner
                .request::<_, Error>(until, move |_| {
                    let _ = entered.send(());
                    released.recv_timeout(Duration::from_secs(2))?;
                    Ok(())
                })
                .await
        });
        waiting.await?;
        let (stop, stopped) = watch::channel(false);
        let task = tokio::spawn(run(
            store.clone(),
            vec![policy("restore_paused")?],
            vec![],
            Reach::default(),
            Duration::from_secs(3600),
            stopped,
        ));
        tokio::task::yield_now().await;
        stop.send_replace(true);
        tokio::time::timeout(Duration::from_millis(250), task).await???;
        release.send(())?;
        blocking.await??;
        let mut store = Arc::try_unwrap(store).map_err(|_| "evaluator retained store")?;
        store.shutdown(deadline()?).await?;
        Ok(())
    }

    // Plan section 11: an unavailable store read has its own five-second deadline.
    // Plan §11
    #[tokio::test]
    async fn store_read_deadline_is_specific_and_keeps_state() -> Result<(), Error> {
        let temp = tempfile::tempdir()?;
        let store = open_store(temp.path().join("deadline.db")).await?;
        let (entered, waiting) = tokio::sync::oneshot::channel();
        let (release, released) = std::sync::mpsc::channel();
        let owner = store.clone();
        let blocking = tokio::spawn(async move {
            let until = std::time::Instant::now() + Duration::from_secs(8);
            owner
                .request::<_, Error>(until, move |_| {
                    let _ = entered.send(());
                    released.recv_timeout(Duration::from_secs(8))?;
                    Ok(())
                })
                .await
        });
        waiting.await?;
        let result = tokio::time::timeout(
            Duration::from_secs(6),
            evaluate(
                &store,
                &[policy("restore_paused")?],
                &[],
                &Reach::default(),
                100,
            ),
        )
        .await?;
        release.send(())?;
        blocking.await??;
        let error = result
            .err()
            .ok_or("queued read succeeded without store access")?;
        assert!(matches!(
            error,
            crate::operator::Error::Store(analyte_store::OperatorError::Worker(
                analyte_store::WorkerError::ResponseTimeout(_)
            ))
        ));
        assert!(
            store
                .request(deadline()?, |s| s.alert_history(0))
                .await?
                .is_empty()
        );
        let mut store = Arc::try_unwrap(store).map_err(|_| "read retained store")?;
        store.shutdown(deadline()?).await?;
        Ok(())
    }

    // Plan section 11: display ages use the raise, not evaluation time; rollback stays unknown.
    // Plan §11
    #[test]
    fn display_age_counts_from_raise_and_survives_rollback() {
        let mut state = AlertState {
            key: AlertKey::RestorePaused,
            responder: "officer".into(),
            active: true,
            raised_at: Some(100),
            cleared_at: None,
            value: Some(1),
            evaluated_at: 120,
        };
        assert_eq!(age(&state, 130), "30");
        assert_eq!(age(&state, 99), "unknown");
        state.raised_at = None;
        assert_eq!(age(&state, 130), "unknown");
    }

    // Plan section 11: unknown cache or machine history must not silently clear an alert.
    // Plan §11
    #[test]
    fn unknown_ages_remain_raised() -> Result<(), Error> {
        for kind in ["machine_silent", "order_cache_stale"] {
            assert!(raised(&policy(kind)?, None));
            assert!(!raised(&policy(kind)?, Some(9)));
            assert!(raised(&policy(kind)?, Some(10)));
        }
        Ok(())
    }
}
