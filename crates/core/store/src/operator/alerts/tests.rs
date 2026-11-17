//! Tests alert states and transition history.
use crate::test_support::*;
use crate::*;

#[test]
fn mutation_plan11_alerts_validate_responders_and_destination_count_at_the_boundary() -> TestResult
{
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    let identity = AlertKey::DiskReserve;
    for responder in ["x".repeat(256), "?".repeat(128)] {
        store.evaluate_alert(&identity, &responder, true, Some(1), 10)?;
        assert_eq!(
            store
                .alert_state(&identity)?
                .ok_or("missing alert")?
                .responder,
            responder
        );
    }
    let before = store.alert_state(&identity)?.ok_or("missing alert")?;
    let history = store.alert_history(0)?.len();
    for responder in [
        String::new(),
        "   ".into(),
        "x".repeat(257),
        "has\ncontrol".into(),
        "has\u{7f}control".into(),
    ] {
        assert!(matches!(
            store.evaluate_alert(&identity, &responder, false, Some(0), 20),
            Err(OperatorError::InvalidOperatorInput)
        ));
        let after = store.alert_state(&identity)?.ok_or("alert lost")?;
        assert_eq!(after.responder, before.responder);
        assert_eq!(after.active, before.active);
        assert_eq!(after.evaluated_at, before.evaluated_at);
        assert_eq!(store.alert_history(0)?.len(), history);
    }
    let destinations = vec![(Identifier::new("app")?, 1); 128];
    assert_eq!(
        store.alert_value(&identity, 1, &destinations)?,
        Some(u64::MAX)
    );
    let destinations = vec![(Identifier::new("app")?, 1); 129];
    assert!(matches!(
        store.alert_value(&identity, 1, &destinations),
        Err(OperatorError::InvalidOperatorInput)
    ));
    Ok(())
}

#[test]
fn mutation_plan11_alerts_pages_and_history_keep_raise_and_clear_times_distinct() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    for identity in [AlertKey::DiskReserve, AlertKey::RestorePaused] {
        store.evaluate_alert(&identity, "first responder", true, Some(42), 10)?;
    }
    store.limits.page_rows = 1;
    let page = store.active_alerts("")?;
    let first = page.first().ok_or("missing active page")?;
    assert_eq!(page.len(), 1);
    assert_eq!(first.raised_at, Some(10));
    assert_eq!(first.cleared_at, None);
    assert_eq!(first.value, Some(42));
    let next = store.active_alerts(&key(&first.key)?)?;
    assert_eq!(next.len(), 1);
    let second = next.first().ok_or("missing next active page")?;
    assert_ne!(second.key, first.key);
    assert!(store.active_alerts(&key(&second.key)?)?.is_empty());
    store.evaluate_alert(
        &AlertKey::DiskReserve,
        "second responder",
        false,
        Some(0),
        20,
    )?;
    store.limits.page_rows = 100;
    let history = store.alert_history(0)?;
    assert_eq!(history.len(), 3);
    for entry in &history {
        if entry.state.active {
            assert_eq!(entry.state.raised_at, Some(10));
            assert_eq!(entry.state.cleared_at, None);
            assert_eq!(entry.state.responder, "first responder");
        } else {
            assert_eq!(entry.state.raised_at, None);
            assert_eq!(entry.state.cleared_at, Some(20));
            assert_eq!(entry.state.responder, "second responder");
        }
    }
    let clear = history.last().ok_or("missing clear")?;
    assert!(store.alert_history(clear.sequence)?.is_empty());
    let active = store.active_alerts("")?;
    assert_eq!(active.len(), 1);
    assert_eq!(
        active.first().ok_or("missing active alert")?.key,
        AlertKey::RestorePaused
    );
    Ok(())
}

#[test]
fn mutation_plan11_alerts_oldest_delivery_age_includes_zero_and_keeps_future_unknown() -> TestResult
{
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    store.enqueue_order_event(
        &OrderEventDelivery {
            id: DeliveryId::try_from(uuid::Uuid::now_v7())?,
            destination: Identifier::new("app")?,
            order_id: Identifier::new("order")?,
            payload: b"event".to_vec(),
            contract_version: 1,
        },
        100,
    )?;
    assert_eq!(
        store.alert_value(&AlertKey::OldestUndelivered, 100, &[])?,
        Some(0)
    );
    assert_eq!(
        store.alert_value(&AlertKey::OldestUndelivered, 101, &[])?,
        Some(1)
    );
    assert_eq!(
        store.alert_value(&AlertKey::OldestUndelivered, 99, &[])?,
        None
    );
    Ok(())
}

mod existing {

    use super::*;
    use crate::test_support::{TestResult, open};

    // Plan section 11: a retry/restart cannot repeat a raise or clear.
    #[test]
    fn plan11_alert_restart_and_retained_transitions() -> TestResult {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("alerts.db");
        let mut store = open(&path)?;
        let keys = [
            AlertKey::OldestUndelivered,
            AlertKey::MachineSilent(Identifier::new("busy")?),
            AlertKey::DiskReserve,
            AlertKey::OrderCacheStale,
            AlertKey::UncertainOrders,
            AlertKey::ParkedItems,
            AlertKey::RestorePaused,
        ];
        for key in &keys {
            assert!(
                store
                    .evaluate_alert(key, "duty officer", false, Some(0), 10)?
                    .is_none()
            );
            assert!(
                store
                    .evaluate_alert(key, "duty officer", true, Some(u64::MAX), 20)?
                    .is_some()
            );
        }
        drop(store);
        let mut store = open(&path)?;
        for key in &keys {
            let state = store.alert_state(key)?.ok_or("missing state")?;
            assert!(state.active);
            assert_eq!(state.raised_at, Some(20));
            assert_eq!(state.value, Some(u64::MAX));
            assert!(
                store
                    .evaluate_alert(key, "replacement officer", true, None, 21)?
                    .is_none()
            );
            assert!(
                store
                    .evaluate_alert(key, "replacement officer", false, Some(0), 30)?
                    .is_some()
            );
            assert!(
                store
                    .evaluate_alert(key, "replacement officer", false, Some(0), 31)?
                    .is_none()
            );
            let state = store.alert_state(key)?.ok_or("missing clear")?;
            assert!(!state.active);
            assert_eq!(state.raised_at, Some(20));
            assert_eq!(state.cleared_at, Some(30));
            assert_eq!(state.responder, "replacement officer");
        }
        let history = store.alert_history(0)?;
        assert_eq!(history.len(), 14);
        assert_eq!(history.iter().filter(|h| h.state.active).count(), 7);
        for table in ["alert_state", "alert_history"] {
            assert_constraint(store.db.execute(&format!("DELETE FROM {table}"), []));
        }
        assert_constraint(store.db.execute("UPDATE alert_history SET time=0", []));
        Ok(())
    }

    // Plan section 11 and rule 10: numeric health never returns parked payload/sample text.
    #[test]
    fn plan11_alert_health_counts_ages_and_unknowns() -> TestResult {
        let temp = tempfile::tempdir()?;
        let store = open(&temp.path().join("metrics.db"))?;
        assert_eq!(
            store.alert_value(
                &AlertKey::OldestUndelivered,
                100,
                &[(Identifier::new("lab")?, 30)]
            )?,
            Some(0)
        );
        assert_eq!(
            store.alert_value(
                &AlertKey::OrderCacheStale,
                100,
                &[(Identifier::new("lab")?, 30)]
            )?,
            None
        );
        let machine = AlertKey::MachineSilent(Identifier::new("busy")?);
        assert_eq!(
            store.alert_value(&machine, 100, &[(Identifier::new("lab")?, 30)])?,
            None
        );
        store
            .db
            .execute("INSERT INTO machine_activity VALUES('busy',90)", [])?;
        assert_eq!(
            store.alert_value(&machine, 100, &[(Identifier::new("lab")?, 30)])?,
            Some(10)
        );
        store.db.execute(
            "INSERT INTO parked(evidence) VALUES('synthetic-sample-00012')",
            [],
        )?;
        assert_eq!(
            store.alert_value(
                &AlertKey::ParkedItems,
                100,
                &[(Identifier::new("lab")?, 30)]
            )?,
            Some(1)
        );
        store
            .db
            .execute("UPDATE parked SET resolution='resolved'", [])?;
        assert_eq!(
            store.alert_value(
                &AlertKey::ParkedItems,
                100,
                &[(Identifier::new("lab")?, 30)]
            )?,
            Some(0)
        );
        store.db.execute(
            "INSERT INTO feed(destination,cursor,complete,trusted,polled) VALUES('lab','c',1,1,90)",
            [],
        )?;
        assert_eq!(
            store.alert_value(
                &AlertKey::OrderCacheStale,
                100,
                &[(Identifier::new("lab")?, 30)]
            )?,
            Some(10)
        );
        store.db.execute("UPDATE feed SET trusted=0", [])?;
        assert_eq!(
            store.alert_value(
                &AlertKey::OrderCacheStale,
                100,
                &[(Identifier::new("lab")?, 30)]
            )?,
            None
        );
        assert_eq!(
            store.alert_value(
                &AlertKey::RestorePaused,
                100,
                &[(Identifier::new("lab")?, 30)]
            )?,
            Some(0)
        );
        assert_eq!(
            store.alert_value(
                &AlertKey::UncertainOrders,
                100,
                &[(Identifier::new("lab")?, 30)]
            )?,
            Some(0)
        );
        Ok(())
    }

    // Plan section 11: failure to retain history must roll back the state transition.
    #[test]
    fn plan11_alert_state_and_history_commit_atomically() -> TestResult {
        let temp = tempfile::tempdir()?;
        let mut store = open(&temp.path().join("atomic.db"))?;
        store.db.execute_batch("CREATE TRIGGER reject_alert BEFORE INSERT ON alert_history BEGIN SELECT RAISE(ABORT,'test history failure'); END;")?;
        {
            let refusal = store
                .evaluate_alert(&AlertKey::RestorePaused, "duty officer", true, Some(1), 10)
                .map(|_| ())
                .expect_err("input must be refused");
            assert!(
                matches!(&refusal, OperatorError::ConstraintViolation(_)),
                "unexpected refusal: {refusal:?}"
            );
        }
        assert!(store.alert_state(&AlertKey::RestorePaused)?.is_none());
        assert!(store.alert_history(0)?.is_empty());
        Ok(())
    }

    // Plan section 11: the disk alert can record its transition inside the admission reserve.
    #[test]
    fn plan11_disk_alert_does_not_require_free_admission_capacity() -> TestResult {
        let temp = tempfile::tempdir()?;
        let mut store = open(&temp.path().join("disk.db"))?;
        store.space = Box::new(Space(1));
        assert!(matches!(store.capacity(1), Err(StorageError::Capacity)));
        assert!(
            store
                .evaluate_alert(&AlertKey::DiskReserve, "duty officer", true, Some(1), 10)?
                .is_some()
        );
        Ok(())
    }

    // Plan section 11: upgrading a version-five store backfills message activity.
    #[test]
    fn plan11_schema5_activity_backfill_and_live_frames() -> TestResult {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("old.db");
        let mut db = Connection::open(&path)?;
        db.execute_batch(include_str!("../../upkeep/schema.sql"))?;
        Store::migrate_v2(&mut db)?;
        Store::migrate_v3(&mut db, 65_536)?;
        db.execute_batch(include_str!("../../upkeep/migration_v4.sql"))?;
        db.execute_batch(include_str!("../../upkeep/migration_v5.sql"))?;
        let metadata = serde_json::json!({"machine":"busy","received_at":"1970-01-01T00:01:30Z"});
        db.execute(
            "INSERT INTO receipts VALUES('synthetic',?1,x'00',x'00')",
            [metadata.to_string()],
        )?;
        drop(db);
        let mut store = open(&path)?;
        let machine = AlertKey::MachineSilent(Identifier::new("busy")?);
        assert_eq!(
            store.alert_value(&machine, 100, &[(Identifier::new("lab")?, 30)])?,
            Some(10)
        );
        let transfer = Identifier::new("transfer")?;
        let provenance = FrameProvenance {
            machine: Identifier::new("busy")?,
            driver: analyte_model::NamespacedId {
                namespace: Identifier::new("test")?,
                value: Identifier::new("v1")?,
            },
            config: vec![],
        };
        store.journal_frame(
            &transfer,
            FrameSequence::new(1, 7)?,
            b"synthetic",
            &provenance,
        )?;
        assert_eq!(
            store.alert_value(&machine, 100, &[(Identifier::new("lab")?, 30)])?,
            None
        );
        Ok(())
    }
}
