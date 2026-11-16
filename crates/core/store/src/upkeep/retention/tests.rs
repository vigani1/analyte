//! Tests retention migration, key continuity and finished-data guards against ADR 0013.
use super::*;
use crate::test_support::*;
use crate::{Attempt, Audit, DeliveryId, Identifier, Reading, key};
use analyte_model::{IdentityStatus, Reason};
use rusqlite::params;

fn finished(store: &mut Store) -> Result<analyte_model::ReceiptId, Box<dyn std::error::Error>> {
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
fn audit() -> Result<Audit, Box<dyn std::error::Error>> {
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
    store.db.execute_batch("DROP TABLE retention_results; DROP TABLE retention_deliveries; DROP TABLE retention_tombstones; DROP TABLE retention_key; DROP TABLE retention_holds; PRAGMA user_version=9;")?;
    drop(store);
    let store = open(&path)?;
    assert_eq!(store.raw(id)?.ok_or("missing raw")?, b"synthetic");
    assert!(store.retention_eligible(id, 1, 86500)?);
    let version: i64 = store
        .db
        .query_row("PRAGMA user_version", [], |r| r.get(0))?;
    assert_eq!(version, 10);
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
