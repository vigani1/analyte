//! Tests certain and uncertain retention links against ADR 0013 items 2 and 7.
use super::*;
fn cached(store: &mut Store, id: &str, sample: &str) -> TestResult {
    store.order_page(&crate::OrderPage {
        destination: Identifier::new("app")?,
        cursor: Identifier::new("cursor")?,
        complete: true,
        polled: 100,
        orders: vec![crate::CachedOrder {
            id: Identifier::new(id)?,
            sample: analyte_model::SampleId::new(sample)?,
            document: r#"{"version":1,"state":"Requested","tombstone":false,"tests":[]}"#.into(),
            tombstone: false,
            expiry: 100000,
        }],
    })?;
    Ok(())
}
// ADR 0013 item 7
#[test]
fn unrelated_order_hold_does_not_block_sample() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    let receipt = finished(&mut store)?;
    cached(&mut store, "other", "different-sample")?;
    store.set_retention_hold(
        &HoldTarget::Order {
            destination: Identifier::new("app")?,
            order: Identifier::new("other")?,
        },
        true,
        &audit()?,
    )?;
    assert!(store.retention_eligible(receipt, 1, 86500)?);
    store.db.execute("UPDATE orders SET sample='000123'", [])?;
    assert!(!store.retention_eligible(receipt, 1, 86500)?);
    store.db.execute("DELETE FROM orders", [])?;
    assert!(!store.retention_eligible(receipt, 1, 86500)?);
    Ok(())
}
// ADR 0013 item 2
#[test]
fn open_work_blocks_only_related_or_unknown_receipts() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    let receipt = finished(&mut store)?;
    cached(&mut store, "other", "different-sample")?;
    store.db.execute("INSERT INTO machine_orders(destination,order_id,machine,version,state,created_at,updated_at,source_code) VALUES ('app','other','synthetic-machine','1','Uncertain',0,100,'synthetic')",[])?;
    assert!(store.retention_eligible(receipt, 1, 86500)?);
    store.db.execute("UPDATE orders SET sample='000123'", [])?;
    assert!(!store.retention_eligible(receipt, 1, 86500)?);
    Ok(())
}
// ADR 0013 item 2
#[test]
fn explicit_poll_receipt_targets_are_narrowed() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    let receipt = finished(&mut store)?;
    let other = crate::test_support::receipt(b"other")?;
    store.receive(&other, b"other", b"config")?;
    let target: String = serde_json::from_str(&key(&other.id)?)?;
    let parked = store.park(&crate::Parked {
        kind: Identifier::new("poll")?,
        reason: Reason::new("synthetic")?,
        targets: vec![Identifier::new(&target)?],
    })?;
    store.db.execute("INSERT INTO poll_intents(machine,parked,state,reason) VALUES ('synthetic-machine',?1,'uncertain','synthetic')",params![parked])?;
    assert!(store.retention_eligible(receipt, 1, 86500)?);
    assert!(!store.retention_eligible(other.id, 1, 86500)?);
    Ok(())
}

fn order_seed(store: &mut Store, state: &str) -> Result<i64, Box<dyn std::error::Error>> {
    cached(store, "order", "sample")?;
    store.db.execute("INSERT INTO machine_orders(destination,order_id,machine,version,state,created_at,updated_at,source_code) VALUES ('app','order','synthetic-machine','1',?1,0,100,'synthetic')",[state])?;
    Ok(store.db.last_insert_rowid())
}
fn order_run(store: &mut Store, now: i64) -> Result<PurgeRun, Error> {
    store.purge_order_batch(
        &Identifier::new("synthetic-machine").map_err(crate::StorageError::Text)?,
        Some(1),
        None,
        now,
        Instant::now()
            .checked_add(HANG_GUARD)
            .ok_or(Error::InvalidMaintenanceInput)?,
    )
}
// ADR 0013 item 2
#[test]
fn final_orders_purge_after_period_keep_replay_guard() -> TestResult {
    for state in ["Done", "Rejected", "CancelConfirmed"] {
        let temp = tempfile::tempdir()?;
        let mut store = open(&temp.path().join("store.db"))?;
        store.configure_retention_key(Some(DigestKey::new([1; 32])), true)?;
        let id = order_seed(&mut store, state)?;
        assert_eq!(order_run(&mut store, 86499)?.orders, 0);
        let run = order_run(&mut store, 86500)?;
        assert_eq!(run.orders, 1);
        assert_eq!(run.order_range, Some((id, id)));
        assert!(
            store
                .cached_order(&Identifier::new("app")?, &Identifier::new("order")?)?
                .is_none()
        );
        let identity = crate::MachineOrderKey {
            destination: Identifier::new("app")?,
            order: Identifier::new("order")?,
            machine: Identifier::new("synthetic-machine")?,
        };
        assert!(matches!(
            store.machine_order(&identity),
            Err(crate::OrderError::Purged)
        ));
        cached(&mut store, "order", "sample")?;
        let result = store.reserve_send(
            &crate::OrderReservation {
                model: &order_model(),
                identity: &identity,
                version: std::num::NonZeroU64::MIN,
                now: 100,
                freshness: 1000,
            },
            crate::ReserveOptions::default(),
        );
        assert!(matches!(result, Err(crate::OrderError::Purged)));
        assert_constraint(store.db.execute("DELETE FROM retention_orders", []));
        Store::integrity(&store.db)?;
    }
    Ok(())
}
proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(64))]
    // ADR 0013 item 2
    #[test]
    fn random_order_states_keep_unfinished_work(state in 0usize..8,held in proptest::bool::ANY,recent in proptest::bool::ANY) {
        let result=order_property(state,held,recent);
        proptest::prop_assert!(result.is_ok(),"{:?}",result.err());
    }
}
fn order_property(state: usize, held: bool, recent: bool) -> TestResult {
    let states = [
        "Sending",
        "Sent",
        "Accepted",
        "Done",
        "Rejected",
        "CancelRequested",
        "CancelConfirmed",
        "Uncertain",
    ];
    let state = *states.get(state).ok_or("state")?;
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    store.configure_retention_key(Some(DigestKey::new([2; 32])), true)?;
    order_seed(&mut store, state)?;
    if held {
        store.set_retention_hold(
            &HoldTarget::Order {
                destination: Identifier::new("app")?,
                order: Identifier::new("order")?,
            },
            true,
            &audit()?,
        )?;
    }
    let expected = matches!(state, "Done" | "Rejected" | "CancelConfirmed") && !held && !recent;
    assert_eq!(
        order_run(&mut store, if recent { 86499 } else { 86500 })?.orders,
        usize::from(expected)
    );
    Store::integrity(&store.db)?;
    Ok(())
}

// ADR 0013 item 2
#[test]
fn unfinished_work_blocks_order_purge() -> TestResult {
    for guard in ["item", "event", "receipt", "audit"] {
        let temp = tempfile::tempdir()?;
        let mut store = open(&temp.path().join("store.db"))?;
        store.configure_retention_key(Some(DigestKey::new([3; 32])), true)?;
        let id = if guard == "event" {
            cached(&mut store, "order", "sample")?;
            crate::machine_orders::tests::seed_order(
                &mut store,
                crate::MachineOrderKey {
                    destination: Identifier::new("app")?,
                    order: Identifier::new("order")?,
                    machine: Identifier::new("synthetic-machine")?,
                },
                analyte_model::OrderState::Accepted,
                100,
            )?
        } else {
            order_seed(&mut store, "Done")?
        };
        match guard {
            "item" => {
                store.db.execute("INSERT INTO machine_order_items(machine_order,item_index,tests,coverage_known,placer,state) VALUES (?1,1,'[]',0,'placer','Uncertain')",[id])?;
            }
            "event" => {
                let event = crate::OrderEventDelivery {
                    id: DeliveryId::try_from(uuid::Uuid::now_v7())?,
                    order_id: Identifier::new("order")?,
                    destination: Identifier::new("app")?,
                    payload: b"synthetic".to_vec(),
                    contract_version: 1,
                };
                store.transition_order(
                    id,
                    analyte_model::OrderState::Done,
                    analyte_model::SourceText::new("synthetic")?,
                    100,
                    |_| Ok(event),
                )?;
            }
            "receipt" => {
                finished(&mut store)?;
                store.db.execute("UPDATE orders SET sample='000123'", [])?;
            }
            "audit" => {
                store.db.execute_batch("CREATE TRIGGER synthetic_audit_failure BEFORE INSERT ON audit BEGIN SELECT RAISE(ABORT,'synthetic'); END;")?;
            }
            _ => return Err("guard".into()),
        }
        let result = order_run(&mut store, 86500);
        if guard == "audit" {
            {
                let refusal = result.map(|_| ()).expect_err("input must be refused");
                assert!(
                    matches!(
                        &refusal,
                        Error::Storage(crate::StorageError::ConstraintViolation(_))
                    ),
                    "unexpected refusal: {refusal:?}"
                );
            }
        } else {
            assert_eq!(result?.orders, 0);
        }
        assert_eq!(
            store
                .db
                .query_row("SELECT count(*) FROM machine_orders", [], |r| r
                    .get::<_, i64>(0))?,
            1
        );
        Store::integrity(&store.db)?;
    }
    Ok(())
}
// ADR 0013 item 2
#[test]
fn newer_cached_order_work_is_never_purged() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    store.configure_retention_key(Some(DigestKey::new([4; 32])), true)?;
    order_seed(&mut store, "Done")?;
    let replacement = crate::CachedOrder {
        id: Identifier::new("order")?,
        sample: analyte_model::SampleId::new("sample")?,
        document: r#"{"version":2,"state":"Requested","tombstone":false,"tests":[]}"#.into(),
        tombstone: false,
        expiry: 100000,
    };
    store.db.execute(
        "UPDATE orders SET evidence=?1",
        [crate::document(&replacement)?],
    )?;
    assert_eq!(order_run(&mut store, 86500)?.orders, 0);
    assert!(
        store
            .cached_order(&Identifier::new("app")?, &Identifier::new("order")?)?
            .is_some()
    );
    Ok(())
}
