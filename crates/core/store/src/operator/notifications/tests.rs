//! Tests persisted alert progress and migration without network or clinical fixtures.
use crate::test_support::*;
use crate::*;

#[test]
fn adr0014_migration_restart_attempt_identity_budget_and_immutable_bytes() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("alerts.db");
    let endpoint = Identifier::new("relay")?;
    let mut store = open(&path)?;
    store.register_alert_endpoint(&endpoint)?;
    let namespace: String =
        store
            .db
            .query_row("SELECT identity FROM alert_installation", [], |r| r.get(0))?;
    store.db.execute_batch("INSERT INTO alert_events VALUES('event','relay','condition',1,'raised',100,x'7b7d'); INSERT INTO alert_progress(event,next_attempt) VALUES('event',100); INSERT INTO alert_reminders VALUES('relay','condition',1,160);")?;
    assert!(
        store
            .db
            .execute("UPDATE alert_events SET payload=x'00'", [])
            .is_err()
    );
    let first = store
        .claim_alert_attempt(&endpoint, 100, 2, 5, 10)?
        .ok_or("no first attempt")?;
    assert_eq!(first.attempts, 1);
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
    assert_eq!(first.id, second.id);
    assert_eq!(first.payload, second.payload);
    assert_eq!(second.attempts, 2);
    assert!(
        store
            .claim_alert_attempt(&endpoint, 200, 2, 5, 10)?
            .is_none()
    );
    store.fail_alert_attempt(&second.id, AlertFailure::Network)?;
    let status = store.alert_endpoint_status()?;
    assert_eq!(status[0].exhausted, 1);
    assert_eq!(status[0].failed, 1);
    assert_eq!(status[0].pending, 1);
    store.complete_alert_attempt(&second.id, 201)?;
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

#[test]
fn adr0014_v11_migration_preserves_local_alert_history_and_is_restart_safe() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("legacy.db");
    let mut store = open(&path)?;
    store.evaluate_alert(&AlertKey::ParkedItems, "Duty officer", true, Some(1), 100)?;
    store.db.execute_batch("DROP TABLE alert_reminders; DROP TABLE alert_progress; DROP TABLE alert_events; DROP TABLE alert_endpoints; DROP TRIGGER alert_installation_retained; DROP TABLE alert_installation; PRAGMA user_version=11;")?;
    drop(store);
    let store = open(&path)?;
    assert_eq!(store.alert_history(0)?.len(), 1);
    assert!(store.alert_endpoint_status()?.is_empty());
    let version: i64 = store
        .db
        .query_row("PRAGMA user_version", [], |r| r.get(0))?;
    assert_eq!(version, 12);
    drop(store);
    assert_eq!(open(&path)?.alert_history(0)?.len(), 1);
    Ok(())
}

fn decoded(event: &PendingAlert) -> Result<serde_json::Value, serde_json::Error> {
    serde_json::from_slice(&event.payload)
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
    let text = String::from_utf8(raise.payload.clone())?;
    for forbidden in [
        "PATIENT-SECRET",
        "SAMPLE-00012",
        "RAW-MESSAGE",
        "https://",
        "secret-token",
    ] {
        assert!(!text.contains(forbidden));
    }
    store.complete_alert_attempt(&raise.id, 101)?;
    assert!(!store.prepare_alert_event(&endpoint, 159, 60, 1)?);
    drop(store);
    let mut store = open(&path)?;
    assert!(store.prepare_alert_event(&endpoint, 160, 60, 1)?);
    let reminder = store
        .claim_alert_attempt(&endpoint, 160, 3, 5, 30)?
        .ok_or("reminder missing")?;
    let reminded = decoded(&reminder)?;
    assert_eq!(reminded["event"], "reminder");
    assert_eq!(reminded["condition_id"], raised["condition_id"]);
    assert_eq!(reminded["episode_id"], raised["episode_id"]);
    assert_ne!(reminder.id, raise.id);
    // Queue exhaustion must preserve the resolution transition until capacity returns.
    store.evaluate_alert(&AlertKey::ParkedItems, "Duty officer", false, Some(0), 161)?;
    assert!(!store.prepare_alert_event(&endpoint, 161, 60, 1)?);
    store.complete_alert_attempt(&reminder.id, 162)?;
    assert!(store.prepare_alert_event(&endpoint, 162, 60, 1)?);
    let resolve = store
        .claim_alert_attempt(&endpoint, 162, 3, 5, 30)?
        .ok_or("resolve missing")?;
    let resolved = decoded(&resolve)?;
    assert_eq!(resolved["event"], "resolved");
    assert_eq!(resolved["episode_id"], raised["episode_id"]);
    store.complete_alert_attempt(&resolve.id, 163)?;
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
