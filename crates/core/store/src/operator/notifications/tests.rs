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
