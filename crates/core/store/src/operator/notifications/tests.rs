//! Tests persisted alert progress without network or clinical fixtures.
use crate::test_support::*;
use crate::*;

#[test]
fn adr0014_restart_attempt_identity_budget_and_immutable_bytes() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("alerts.db");
    let endpoint = Identifier::new("relay")?;
    let mut store = open(&path)?;
    store.register_alert_endpoint(&endpoint)?;
    let namespace: String =
        store
            .db
            .query_row("SELECT identity FROM alert_installation", [], |r| r.get(0))?;
    store.db.execute_batch("INSERT INTO alert_events VALUES('00000000000000000000000000000001','relay','condition',1,'raised',100,x'7b7d'); INSERT INTO alert_progress(event,next_attempt) VALUES('00000000000000000000000000000001',100); INSERT INTO alert_reminders VALUES('relay','condition',1,160);")?;
    assert_constraint(
        store
            .db
            .execute("UPDATE alert_events SET payload=x'00'", []),
    );
    let first = store
        .claim_alert_attempt(&endpoint, 100, 2, 5, 10)?
        .ok_or("no first attempt")?;
    assert_eq!(first.attempts(), 1);
    assert!(
        store
            .claim_alert_attempt(&endpoint, 104, 2, 5, 10)?
            .is_none()
    );
    drop(store);
    let mut store = open(&path)?;
    store.register_alert_endpoint(&endpoint)?;
    let restored: String =
        store
            .db
            .query_row("SELECT identity FROM alert_installation", [], |r| r.get(0))?;
    assert_eq!(namespace, restored);
    let second = store
        .claim_alert_attempt(&endpoint, 105, 2, 5, 10)?
        .ok_or("retry lost")?;
    assert_eq!(first.id(), second.id());
    assert_eq!(first.payload(), second.payload());
    assert_eq!(second.attempts(), 2);
    assert!(
        store
            .claim_alert_attempt(&endpoint, 200, 2, 5, 10)?
            .is_none()
    );
    store.fail_alert_attempt(second.id(), AlertFailure::Network)?;
    let status = store.alert_endpoint_status()?;
    assert_eq!(status[0].exhausted, 1);
    assert_eq!(status[0].failed, 1);
    assert_eq!(status[0].pending, 1);
    store.complete_alert_attempt(second.id(), 201)?;
    assert_eq!(store.alert_endpoint_status()?[0].last_success, Some(201));
    assert_eq!(store.alert_endpoint_status()?[0].pending, 0);
    let due: i64 = store
        .db
        .query_row("SELECT next_reminder FROM alert_reminders", [], |r| {
            r.get(0)
        })?;
    assert_eq!(due, 160);
    Ok(())
}

fn decoded(event: &PendingAlert) -> Result<serde_json::Value, serde_json::Error> {
    serde_json::from_slice(event.payload())
}

#[test]
fn adr0014_raise_reminder_resolve_episodes_capacity_and_closed_payload() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("lifecycle.db");
    let endpoint = Identifier::new("relay")?;
    let mut store = open(&path)?;
    store.register_alert_endpoint(&endpoint)?;
    store.db.execute(
        "INSERT INTO parked(evidence) VALUES(?1)",
        ["PATIENT-SECRET SAMPLE-00012 RAW-MESSAGE https://private.invalid secret-token"],
    )?;
    store.evaluate_alert(&AlertKey::ParkedItems, "Duty officer", true, Some(1), 100)?;
    assert!(store.prepare_alert_event(&endpoint, 100, 60, 1)?);
    assert!(!store.prepare_alert_event(&endpoint, 100, 60, 1)?);
    assert!(store.alert_endpoint_status()?[0].queue_full);
    let raise = store
        .claim_alert_attempt(&endpoint, 100, 3, 5, 30)?
        .ok_or("raise missing")?;
    let raised = decoded(&raise)?;
    assert_eq!(raised["event"], "raised");
    let mut keys: Vec<_> = raised
        .as_object()
        .ok_or("not object")?
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec![
            "condition_id",
            "episode_at",
            "episode_id",
            "event",
            "event_id",
            "kind",
            "machine_id",
            "measurement",
            "observed_at",
            "responder",
            "version"
        ]
    );
    let text = String::from_utf8(raise.payload().to_vec())?;
    for forbidden in [
        "PATIENT-SECRET",
        "SAMPLE-00012",
        "RAW-MESSAGE",
        "https://",
        "secret-token",
    ] {
        assert!(!text.contains(forbidden));
    }
    store.complete_alert_attempt(raise.id(), 101)?;
    assert!(!store.prepare_alert_event(&endpoint, 159, 60, 1)?);
    drop(store);
    let mut store = open(&path)?;
    assert!(store.prepare_alert_event(&endpoint, 160, 60, 1)?);
    let reminder = store
        .claim_alert_attempt(&endpoint, 160, 3, 5, 30)?
        .ok_or("reminder missing")?;
    let reminder_body = decoded(&reminder)?;
    assert_eq!(reminder_body["event"], "reminder");
    assert_eq!(reminder_body["condition_id"], raised["condition_id"]);
    assert_eq!(reminder_body["episode_id"], raised["episode_id"]);
    assert_ne!(reminder.id(), raise.id());
    // Queue exhaustion must preserve the resolution transition until capacity returns.
    store.evaluate_alert(&AlertKey::ParkedItems, "Duty officer", false, Some(0), 161)?;
    assert!(!store.prepare_alert_event(&endpoint, 161, 60, 1)?);
    store.complete_alert_attempt(reminder.id(), 162)?;
    assert!(store.prepare_alert_event(&endpoint, 162, 60, 1)?);
    let resolve = store
        .claim_alert_attempt(&endpoint, 162, 3, 5, 30)?
        .ok_or("resolve missing")?;
    let resolved = decoded(&resolve)?;
    assert_eq!(resolved["event"], "resolved");
    assert_eq!(resolved["episode_id"], raised["episode_id"]);
    store.complete_alert_attempt(resolve.id(), 163)?;
    assert!(!store.prepare_alert_event(&endpoint, 1000, 60, 1)?);
    store.evaluate_alert(&AlertKey::ParkedItems, "New officer", true, None, 1001)?;
    assert!(store.prepare_alert_event(&endpoint, 1001, 60, 1)?);
    let reraised = decoded(
        &store
            .claim_alert_attempt(&endpoint, 1001, 3, 5, 30)?
            .ok_or("new raise")?,
    )?;
    assert_eq!(reraised["condition_id"], raised["condition_id"]);
    assert_ne!(reraised["episode_id"], raised["episode_id"]);
    assert!(reraised["measurement"]["value"].is_null());
    Ok(())
}

#[test]
fn adr0014_missed_intervals_generate_one_outstanding_reminder() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("reminders.db"))?;
    let endpoint = Identifier::new("relay")?;
    store.register_alert_endpoint(&endpoint)?;
    store.evaluate_alert(&AlertKey::DiskReserve, "IT officer", true, Some(10), 100)?;
    store.prepare_alert_event(&endpoint, 100, 60, 100)?;
    assert!(store.prepare_alert_event(&endpoint, 10000, 60, 100)?);
    assert!(!store.prepare_alert_event(&endpoint, 20000, 60, 100)?);
    assert_eq!(store.alert_endpoint_status()?[0].pending, 2);
    Ok(())
}

#[test]
fn adr0014_backoff_doubles_then_caps_and_limits_refuse_invalid_input() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("bounds.db"))?;
    let endpoint = Identifier::new("relay")?;
    store.register_alert_endpoint(&endpoint)?;
    store.evaluate_alert(&AlertKey::DiskReserve, "IT officer", true, Some(10), 100)?;
    for (reminder, capacity) in [(0, 1), (59, 1), (86401, 1), (60, 0), (60, 4097)] {
        assert!(matches!(
            store.prepare_alert_event(&endpoint, 100, reminder, capacity),
            Err(OperatorError::InvalidOperatorInput)
        ));
    }
    store.prepare_alert_event(&endpoint, 100, 60, 1)?;
    for (attempts, min, max) in [(0, 1, 1), (101, 1, 1), (1, 0, 1), (1, 2, 1), (1, 1, 86401)] {
        assert!(matches!(
            store.claim_alert_attempt(&endpoint, 100, attempts, min, max),
            Err(OperatorError::InvalidOperatorInput)
        ));
    }
    for (now, due) in [(100, 105), (105, 115), (115, 125)] {
        assert!(
            store
                .claim_alert_attempt(&endpoint, now, 5, 5, 10)?
                .is_some()
        );
        assert_eq!(store.alert_endpoint_status()?[0].next_attempt, Some(due));
        assert!(
            store
                .claim_alert_attempt(&endpoint, due - 1, 5, 5, 10)?
                .is_none()
        );
    }
    Ok(())
}

#[test]
fn adr0014_operator_retry_is_audited_and_preserves_original_event() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("retry.db"))?;
    let endpoint = Identifier::new("relay")?;
    store.register_alert_endpoint(&endpoint)?;
    store.evaluate_alert(&AlertKey::ParkedItems, "Duty officer", true, Some(1), 100)?;
    store.prepare_alert_event(&endpoint, 100, 60, 1)?;
    let first = store
        .claim_alert_attempt(&endpoint, 100, 1, 5, 10)?
        .ok_or("first attempt")?;
    store.fail_alert_attempt(first.id(), AlertFailure::Http)?;
    let audit = Audit {
        actor: Identifier::new("operator")?,
        action: Identifier::new("alert-retry")?,
        targets: vec![endpoint.clone()],
        time: 200,
        reason: analyte_model::Reason::new("Relay repaired and receipt checked")?,
    };
    store.retry_failed_alerts(&endpoint, &audit)?;
    {
        let refusal = store
            .retry_failed_alerts(&endpoint, &audit)
            .map(|_| ())
            .expect_err("input must be refused");
        assert!(
            matches!(&refusal, OperatorError::InvalidOperatorInput),
            "unexpected refusal: {refusal:?}"
        );
    }
    let retry = store
        .claim_alert_attempt(&endpoint, 200, 1, 5, 10)?
        .ok_or("retry missing")?;
    assert_eq!(first.id(), retry.id());
    assert_eq!(first.payload(), retry.payload());
    assert_eq!(store.audits(0)?.len(), 1);
    Ok(())
}

#[test]
fn adr0014_resolution_never_overtakes_failed_raise_even_in_same_second() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("ordering.db"))?;
    let endpoint = Identifier::new("relay")?;
    store.register_alert_endpoint(&endpoint)?;
    store.evaluate_alert(&AlertKey::ParkedItems, "Duty officer", true, Some(1), 100)?;
    store.prepare_alert_event(&endpoint, 100, 60, 10)?;
    let raise = store
        .claim_alert_attempt(&endpoint, 100, 3, 5, 10)?
        .ok_or("raise")?;
    store.fail_alert_attempt(raise.id(), AlertFailure::Network)?;
    store.evaluate_alert(&AlertKey::ParkedItems, "Duty officer", false, Some(0), 100)?;
    store.prepare_alert_event(&endpoint, 101, 60, 10)?;
    assert!(
        store
            .claim_alert_attempt(&endpoint, 101, 3, 5, 10)?
            .is_none()
    );
    // An unrelated condition remains deliverable while the earlier raise waits.
    store.evaluate_alert(&AlertKey::DiskReserve, "IT officer", true, Some(1), 101)?;
    store.prepare_alert_event(&endpoint, 101, 60, 10)?;
    let disk = store
        .claim_alert_attempt(&endpoint, 101, 3, 5, 10)?
        .ok_or("unrelated condition blocked")?;
    assert_eq!(decoded(&disk)?["kind"], "disk_reserve");
    store.complete_alert_attempt(disk.id(), 101)?;
    let retry = store
        .claim_alert_attempt(&endpoint, 105, 3, 5, 10)?
        .ok_or("raise retry")?;
    assert_eq!(raise.id(), retry.id());
    store.complete_alert_attempt(retry.id(), 105)?;
    let resolution = store
        .claim_alert_attempt(&endpoint, 105, 3, 5, 10)?
        .ok_or("resolution lost")?;
    assert_eq!(decoded(&resolution)?["event"], "resolved");
    Ok(())
}

#[test]
fn adr0014_stored_progress_records_setup_and_tls_failures() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("progress.db");
    drop(open(&path)?);
    let db = Connection::open(&path)?;
    db.execute_batch(
        "INSERT INTO alert_endpoints VALUES('relay',7,0,90);
        INSERT INTO alert_events VALUES('00000000000000000000000000000001','relay','condition',1,'raised',100,x'7b7d');
        INSERT INTO alert_progress VALUES('00000000000000000000000000000001',2,130,0,'network');",
    )?;
    drop(db);
    let mut store = open(&path)?;
    let endpoint = Identifier::new("relay")?;
    let status = store.alert_endpoint_status()?;
    assert_eq!(status[0].last_failure.as_deref(), Some("network"));
    assert_eq!(status[0].last_success, Some(90));
    assert_eq!(status[0].next_attempt, Some(130));
    let event = store
        .claim_alert_attempt(&endpoint, 130, 5, 5, 300)?
        .ok_or("event lost")?;
    assert_eq!(event.id().as_str(), "00000000000000000000000000000001");
    assert_eq!(event.payload(), b"{}");
    assert_eq!(event.attempts(), 3);
    for failure in [AlertFailure::Setup, AlertFailure::Tls] {
        store.fail_alert_attempt(event.id(), failure)?;
    }
    assert_eq!(
        store.alert_endpoint_status()?[0].last_failure.as_deref(),
        Some("tls_setup")
    );
    Ok(())
}
