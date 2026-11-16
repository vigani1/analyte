//! Tests machine order provenance and ownership checks.
use super::*;

#[test]
fn law_tf2b_3_27_4_sample_and_pending_share_freshness_generation_and_restore_trust() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("trust.db"))?;
    let key = identity()?;
    store.order_page(&page(1, "requested", false, 200)?)?;
    for (now, trusted) in [(99, false), (100, true), (129, true), (130, false)] {
        assert_eq!(
            matches!(
                store.orders(&key.destination, &SampleId::new("001a")?, now, 30)?,
                Lookup::Orders(_)
            ),
            trusted
        );
        assert_eq!(
            matches!(
                store.pending_orders(&key.destination, &key.machine, "", now, 30)?,
                PendingOrders::Page { .. }
            ),
            trusted
        );
    }
    for update in [
        "UPDATE feed SET complete=0",
        "UPDATE feed SET trusted=0",
        "UPDATE feed SET polled=102",
    ] {
        store.order_page(&page(1, "requested", false, 200)?)?;
        store.db.execute_batch(update)?;
        assert!(matches!(
            store.orders(&key.destination, &SampleId::new("001a")?, 101, 30)?,
            Lookup::NotTrusted
        ));
        assert!(matches!(
            store.pending_orders(&key.destination, &key.machine, "", 101, 30)?,
            PendingOrders::NotTrusted
        ));
    }
    store.reset_orders(&key.destination)?;
    assert!(matches!(
        store.pending_orders(&key.destination, &key.machine, "", 101, 30)?,
        PendingOrders::NotTrusted
    ));
    let mut empty = page(1, "requested", false, 200)?;
    empty.orders.clear();
    store.order_page(&empty)?;
    let PendingOrders::Page { orders, after } =
        store.pending_orders(&key.destination, &key.machine, "", 101, 30)?
    else {
        return Err("new generation should be trusted".into());
    };
    assert!(orders.is_empty() && after.is_none());
    store.audit(&Audit {
        actor: Identifier::new("operator")?,
        action: Identifier::new("store-restore")?,
        targets: vec![],
        time: 101,
        reason: analyte_model::Reason::new("synthetic restore")?,
    })?;
    assert!(matches!(
        store.orders(&key.destination, &SampleId::new("001a")?, 101, 30)?,
        Lookup::NotTrusted
    ));
    assert!(matches!(
        store.pending_orders(&key.destination, &key.machine, "", 101, 30)?,
        PendingOrders::NotTrusted
    ));
    Ok(())
}

#[test]
fn plan11_rule2_restore_keeps_current_and_rule8_requires_audited_resume() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("store.db");
    let backup = temp.path().join("backup.db");
    let mut store = open(&path)?;
    store.order_page(&page(1, "requested", false, 200)?)?;
    store.backup(&backup, Instant::now() + test_support::HANG_GUARD)?;
    let order = store
        .reserve_send(
            &OrderReservation {
                identity: &identity()?,
                version: NonZeroU64::MIN,
                now: 101,
                freshness: 30,
            },
            ReserveOptions::default(),
        )?
        .map_err(std::io::Error::other)?;
    assert_eq!(order.state, OrderState::Sending);
    let audit = Audit {
        actor: Identifier::new("operator")?,
        action: Identifier::new("store-restore")?,
        targets: vec![],
        time: 102,
        reason: analyte_model::Reason::new("verified recovery")?,
    };
    assert!(matches!(
        Store::restore(
            &path,
            &backup,
            test_support::limits(),
            Box::new(test_support::Space(u64::MAX)),
            &audit,
            Instant::now() + test_support::HANG_GUARD
        ),
        Err(Error::InUse)
    ));
    drop(store);
    let kept = Store::restore(
        &path,
        &backup,
        test_support::limits(),
        Box::new(test_support::Space(u64::MAX)),
        &audit,
        Instant::now() + test_support::HANG_GUARD,
    )?;
    assert!(kept.exists());
    let old = open(&kept)?;
    assert_eq!(
        old.machine_orders(&MachineOrderFilter {
            after: 0,
            machine: None,
            state: None
        })?
        .len(),
        1
    );
    let mut store = open(&path)?;
    assert!(store.restore_unreconciled()?);
    assert!(matches!(
        store.reserve_send(
            &OrderReservation {
                identity: &identity()?,
                version: NonZeroU64::MIN,
                now: 103,
                freshness: 30
            },
            ReserveOptions::default()
        )?,
        Err(SendRefusal::RestoreUnreconciled)
    ));
    assert!(matches!(
        store.orders(&Identifier::new("app")?, &SampleId::new("001a")?, 103, 30)?,
        Lookup::NotTrusted
    ));
    verify_plan11_rule8_audited_resume(&mut store, audit)
}

#[test]
fn plan11_restore_pause_blocks_hl7_items_and_owned_dsr_in_shared_transaction() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("paused.db"))?;
    store.order_page(&page(1, "requested", false, 200)?)?;
    let mut audit = Audit {
        actor: Identifier::new("operator")?,
        action: Identifier::new("store-restore")?,
        targets: Vec::new(),
        time: 101,
        reason: analyte_model::Reason::new("synthetic restore")?,
    };
    store.audit(&audit)?;
    let identity = identity()?;
    let candidate = OrderReservation {
        identity: &identity,
        version: NonZeroU64::MIN,
        now: 102,
        freshness: 30,
    };
    assert!(matches!(
        store.reserve_send(
            &candidate,
            ReserveOptions {
                items: OrderItems::Groups {
                    groups: &test_groups(&["test"])?,
                    namespace: Some(&Identifier::new("lab")?),
                    max_identity_bytes: 18
                },
                ..Default::default()
            }
        )?,
        Err(SendRefusal::RestoreUnreconciled)
    ));
    assert!(matches!(
        store.reserve_send(
            &candidate,
            ReserveOptions {
                owner: Some(&Identifier::new("connection")?),
                ..Default::default()
            }
        )?,
        Err(SendRefusal::RestoreUnreconciled)
    ));
    for table in [
        "machine_orders",
        "machine_order_items",
        "machine_order_history",
        "machine_order_item_history",
        "outbox",
    ] {
        assert_eq!(
            store
                .db
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| row
                    .get::<_, i64>(0))?,
            0
        );
    }
    audit.action = Identifier::new("orders-resume-after-restore")?;
    store.resume_after_restore(&audit)?;
    assert!(
        store
            .reserve_send(
                &candidate,
                ReserveOptions {
                    items: OrderItems::Groups {
                        groups: &test_groups(&["test"])?,
                        namespace: Some(&Identifier::new("lab")?),
                        max_identity_bytes: 18
                    },
                    ..Default::default()
                }
            )?
            .is_ok()
    );
    Ok(())
}

#[test]
fn adr0003_hl7_item_preflight_failure_is_atomic_and_restore_guard_runs_first() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("preflight.db"))?;
    store.order_page(&page(1, "requested", false, 200)?)?;
    let identity = identity()?;
    let candidate = OrderReservation {
        identity: &identity,
        version: NonZeroU64::MIN,
        now: 102,
        freshness: 30,
    };
    let owner = Identifier::new("connection")?;
    let namespace = Identifier::new("LAB")?;
    assert!(matches!(
        store.reserve_send(
            &candidate,
            ReserveOptions {
                items: OrderItems::Groups {
                    groups: &test_groups(&["test"])?,
                    namespace: Some(&namespace),
                    max_identity_bytes: 18
                },
                owner: Some(&owner),
                prepare: Some(&mut |record| -> Result<(), Error> {
                    assert_eq!(
                        record.items.first().ok_or(Error::InvalidOrderInput)?.placer,
                        format!("{}-1", record.id)
                    );
                    Err(Error::InvalidOrderInput)
                }),
            }
        ),
        Err(Error::InvalidOrderInput)
    ));
    for table in [
        "machine_orders",
        "machine_order_items",
        "machine_order_history",
        "machine_order_item_history",
        "outbox",
    ] {
        assert_eq!(
            store
                .db
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| row
                    .get::<_, i64>(0))?,
            0
        );
    }
    store.audit(&Audit {
        actor: Identifier::new("operator")?,
        action: Identifier::new("store-restore")?,
        targets: Vec::new(),
        time: 101,
        reason: analyte_model::Reason::new("synthetic restore")?,
    })?;
    assert!(matches!(
        store.reserve_send(
            &candidate,
            ReserveOptions {
                items: OrderItems::Groups {
                    groups: &test_groups(&["test"])?,
                    namespace: Some(&namespace),
                    max_identity_bytes: 18
                },
                owner: Some(&owner),
                prepare: Some(&mut |_| -> Result<(), Error> { Err(Error::InvalidOrderInput) }),
            }
        )?,
        Err(SendRefusal::RestoreUnreconciled)
    ));
    Ok(())
}

#[test]
fn rule9_typed_trust_matrix_and_configured_age_boundary() -> TestResult {
    for case in [
        "unknown",
        "incomplete",
        "paused",
        "stale",
        "trusted",
        "future",
    ] {
        let temp = tempfile::tempdir()?;
        let mut store = open(&temp.path().join("trust.db"))?;
        let key = identity()?;
        if case != "unknown" {
            let mut snapshot = page(1, "requested", false, 1000)?;
            snapshot.complete = case != "incomplete";
            snapshot.polled = if case == "future" {
                131
            } else if case == "paused" {
                120
            } else {
                100
            };
            store.order_page(&snapshot)?;
        }
        if case == "paused" {
            store.audit(&Audit {
                actor: Identifier::new("operator")?,
                action: Identifier::new("store-restore")?,
                targets: vec![],
                time: 101,
                reason: analyte_model::Reason::new("synthetic restore")?,
            })?;
        }
        let freshness = if case == "trusted" { 31 } else { 30 };
        let expected = match case {
            "unknown" | "future" => OrderTrust::Unknown,
            "incomplete" => OrderTrust::Incomplete,
            "paused" => OrderTrust::RestorePaused,
            "stale" => OrderTrust::Stale { age: 30 },
            "trusted" => OrderTrust::Trusted { age: 30 },
            _ => unreachable!(),
        };
        assert_eq!(
            store.order_trust(&key.destination, 130, freshness)?,
            expected
        );
        let lookup = store.orders(&key.destination, &SampleId::new("001a")?, 130, freshness)?;
        let pending = store.pending_orders(&key.destination, &key.machine, "", 130, freshness)?;
        assert_eq!(!matches!(lookup, Lookup::NotTrusted), expected.is_trusted());
        assert_eq!(
            !matches!(pending, PendingOrders::NotTrusted),
            expected.is_trusted()
        );
        assert_eq!(
            store
                .order_candidates(&key.destination, &key.machine, "", 130, freshness)
                .is_ok(),
            expected.is_trusted()
        );
        assert_eq!(
            store
                .cancel_candidates(&key.destination, &key.machine, 130, freshness)
                .is_ok(),
            expected.is_trusted()
        );
        assert_eq!(
            store.alert_value(
                &AlertKey::OrderCacheStale,
                130,
                &[(key.destination.clone(), freshness)]
            )?,
            if expected.is_trusted() {
                Some(30)
            } else {
                None
            }
        );
        assert_eq!(
            store
                .reserve_send(
                    &OrderReservation {
                        identity: &key,
                        version: NonZeroU64::MIN,
                        now: 130,
                        freshness
                    },
                    ReserveOptions::default()
                )?
                .is_ok(),
            expected.is_trusted()
        );
    }
    Ok(())
}

// ADR 0003 rules 4/8: caller grouping cannot omit, duplicate or invent cached tests.
