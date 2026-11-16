//! Tests retention migration, key continuity and finished-data guards against ADR 0013.
use super::*;
use crate::test_support::*;
use crate::{Attempt, Audit, DeliveryId, Identifier, Reading, key};
use analyte_model::{IdentityStatus, Reason};
use rusqlite::params;
use std::time::Instant;

pub(super) fn finished(
    store: &mut Store,
) -> Result<analyte_model::ReceiptId, Box<dyn std::error::Error>> {
    let metadata = receipt(b"synthetic")?;
    store.receive(&metadata, b"synthetic", b"config")?;
    let result = production_result(metadata.id)?;
    let IdentityStatus::Resolved(identity) = &result.identity else {
        return Err("fixture".into());
    };
    let ordering = crate::OrderingKey {
        machine: metadata.machine.clone(),
        observation: identity.observation.clone(),
    };
    let mut evidence = interpretation()?;
    evidence.outcome = Reading::Results(vec![result]);
    store.db.execute(
        "INSERT INTO interpretations VALUES (?1,1,?2)",
        params![key(&metadata.id)?, key(&evidence)?],
    )?;
    let delivery = DeliveryId::try_from(uuid::Uuid::now_v7())?;
    seed_delivery(store, metadata.id, delivery, &key(&ordering)?)?;
    store
        .db
        .execute("UPDATE outbox SET state='in_flight'", [])?;
    store.attempted(delivery, &Attempt::Done, 100)?;
    Ok(metadata.id)
}
pub(super) fn audit() -> Result<Audit, Box<dyn std::error::Error>> {
    Ok(Audit {
        actor: Identifier::new("synthetic-operator")?,
        action: Identifier::new("hold")?,
        targets: vec![],
        time: 200,
        reason: Reason::new("synthetic review")?,
    })
}
#[test]
fn adr0013_item1_item6_finite_policy_requires_digest_key() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    assert!(matches!(
        store.configure_retention_key(None, true),
        Err(Error::RetentionKeyRequired)
    ));
    store.configure_retention_key(None, false)?;
    store.configure_retention_key(Some(DigestKey::new([1; 32])), true)?;
    Ok(())
}
#[test]
fn adr0013_item6_fingerprint_mismatch_and_missing_key_refuse() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("store.db");
    let store = open(&path)?;
    let expected = DigestKey::new([1; 32]);
    store.db.execute(
        "INSERT INTO retention_key VALUES (1,?1)",
        [expected.fingerprint().as_slice()],
    )?;
    drop(store);
    let mut store = open(&path)?;
    assert!(matches!(
        store.configure_retention_key(Some(DigestKey::new([2; 32])), true),
        Err(Error::RetentionKeyMismatch)
    ));
    assert!(matches!(
        store.configure_retention_key(None, false),
        Err(Error::RetentionKeyRequired)
    ));
    store.configure_retention_key(Some(expected), true)?;
    Ok(())
}
#[test]
fn adr0013_item2_last_acknowledgment_and_every_delivery_required() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    let id = finished(&mut store)?;
    assert!(!store.retention_eligible(id, 1, 86499)?);
    assert!(store.retention_eligible(id, 1, 86500)?);
    store.db.execute("UPDATE outbox SET state='pending'", [])?;
    assert!(!store.retention_eligible(id, 1, 86500)?);
    Ok(())
}
#[test]
fn adr0013_item2_item7_receipt_hold_blocks_until_audited_release() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    let id = finished(&mut store)?;
    let target = HoldTarget::Receipt(id);
    store.set_retention_hold(&target, true, &audit()?)?;
    assert!(store.retention_hold(&target)?);
    assert!(!store.retention_eligible(id, 1, 86500)?);
    store.set_retention_hold(&target, false, &audit()?)?;
    assert!(store.retention_eligible(id, 1, 86500)?);
    let count: i64 = store
        .db
        .query_row("SELECT count(*) FROM audit", [], |r| r.get(0))?;
    assert_eq!(count, 2);
    let audits = store.audits(0)?;
    assert_eq!(audits.len(), 2);
    assert_eq!(
        audits.first().ok_or("audit")?.1.actor.as_str(),
        "synthetic-operator"
    );
    assert_eq!(
        audits.first().ok_or("audit")?.1.action.as_str(),
        "retention-hold-place"
    );
    Ok(())
}
#[test]
fn adr0013_item3_migration_from_v9_preserves_evidence() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("store.db");
    let mut store = open(&path)?;
    let id = finished(&mut store)?;
    remove_alert_schema_for_legacy_fixture(&store.db)?;
    store.db.execute_batch("DROP TABLE awos_execution_history; DROP TABLE retention_order_deliveries; DROP TABLE retention_orders; DROP TABLE retention_results; DROP TABLE retention_deliveries; DROP TABLE retention_tombstones; DROP TABLE retention_key; DROP TABLE retention_holds; PRAGMA user_version=9;")?;
    drop(store);
    let store = open(&path)?;
    assert_eq!(store.raw(id)?.ok_or("missing raw")?, b"synthetic");
    assert!(store.retention_eligible(id, 1, 86500)?);
    let version: i64 = store
        .db
        .query_row("PRAGMA user_version", [], |r| r.get(0))?;
    assert_eq!(version, crate::SCHEMA_VERSION);
    Ok(())
}

#[test]
fn adr0013_item2_parking_unresolved_orders_and_polls_prevent_purge() -> TestResult {
    for guard in ["parking", "unresolved", "order", "poll", "ack"] {
        let temp = tempfile::tempdir()?;
        let mut store = open(&temp.path().join("store.db"))?;
        let id = finished(&mut store)?;
        assert!(store.retention_eligible(id, 1, 86500)?);
        match guard {
            "parking" => {
                store.park(&crate::Parked {
                    kind: Identifier::new("synthetic")?,
                    reason: Reason::new("review required")?,
                    targets: vec![Identifier::new(&serde_json::from_str::<String>(&key(
                        &id,
                    )?)?)?],
                })?;
            }
            "unresolved" => {
                let mut result = production_result(id)?;
                result.identity = IdentityStatus::Unresolved(Reason::new("missing run")?);
                let mut evidence = interpretation()?;
                evidence.outcome = Reading::Results(vec![result]);
                store.db.execute(
                    "INSERT INTO interpretations VALUES (?1,2,?2)",
                    params![key(&id)?, key(&evidence)?],
                )?;
            }
            "order" => {
                store.db.execute("INSERT INTO machine_orders(destination,order_id,machine,version,state,created_at,updated_at,source_code) VALUES ('app','order','synthetic-machine','v1','Uncertain',0,0,'synthetic')", [])?;
            }
            "poll" => {
                let parked = store.park(&crate::Parked {
                    kind: Identifier::new("poll")?,
                    reason: Reason::new("uncertain")?,
                    targets: vec![Identifier::new("synthetic-machine")?],
                })?;
                store.db.execute("INSERT INTO poll_intents(machine,parked,state,reason) VALUES ('synthetic-machine',?1,'uncertain','synthetic')", [parked])?;
            }
            "ack" => {
                store.db.execute("INSERT INTO attempts(delivery,outcome,time) SELECT id,'\"Done\"',101 FROM outbox", [])?;
            }
            _ => return Err("unknown synthetic guard".into()),
        }
        assert!(!store.retention_eligible(id, 1, 86500)?, "{guard}");
    }
    Ok(())
}
#[test]
fn adr0013_item3_item6_purged_duplicate_is_still_retransmission() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    let credential = DigestKey::new([3; 32]);
    store.configure_retention_key(Some(credential.clone()), true)?;
    let original = receipt(b"original")?;
    let result = production_result(original.id)?;
    let IdentityStatus::Resolved(identity) = &result.identity else {
        return Err("fixture".into());
    };
    let identity_digest = credential.digest(
        "identity",
        key(&(&original.machine, &identity.observation))?.as_bytes(),
    )?;
    let content = crate::results::revisions::content::clinical_digest(&result)?;
    let content_digest = credential.digest("content", &content.0)?;
    store.db.execute("INSERT INTO retention_tombstones VALUES (?1,'synthetic-machine','driver','v1',?2,0,100,86500)", params![key(&original.id)?,original.raw_digest.0.as_slice()])?;
    store.db.execute(
        "INSERT INTO retention_results VALUES (?1,0,?2,?1,?1,?1,?3)",
        params![
            identity_digest.as_slice(),
            key(&original.id)?,
            content_digest.as_slice()
        ],
    )?;
    let new = receipt(b"late resend")?;
    store.receive(&new, b"late resend", b"config")?;
    let mut evidence = interpretation()?;
    evidence.outcome = Reading::Results(vec![production_result(new.id)?]);
    let commit = store.interpret_reconciled(new.id, evidence, &[false], &[], |_, outcomes| {
        assert_eq!(
            outcomes,
            &[crate::RevisionOutcome::Retransmission(
                analyte_model::Revision(0)
            )]
        );
        Ok(vec![])
    })?;
    assert_eq!(commit.outcomes.len(), 1);
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM outbox", [], |r| r.get::<_, i64>(0))?,
        0
    );
    Ok(())
}

#[test]
fn adr0013_item7_order_hold_blocks_linked_receipt_without_expiry() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    let id = finished(&mut store)?;
    let destination = Identifier::new("synthetic")?;
    let order = Identifier::new("test")?;
    store.order_page(&crate::OrderPage {
        destination: destination.clone(),
        cursor: Identifier::new("cursor")?,
        complete: true,
        polled: 100,
        orders: vec![crate::CachedOrder {
            id: order.clone(),
            sample: analyte_model::SampleId::new("000123")?,
            document: "synthetic".into(),
            tombstone: false,
            expiry: 200,
        }],
    })?;
    let target = HoldTarget::Order { destination, order };
    store.set_retention_hold(&target, true, &audit()?)?;
    assert!(!store.retention_eligible(id, 1, i64::MAX)?);
    store.set_retention_hold(&target, false, &audit()?)?;
    assert!(store.retention_eligible(id, 1, i64::MAX)?);
    Ok(())
}

#[test]
fn adr0013_items3_4_5_6_real_purge_audit_refusals_and_late_duplicate() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("store.db");
    let mut store = open(&path)?;
    let id = finished(&mut store)?;
    let delivery: String = store
        .db
        .query_row("SELECT id FROM outbox", [], |r| r.get(0))?;
    let delivery: DeliveryId = serde_json::from_str(&delivery)?;
    store.configure_retention_key(Some(DigestKey::new([7; 32])), true)?;
    let machine = Identifier::new("synthetic-machine")?;
    let run = store.purge_batch(
        &machine,
        Some(1),
        None,
        86500,
        Instant::now().checked_add(HANG_GUARD).ok_or("deadline")?,
    )?;
    assert_eq!(run.purged, 1);
    assert_eq!(run.deliveries, 1);
    assert_eq!(run.results, 1);
    assert_eq!(
        store.audits(0)?.last().ok_or("audit")?.1.action.as_str(),
        "retention-purge"
    );
    assert_eq!(run.first, Some(id));
    assert_eq!(run.last, Some(id));
    assert_eq!(store.last_purge_run()?.ok_or("audit")?.purged, 1);
    assert!(matches!(store.receipt_metadata(id), Err(Error::Purged)));
    assert!(matches!(store.receipt(id), Err(Error::Purged)));
    assert!(matches!(
        store.resend_delivery(delivery, false, &audit()?),
        Err(Error::Purged)
    ));
    for table in [
        "receipts",
        "interpretations",
        "outbox",
        "attempts",
        "observation_revisions",
    ] {
        assert_eq!(
            store
                .db
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                    .get::<_, i64>(0))?,
            0
        );
    }
    let new = receipt(b"late repeat")?;
    store.receive(&new, b"late repeat", b"config")?;
    let mut evidence = interpretation()?;
    evidence.outcome = Reading::Results(vec![production_result(new.id)?]);
    let commit = store.interpret_reconciled(new.id, evidence, &[false], &[], |_, _| Ok(vec![]))?;
    assert_eq!(
        commit.outcomes,
        vec![crate::RevisionOutcome::Retransmission(
            analyte_model::Revision(0)
        )]
    );
    assert!(store.db.execute("DELETE FROM receipts", []).is_err());
    drop(store);
    let mut store = open(&path)?;
    assert!(matches!(
        store.configure_retention_key(Some(DigestKey::new([8; 32])), true),
        Err(Error::RetentionKeyMismatch)
    ));
    store.configure_retention_key(Some(DigestKey::new([7; 32])), true)?;
    Ok(())
}
#[test]
fn adr0013_item4_backup_and_restore_pause_purge_and_keep_is_audited() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("store.db");
    let mut store = open(&path)?;
    let id = finished(&mut store)?;
    store.configure_retention_key(Some(DigestKey::new([9; 32])), true)?;
    let machine = Identifier::new("synthetic-machine")?;
    let until = || Instant::now().checked_add(HANG_GUARD).ok_or("deadline");
    let maintenance = maintenance(&path, false)?;
    let run = store.purge_batch(&machine, Some(1), None, 86500, until()?)?;
    assert!(run.paused);
    assert_eq!(run.purged, 0);
    drop(maintenance);
    let run = store.purge_batch(&machine, None, None, 86500, until()?)?;
    assert_eq!(run.purged, 0);
    assert_eq!(store.last_purge_run()?.ok_or("audit")?.days, None);
    let mut restore = audit()?;
    restore.action = Identifier::new("store-restore")?;
    store.audit(&restore)?;
    assert!(
        store
            .purge_batch(&machine, Some(1), None, 86500, until()?)?
            .paused
    );
    assert!(store.receipt(id)?.is_some());
    Ok(())
}
#[test]
fn adr0013_item4_crash_child() -> TestResult {
    let Some(path) = std::env::var_os("ANALYTE_RETENTION_CRASH_FIXTURE") else {
        return Ok(());
    };
    let mut store = open(std::path::Path::new(&path))?;
    let id: String = store
        .db
        .query_row("SELECT id FROM receipts LIMIT 1", [], |r| r.get(0))?;
    let id = serde_json::from_str(&id)?;
    let tx = store.db.transaction()?;
    tombstones::remove_delete_guards(&tx)?;
    tombstones::purge(&tx, &DigestKey::new([10; 32]), id, 86500, 65536)?;
    // Abrupt termination skips all destructors and SQLite rollback code.
    std::process::exit(99);
}
#[test]
fn adr0013_item4_crash_mid_batch_rolls_back_data_tombstones_and_guards() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("store.db");
    let mut store = open(&path)?;
    let id = finished(&mut store)?;
    drop(store);
    let mut child = std::process::Command::new(std::env::current_exe()?)
        .arg("--exact")
        .arg("upkeep::retention::tests::adr0013_item4_crash_child")
        .env("ANALYTE_RETENTION_CRASH_FIXTURE", &path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    let deadline = Instant::now().checked_add(HANG_GUARD).ok_or("deadline")?;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            child.kill()?;
            return Err("crash fixture exceeded deadline".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    };
    assert_eq!(status.code(), Some(99));
    let store = open(&path)?;
    assert_eq!(store.receipt(id)?.ok_or("receipt")?.raw, b"synthetic");
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM retention_tombstones", [], |r| r
                .get::<_, i64>(0))?,
        0
    );
    assert!(store.db.execute("DELETE FROM receipts", []).is_err());
    Store::integrity(&store.db)?;
    Ok(())
}
proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(64))]
    #[test]
    fn adr0013_item2_random_unfinished_states_never_purge(mask in 0u8..128) {
        let result = property_batch(mask);
        proptest::prop_assert!(result.is_ok(), "{:?}",result.err());
    }
}
fn property_batch(mask: u8) -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    let id = finished(&mut store)?;
    store.configure_retention_key(Some(DigestKey::new([11; 32])), true)?;
    if mask & 1 != 0 {
        store.db.execute("UPDATE outbox SET state='pending'", [])?;
    }
    if mask & 2 != 0 {
        store.park(&crate::Parked {
            kind: Identifier::new("test")?,
            reason: Reason::new("unfinished")?,
            targets: vec![Identifier::new(&serde_json::from_str::<String>(&key(
                &id,
            )?)?)?],
        })?;
    }
    if mask & 4 != 0 {
        store.set_retention_hold(&HoldTarget::Receipt(id), true, &audit()?)?;
    }
    if mask & 8 != 0 {
        store.db.execute(
            "INSERT INTO attempts(delivery,outcome,time) SELECT id,'\"Done\"',200 FROM outbox",
            [],
        )?;
    }
    if mask & 16 != 0 {
        store.db.execute("INSERT INTO machine_orders(destination,order_id,machine,version,state,created_at,updated_at,source_code) VALUES ('app','order','synthetic-machine','v1','Uncertain',0,0,'synthetic')", [])?;
    }
    if mask & 32 != 0 {
        let parked = store.park(&crate::Parked {
            kind: Identifier::new("poll")?,
            reason: Reason::new("uncertain")?,
            targets: vec![Identifier::new("synthetic-machine")?],
        })?;
        store.db.execute("INSERT INTO poll_intents(machine,parked,state,reason) VALUES ('synthetic-machine',?1,'uncertain','synthetic')",[parked])?;
    }
    if mask & 64 != 0 {
        let mut result = production_result(id)?;
        result.identity = IdentityStatus::Unresolved(Reason::new("unresolved")?);
        let mut evidence = interpretation()?;
        evidence.outcome = Reading::Results(vec![result]);
        store.db.execute(
            "INSERT INTO interpretations VALUES (?1,2,?2)",
            params![key(&id)?, key(&evidence)?],
        )?;
    }
    let run = store.purge_batch(
        &Identifier::new("synthetic-machine")?,
        Some(1),
        None,
        86500,
        Instant::now().checked_add(HANG_GUARD).ok_or("deadline")?,
    )?;
    assert_eq!(run.purged, usize::from(mask == 0));
    Store::integrity(&store.db)?;
    Ok(())
}

#[test]
fn adr0013_item4_offline_conversion_and_incremental_vacuum_return_space() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("store.db");
    let mut store = open(&path)?;
    let id = finished(&mut store)?;
    drop(store);
    let db = rusqlite::Connection::open(&path)?;
    db.execute_batch("PRAGMA auto_vacuum=NONE; VACUUM;")?;
    drop(db);
    let store = open(&path)?;
    assert!(matches!(
        store.retention_space_ready(),
        Err(Error::RetentionVacuumRequired)
    ));
    assert!(matches!(
        Store::prepare_retention_space(
            &path,
            limits(),
            Box::new(Space(u64::MAX)),
            Instant::now().checked_add(HANG_GUARD).ok_or("deadline")?
        ),
        Err(Error::InUse)
    ));
    drop(store);
    Store::prepare_retention_space(
        &path,
        limits(),
        Box::new(Space(u64::MAX)),
        Instant::now().checked_add(HANG_GUARD).ok_or("deadline")?,
    )?;
    let mut store = open(&path)?;
    store.retention_space_ready()?;
    assert!(store.receipt(id)?.is_some());
    store.db.execute_batch("CREATE TABLE synthetic_space(payload BLOB); INSERT INTO synthetic_space VALUES(zeroblob(1048576)); DROP TABLE synthetic_space; PRAGMA wal_checkpoint(TRUNCATE);")?;
    let before = std::fs::metadata(&path)?.len();
    store.reclaim_retention_space(Instant::now().checked_add(HANG_GUARD).ok_or("deadline")?)?;
    assert!(std::fs::metadata(&path)?.len() < before);
    assert!(store.receipt(id)?.is_some());
    Ok(())
}

#[test]
fn adr0013_item4_audit_failure_rolls_back_purge_and_restores_guards() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    let id = finished(&mut store)?;
    store.configure_retention_key(Some(DigestKey::new([12; 32])), true)?;
    store.db.execute_batch("CREATE TRIGGER synthetic_audit_failure BEFORE INSERT ON audit BEGIN SELECT RAISE(ABORT,'synthetic'); END;")?;
    let result = store.purge_batch(
        &Identifier::new("synthetic-machine")?,
        Some(1),
        None,
        86500,
        Instant::now().checked_add(HANG_GUARD).ok_or("deadline")?,
    );
    assert!(result.is_err());
    assert!(store.receipt(id)?.is_some());
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM retention_tombstones", [], |r| r
                .get::<_, i64>(0))?,
        0
    );
    assert!(store.db.execute("DELETE FROM receipts", []).is_err());
    Store::integrity(&store.db)?;
    Ok(())
}

mod refinement;
