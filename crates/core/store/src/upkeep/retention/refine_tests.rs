//! Tests certain and uncertain retention links against ADR 0013 items 2 and 7.
use super::*;
use crate::test_support::*;
use crate::{Identifier, key};
use rusqlite::params;
fn cached(store: &mut Store, id: &str, sample: &str) -> TestResult {
    store.order_page(&crate::OrderPage {
        destination: Identifier::new("app")?,
        cursor: Identifier::new("cursor")?,
        complete: true,
        polled: 100,
        orders: vec![crate::CachedOrder {
            id: Identifier::new(id)?,
            sample: analyte_model::SampleId::new(sample)?,
            document: "synthetic".into(),
            tombstone: false,
            expiry: 100000,
        }],
    })?;
    Ok(())
}
#[test]
fn adr0013_item7_unrelated_order_hold_does_not_block_known_sample() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    let receipt = tests::finished(&mut store)?;
    cached(&mut store, "other", "different-sample")?;
    store.set_retention_hold(
        &HoldTarget::Order {
            destination: Identifier::new("app")?,
            order: Identifier::new("other")?,
        },
        true,
        &tests::audit()?,
    )?;
    assert!(store.retention_eligible(receipt, 1, 86500)?);
    store.db.execute("UPDATE orders SET sample='000123'", [])?;
    assert!(!store.retention_eligible(receipt, 1, 86500)?);
    store.db.execute("DELETE FROM orders", [])?;
    assert!(!store.retention_eligible(receipt, 1, 86500)?);
    Ok(())
}
#[test]
fn adr0013_item2_open_work_only_blocks_related_or_unknown_receipts() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    let receipt = tests::finished(&mut store)?;
    cached(&mut store, "other", "different-sample")?;
    store.db.execute("INSERT INTO machine_orders(destination,order_id,machine,version,state,created_at,updated_at,source_code) VALUES ('app','other','synthetic-machine','1','Uncertain',0,100,'synthetic')",[])?;
    assert!(store.retention_eligible(receipt, 1, 86500)?);
    store.db.execute("UPDATE orders SET sample='000123'", [])?;
    assert!(!store.retention_eligible(receipt, 1, 86500)?);
    Ok(())
}
#[test]
fn adr0013_item2_explicit_poll_receipt_targets_are_narrowed() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    let receipt = tests::finished(&mut store)?;
    let other = crate::test_support::receipt(b"other")?;
    store.receive(&other, b"other", b"config")?;
    let target: String = serde_json::from_str(&key(&other.id)?)?;
    let parked = store.park(&crate::Parked {
        kind: Identifier::new("poll")?,
        reason: analyte_model::Reason::new("synthetic")?,
        targets: vec![Identifier::new(&target)?],
    })?;
    store.db.execute("INSERT INTO poll_intents(machine,parked,state,reason) VALUES ('synthetic-machine',?1,'uncertain','synthetic')",params![parked])?;
    assert!(store.retention_eligible(receipt, 1, 86500)?);
    assert!(!store.retention_eligible(other.id, 1, 86500)?);
    Ok(())
}
