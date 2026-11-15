//! ADR 0004 and safety rule 8: atomic reservation and no automatic replay.
use super::*;
use crate::tests::{TestResult, open};
use analyte_model::{OrderState, SourceText};
use std::num::NonZeroU64;

fn identity() -> Result<MachineOrderKey, analyte_model::TextError> {
    Ok(MachineOrderKey {
        destination: Identifier::new("app")?,
        order: Identifier::new("order")?,
        machine: Identifier::new("machine")?,
    })
}

fn page(
    version: u64,
    state: &str,
    tombstone: bool,
    expiry: i64,
) -> Result<OrderPage, Box<dyn std::error::Error>> {
    Ok(OrderPage {
        destination: Identifier::new("app")?,
        cursor: Identifier::new("cursor")?,
        complete: true,
        polled: 100,
        orders: vec![CachedOrder {
            id: Identifier::new("order")?,
            sample: SampleId::new("001a")?,
            document: serde_json::json!({"version":version,"state":state,"tombstone":tombstone})
                .to_string(),
            tombstone,
            expiry,
        }],
    })
}

fn event(order: &MachineOrder) -> Result<OrderEventDelivery, Error> {
    Ok(OrderEventDelivery {
        id: DeliveryId::try_from(uuid::Uuid::now_v7()).map_err(|_| Error::InvalidInput)?,
        destination: order.key.destination.clone(),
        order_id: order.key.order.clone(),
        payload: serde_json::to_vec(order)?,
        contract_version: 1,
    })
}

#[test]
fn adr0004_reserve_transition_outbox_and_restart_never_resend() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("orders.db");
    let mut store = open(&path)?;
    store.order_page(&page(1, "requested", false, 200)?)?;
    let reserved = store
        .reserve_send(&identity()?, NonZeroU64::MIN, 101, 30, event)?
        .map_err(std::io::Error::other)?;
    assert_eq!(reserved.state, OrderState::Sent);
    assert!(matches!(
        store.reserve_send(&identity()?, NonZeroU64::MIN, 101, 30, event)?,
        Err(SendRefusal::AlreadyReserved)
    ));
    let uncertain = store.transition_order(
        reserved.id,
        OrderState::Uncertain,
        SourceText::new("timeout")?,
        102,
        event,
    )?;
    assert_eq!(uncertain.state, OrderState::Uncertain);
    assert!(
        store
            .transition_order(
                reserved.id,
                OrderState::Requested,
                SourceText::new("invalid")?,
                103,
                event
            )
            .is_err()
    );
    drop(store);
    let mut store = open(&path)?;
    store.order_page(&page(2, "requested", false, 200)?)?;
    let version = NonZeroU64::new(2).ok_or("version")?;
    assert!(matches!(
        store.reserve_send(&identity()?, version, 103, 30, event)?,
        Err(SendRefusal::AlreadyReserved)
    ));
    let audit = Audit {
        actor: Identifier::new("operator")?,
        action: Identifier::new("orders-reconcile")?,
        targets: vec![Identifier::new(&reserved.id.to_string())?],
        time: 104,
        reason: analyte_model::Reason::new("checked synthetic machine work list")?,
    };
    store.reconcile_order(reserved.id, OrderState::Accepted, &audit, event)?;
    assert_eq!(
        store
            .machine_orders(&MachineOrderFilter {
                after: 0,
                machine: None,
                state: Some(OrderState::Accepted)
            })?
            .len(),
        1
    );
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM machine_order_history", [], |r| r
                .get::<_, i64>(0))?,
        3
    );
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM outbox", [], |r| r.get::<_, i64>(0))?,
        3
    );
    assert_eq!(store.audits(0)?.len(), 1);
    assert!(
        store
            .db
            .execute("DELETE FROM machine_order_history", [])
            .is_err()
    );
    assert!(
        store
            .db
            .execute("UPDATE machine_order_history SET evidence='{}'", [])
            .is_err()
    );
    Ok(())
}

#[test]
fn adr0004_cancel_expiry_version_trust_and_builder_failure_never_reserve() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("orders.db"))?;
    for (version, state, tombstone, expiry, now, expected) in [
        (2, "requested", false, 200, 101, SendRefusal::VersionChanged),
        (
            1,
            "cancel_requested",
            false,
            200,
            101,
            SendRefusal::NotRequested,
        ),
        (1, "requested", true, 200, 101, SendRefusal::Withdrawn),
        (1, "requested", false, 101, 101, SendRefusal::Expired),
        (1, "requested", false, 200, 130, SendRefusal::Untrusted),
    ] {
        store.order_page(&page(version, state, tombstone, expiry)?)?;
        assert!(
            matches!(store.reserve_send(&identity()?,NonZeroU64::MIN,now,30,event)?,Err(reason) if reason == expected)
        );
    }
    store.order_page(&page(1, "requested", false, 200)?)?;
    assert!(
        store
            .reserve_send(&identity()?, NonZeroU64::MIN, 101, 30, |_| Err(
                Error::InvalidInput
            ))
            .is_err()
    );
    assert!(
        store
            .machine_orders(&MachineOrderFilter {
                after: 0,
                machine: None,
                state: None
            })?
            .is_empty()
    );
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM outbox", [], |r| r.get::<_, i64>(0))?,
        0
    );
    Ok(())
}

#[test]
fn plan11_schema3_to4_preserves_rows_and_triggers() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("legacy.db");
    let mut db = Connection::open(&path)?;
    db.execute_batch(include_str!("schema.sql"))?;
    Store::migrate_v2(&mut db)?;
    Store::migrate_v3(&mut db, 65_536)?;
    db.execute("INSERT INTO audit(evidence) VALUES ('legacy-evidence')", [])?;
    db.execute_batch("CREATE TRIGGER legacy_audit_retained BEFORE DELETE ON audit BEGIN SELECT RAISE(ABORT,'retained'); END;")?;
    let before = db
        .prepare("SELECT name,sql FROM sqlite_master WHERE type='trigger' ORDER BY name")?
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    drop(db);
    let store = open(&path)?;
    assert_eq!(
        store
            .db
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))?,
        4
    );
    assert_eq!(
        store
            .db
            .query_row("SELECT evidence FROM audit", [], |r| r.get::<_, String>(0))?,
        "legacy-evidence"
    );
    for (name, sql) in before {
        assert_eq!(
            store.db.query_row(
                "SELECT sql FROM sqlite_master WHERE type='trigger' AND name=?1",
                [name],
                |r| r.get::<_, String>(0)
            )?,
            sql
        );
    }
    Ok(())
}
