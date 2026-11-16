//! Tests allowed lifecycle transitions and retained states.
use super::*;

#[test]
fn law_tf2b_3_27_4_pending_pages_filter_targets_without_false_empty_and_never_replay() -> TestResult
{
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("pending.db"))?;
    let mut snapshot = page(1, "requested", false, 200)?;
    snapshot.orders.clear();
    for (id, state, target, expiry, tombstone) in [
        ("a", "requested", Some("other"), 200, false),
        ("b", "requested", Some("other"), 200, false),
        ("c", "requested", Some("machine"), 200, false),
        ("d", "requested", None, 200, false),
        ("e", "cancel_confirmed", None, 200, false),
        ("f", "requested", None, 101, false),
        ("g", "requested", None, 200, true),
        ("h", "requested", None, 200, false),
    ] {
        snapshot.orders.push(CachedOrder { id: Identifier::new(id)?, sample: SampleId::new("001a")?,
            document: serde_json::json!({"version":1,"state":state,"tombstone":tombstone,"target_analyzers":target.map(|value| vec![value]),"tests":[{"namespace":"local","value":"test"}]}).to_string(), tombstone, expiry });
    }
    store.order_page(&snapshot)?;
    let mut key = identity()?;
    key.order = Identifier::new("h")?;
    store
        .reserve_send(
            &OrderReservation {
                identity: &key,
                version: NonZeroU64::MIN,
                now: 101,
                freshness: 30,
            },
            ReserveOptions::default(),
        )?
        .map_err(std::io::Error::other)?;
    store.limits.page_rows = 2;
    let PendingOrders::Page { orders, after } =
        store.pending_orders(&key.destination, &key.machine, "", 101, 30)?
    else {
        return Err("trusted page refused".into());
    };
    assert!(orders.is_empty());
    let after = after.ok_or("filtered-empty must not terminate pagination")?;
    assert_eq!(after.as_str(), "b");
    let mut cursor = after.as_str().to_owned();
    let mut found = Vec::new();
    loop {
        let PendingOrders::Page { orders, after } =
            store.pending_orders(&key.destination, &key.machine, &cursor, 101, 30)?
        else {
            return Err("trusted page refused".into());
        };
        assert!(orders.len() <= 2);
        found.extend(orders.into_iter().map(|order| order.id.as_str().to_owned()));
        match after {
            Some(after) => cursor = after.as_str().to_owned(),
            None => break,
        }
    }
    assert_eq!(found, ["c", "d"]);
    Ok(())
}

#[test]
fn adr0004_reserve_transition_outbox_and_restart_never_resend() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("orders.db");
    let mut store = open(&path)?;
    store.order_page(&page(1, "requested", false, 200)?)?;
    let reserved = store
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
    assert_eq!(reserved.state, OrderState::Sending);
    assert_eq!(
        store
            .machine_order(&identity()?)?
            .ok_or("reservation lookup absent")?
            .id,
        reserved.id
    );
    let mut missing = identity()?;
    missing.machine = Identifier::new("absent")?;
    assert!(store.machine_order(&missing)?.is_none());
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM outbox", [], |r| r.get::<_, i64>(0))?,
        0
    );
    assert!(matches!(
        store.reserve_send(
            &OrderReservation {
                identity: &identity()?,
                version: NonZeroU64::MIN,
                now: 101,
                freshness: 30
            },
            ReserveOptions::default()
        )?,
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
    verify_adr0004_restart_and_reconciliation(&path, reserved.id)
}

#[test]
fn plan11_schema3_to6_preserves_rows_and_triggers() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("legacy.db");
    let mut db = Connection::open(&path)?;
    db.execute_batch(include_str!("../../../upkeep/schema.sql"))?;
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
        SCHEMA_VERSION
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

#[test]
fn adr0004_sending_reservations_survive_restart_and_plan11_listing_is_filtered_and_bounded()
-> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("orders.db");
    let mut store = open(&path)?;
    store.order_page(&page(1, "requested", false, 200)?)?;
    let first = store
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
    let mut second_key = identity()?;
    second_key.machine = Identifier::new("second-machine")?;
    let second = store
        .reserve_send(
            &OrderReservation {
                identity: &second_key,
                version: NonZeroU64::MIN,
                now: 101,
                freshness: 30,
            },
            ReserveOptions::default(),
        )?
        .map_err(std::io::Error::other)?;
    drop(store);
    let mut store = open(&path)?;
    assert!(matches!(
        store.reserve_send(
            &OrderReservation {
                identity: &identity()?,
                version: NonZeroU64::MIN,
                now: 102,
                freshness: 30
            },
            ReserveOptions::default()
        )?,
        Err(SendRefusal::AlreadyReserved)
    ));
    store.limits.page_rows = 1;
    let mut filter = MachineOrderFilter {
        after: 0,
        machine: None,
        state: None,
    };
    let rows = store.machine_orders(&filter)?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows.first().ok_or("first page absent")?.id, first.id);
    filter.after = first.id;
    assert_eq!(
        store
            .machine_orders(&filter)?
            .first()
            .ok_or("second page absent")?
            .id,
        second.id
    );
    filter.after = second.id;
    assert!(store.machine_orders(&filter)?.is_empty());
    filter.after = 0;
    filter.machine = Some(second_key.machine);
    assert_eq!(
        store
            .machine_orders(&filter)?
            .first()
            .ok_or("machine filter absent")?
            .id,
        second.id
    );
    filter.state = Some(OrderState::Accepted);
    assert!(store.machine_orders(&filter)?.is_empty());
    Ok(())
}

#[test]
fn adr0003_v4_migration_preserves_orders_and_flags_unrecoverable_coverage() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("legacy.db");
    let mut db = Connection::open(&path)?;
    db.execute_batch(include_str!("../../../upkeep/schema.sql"))?;
    Store::migrate_v2(&mut db)?;
    Store::migrate_v3(&mut db, 65_536)?;
    db.execute_batch(include_str!("../../../upkeep/migration_v4.sql"))?;
    db.execute("INSERT INTO machine_orders(destination,order_id,machine,version,state,created_at,updated_at,source_code) VALUES ('app','order','machine','1',?1,101,101,'reserved')",[document(&OrderState::Sending)?])?;
    db.execute("INSERT INTO machine_order_history(machine_order,evidence) VALUES (1,?1)",[serde_json::json!({"id":1,"key":{"destination":"app","order":"order","machine":"machine"},"version":1,"state":"Sending","created_at":101,"updated_at":101,"source_code":"reserved"}).to_string()])?;
    drop(db);
    let store = open(&path)?;
    let orders = store.machine_orders(&MachineOrderFilter {
        after: 0,
        machine: None,
        state: None,
    })?;
    let item = orders
        .first()
        .and_then(|order| order.items.first())
        .ok_or("missing migrated item")?;
    assert!(!item.coverage_known);
    assert!(item.tests.is_empty());
    assert_eq!(item.state, OrderState::Sending);
    assert_eq!(
        item.source_code.as_str(),
        "migration-v5-coverage-unavailable"
    );
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM machine_order_history", [], |r| r
                .get::<_, i64>(0))?,
        1
    );
    Ok(())
}

#[test]
fn adr0003_identity_overflow_duplicate_unknown_and_illegal_outcomes_roll_back() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("limits.db"))?;
    store.order_page(&page(1, "requested", false, 200)?)?;
    for namespace in ["n".repeat(22), "n".repeat(256)] {
        assert!(matches!(
            store.reserve_send(
                &OrderReservation {
                    identity: &identity()?,
                    version: NonZeroU64::MIN,
                    now: 101,
                    freshness: 30
                },
                ReserveOptions {
                    items: OrderItems::Groups {
                        groups: &test_groups(&["test"])?,
                        namespace: Some(&Identifier::new(&namespace)?),
                        max_identity_bytes: 22usize
                            .saturating_sub(namespace.len())
                            .saturating_sub(1)
                    },
                    ..Default::default()
                }
            ),
            Err(Error::ItemIdentityTooLong)
        ));
        assert_eq!(
            store
                .db
                .query_row("SELECT count(*) FROM machine_orders", [], |r| r
                    .get::<_, i64>(0))?,
            0
        );
    }
    let record = reserve_two(&mut store)?;
    for indices in [[1, 1], [1, 3]] {
        let outcomes = indices.map(|index| ItemOutcome {
            index,
            decision: ItemDecision::Accepted,
            code: SourceText::new("OK").expect("constant code"),
        });
        assert!(
            store
                .finish_order_items(record.id, &outcomes, 102, event)
                .is_err()
        );
    }
    assert!(
        store
            .finish_order_items(
                record.id,
                &[ItemOutcome {
                    index: 1,
                    decision: ItemDecision::CancelConfirmed,
                    code: SourceText::new("CR")?
                }],
                102,
                event
            )
            .is_err()
    );
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM machine_order_item_history", [], |r| r
                .get::<_, i64>(0))?,
        2
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
fn adr0003_caller_groups_are_exact_and_identity_budget_has_no_namespace_assumptions() -> TestResult
{
    let temp = tempfile::tempdir()?;
    let long_namespace = Identifier::new(&"n".repeat(256))?;
    for (case, namespace) in [None, Some(&long_namespace)].into_iter().enumerate() {
        let path = temp.path().join(format!("grouping-{case}.db"));
        let mut store = open(&path)?;
        let mut cached = page(1, "requested", false, 200)?;
        cached.orders.first_mut().ok_or("missing order")?.document = serde_json::json!({
            "version":1,"state":"requested","tombstone":false,
            "tests":[{"namespace":"local","value":"a"},{"namespace":"local","value":"b"},{"namespace":"local","value":"c"}]
        }).to_string();
        store.order_page(&cached)?;
        let key = identity()?;
        let candidate = OrderReservation {
            identity: &key,
            version: NonZeroU64::MIN,
            now: 101,
            freshness: 30,
        };
        let groups =
            verify_adr0003_invalid_groups_and_identity_budget(&mut store, &candidate, namespace)?;
        let order = store
            .reserve_send(
                &candidate,
                ReserveOptions {
                    items: OrderItems::Groups {
                        groups: &groups,
                        namespace,
                        max_identity_bytes: 3,
                    },
                    ..Default::default()
                },
            )?
            .map_err(std::io::Error::other)?;
        assert_eq!(
            order
                .items
                .iter()
                .map(|item| item.tests.clone())
                .collect::<Vec<_>>(),
            groups
        );
        assert!(
            order
                .items
                .iter()
                .all(|item| item.placer.len() == 3 && item.placer_namespace.as_ref() == namespace)
        );
        drop(store);
        let store = open(&path)?;
        assert_eq!(
            store
                .machine_order(&key)?
                .ok_or("missing reservation")?
                .items
                .iter()
                .map(|item| item.tests.clone())
                .collect::<Vec<_>>(),
            groups
        );
    }
    Ok(())
}

#[test]
fn law_tf2b_b7_6_original_command_is_owned_immutable_and_survives_restart() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("command.db");
    let mut store = open(&path)?;
    store.order_page(&page(1, "requested", false, 200)?)?;
    let owner = Identifier::new("owner")?;
    let key = identity()?;
    let groups = vec![vec![analyte_model::NamespacedId {
        namespace: Identifier::new("local")?,
        value: Identifier::new("test")?,
    }]];
    let namespace = Identifier::new("LAB")?;
    let record = store
        .reserve_send(
            &OrderReservation {
                identity: &key,
                version: NonZeroU64::MIN,
                now: 101,
                freshness: 30,
            },
            ReserveOptions {
                items: OrderItems::Groups {
                    groups: &groups,
                    namespace: Some(&namespace),
                    max_identity_bytes: 46,
                },
                owner: Some(&owner),
                prepare: None,
            },
        )?
        .map_err(std::io::Error::other)?;
    assert!(
        store
            .retain_order_command(&key, &Identifier::new("other")?, b"wrong-owner".to_vec())
            .is_err()
    );
    store.retain_order_command(&key, &owner, b"synthetic original command".to_vec())?;
    assert!(
        store
            .retain_order_command(&key, &owner, b"changed command".to_vec())
            .is_err()
    );
    let placer = record.items.first().ok_or("item")?.placer.clone();
    assert!(
        store
            .machine_order_by_placer(&Identifier::new("other-machine")?, &placer)?
            .is_none()
    );
    drop(store);
    let store = open(&path)?;
    let retained = store
        .machine_order_by_placer(&key.machine, &placer)?
        .ok_or("retained command")?;
    assert_eq!(
        retained.original_command(),
        Some(b"synthetic original command".as_slice())
    );
    Ok(())
}
