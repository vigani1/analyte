//! Tests rejection of malformed inputs without losing evidence.
use super::*;

// ADR 0003 rule 8; mutation-testing regression
#[test]
fn order_clock_allows_equal_time_refuses_regression() -> TestResult {
    for now in [100, 101, 102] {
        let temp = tempfile::tempdir()?;
        let mut store = open(&temp.path().join("order-clock.db"))?;
        store.order_page(&page(1, "requested", false, 200)?)?;
        let key = identity()?;
        let record = store
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
        let result = store.transition_order(
            record.id,
            OrderState::Sent,
            SourceText::new("peer-sent")?,
            now,
            event,
        );
        if now == 100 {
            assert!(matches!(result, Err(OrderError::Conflict)));
            let retained = store.machine_order(&key)?.ok_or("missing reservation")?;
            assert_eq!(retained.state, OrderState::Sending);
            assert_eq!(retained.updated_at, 101);
            assert_eq!(
                store
                    .db
                    .query_row("SELECT count(*) FROM machine_order_history", [], |r| r
                        .get::<_, i64>(0))?,
                1
            );
            assert_eq!(
                store
                    .db
                    .query_row("SELECT count(*) FROM outbox", [], |r| r.get::<_, i64>(0))?,
                0
            );
        } else {
            let sent = result?;
            assert_eq!(sent.state, OrderState::Sent);
            assert_eq!(sent.updated_at, now);
            assert_eq!(
                sent.items.first().ok_or("missing item")?.state,
                OrderState::Sent
            );
            assert_eq!(
                store
                    .db
                    .query_row("SELECT count(*) FROM outbox", [], |r| r.get::<_, i64>(0))?,
                1
            );
        }
        assert!(matches!(
            store.reserve_send(
                &OrderReservation {
                    identity: &key,
                    version: NonZeroU64::MIN,
                    now: 102,
                    freshness: 30
                },
                ReserveOptions::default()
            )?,
            Err(SendRefusal::AlreadyReserved)
        ));
    }
    Ok(())
}

// ADR 0003 rule 8; mutation-testing regression
#[test]
fn reconciliation_checks_target_and_outcome_separately() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("order-operator.db"))?;
    store.order_page(&page(1, "requested", false, 200)?)?;
    let key = identity()?;
    let record = store
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
    store.transition_order(
        record.id,
        OrderState::Uncertain,
        SourceText::new("lost reply")?,
        102,
        event,
    )?;
    for (outcome, target) in [
        (OrderState::Accepted, "unrelated".to_owned()),
        (OrderState::Sent, record.id.to_string()),
    ] {
        let audit = Audit {
            actor: Identifier::new("operator")?,
            action: Identifier::new("orders-reconcile")?,
            targets: vec![Identifier::new(&target)?],
            time: 103,
            reason: analyte_model::Reason::new("synthetic evidence")?,
        };
        assert!(matches!(
            store.reconcile_order(record.id, outcome, &audit, event),
            Err(OrderError::InvalidOrderInput)
        ));
        assert_eq!(
            store.machine_order(&key)?.ok_or("missing order")?.state,
            OrderState::Uncertain
        );
        assert!(store.audits(0)?.is_empty());
        assert_eq!(
            store
                .db
                .query_row("SELECT count(*) FROM machine_order_history", [], |r| r
                    .get::<_, i64>(0))?,
            2
        );
    }
    let audit = Audit {
        actor: Identifier::new("operator")?,
        action: Identifier::new("orders-reconcile")?,
        targets: vec![Identifier::new(&record.id.to_string())?],
        time: 103,
        reason: analyte_model::Reason::new("synthetic acceptance")?,
    };
    assert_eq!(
        store
            .reconcile_order(record.id, OrderState::Accepted, &audit, event)?
            .state,
        OrderState::Accepted
    );
    assert_eq!(store.audits(0)?.len(), 1);
    Ok(())
}

// ADR 0003 rule 8; mutation-testing regression
#[test]
fn pending_page_takes_exact_limit_skips_reservations() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("pending-bound.db"))?;
    store.limits.page_rows = 1;
    store.order_page(&page(1, "requested", false, 200)?)?;
    let key = identity()?;
    let PendingOrders::Page { orders, after } =
        store.pending_orders(&key.destination, &key.machine, "", 101, 30)?
    else {
        return Err("trusted cache unavailable".into());
    };
    assert_eq!(orders.len(), 1);
    assert_eq!(orders.first().ok_or("missing pending order")?.id, key.order);
    assert!(after.is_none());
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
    let PendingOrders::Page { orders, after } =
        store.pending_orders(&key.destination, &key.machine, "", 102, 30)?
    else {
        return Err("trusted cache unavailable".into());
    };
    assert!(orders.is_empty());
    assert!(after.is_none());
    assert!(
        store
            .order_candidates(&key.destination, &key.machine, "", 102, 30)?
            .is_empty()
    );
    Ok(())
}

// ADR 0003 rule 8; mutation-testing regression
#[test]
fn history_and_audit_bounds_are_inclusive() -> TestResult {
    for (boundary, delta) in [("history", 0), ("history", 1), ("audit", 0), ("audit", 1)] {
        let temp = tempfile::tempdir()?;
        let mut store = open(&temp.path().join("order-evidence-bounds.db"))?;
        store.order_page(&page(1, "requested", false, 200)?)?;
        let key = identity()?;
        let record = store
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
        let build = |order: &MachineOrder| -> Result<OrderEventDelivery, OrderError> {
            let mut value = event(order)?;
            value.payload = b"small event".to_vec();
            Ok(value)
        };
        if boundary == "history" {
            let mut expected = record.clone();
            expected.state = OrderState::Sent;
            expected.updated_at = 102;
            expected.source_code = SourceText::new("sent")?;
            for item in &mut expected.items {
                item.state = OrderState::Sent;
                item.updated_at = 102;
                item.source_code = SourceText::new("sent")?;
            }
            store.limits.record_bytes = serde_json::to_vec(&expected)?
                .len()
                .checked_sub(delta)
                .ok_or("bound")?;
            let result = store.transition_order(
                record.id,
                OrderState::Sent,
                SourceText::new("sent")?,
                102,
                build,
            );
            if delta == 0 {
                assert_eq!(result?.state, OrderState::Sent);
            } else {
                assert!(matches!(
                    result,
                    Err(OrderError::Storage(StorageError::LimitExceeded {
                        what: "record_bytes"
                    }))
                ));
                assert_eq!(
                    store.machine_order(&key)?.ok_or("missing order")?.state,
                    OrderState::Sending
                );
            }
        } else {
            store.transition_order(
                record.id,
                OrderState::Uncertain,
                SourceText::new("timeout")?,
                102,
                event,
            )?;
            let audit = Audit {
                actor: Identifier::new("operator")?,
                action: Identifier::new("orders-reconcile")?,
                targets: vec![Identifier::new(&record.id.to_string())?],
                time: 103,
                reason: analyte_model::Reason::new(&"r".repeat(1024))?,
            };
            store.limits.record_bytes = serde_json::to_vec(&audit)?
                .len()
                .checked_sub(delta)
                .ok_or("bound")?;
            let result = store.reconcile_order(record.id, OrderState::Accepted, &audit, build);
            if delta == 0 {
                assert_eq!(result?.state, OrderState::Accepted);
                assert_eq!(store.audits(0)?.len(), 1);
            } else {
                assert!(matches!(
                    result,
                    Err(OrderError::Storage(StorageError::LimitExceeded {
                        what: "record_bytes"
                    }))
                ));
                assert_eq!(
                    store.machine_order(&key)?.ok_or("missing order")?.state,
                    OrderState::Uncertain
                );
                assert!(store.audits(0)?.is_empty());
            }
        }
    }
    Ok(())
}

// ADR 0003 rule 8; mutation-testing regression
#[test]
fn order_event_checks_bounds_and_provenance_atomically() -> TestResult {
    let app = Identifier::new("app")?;
    for case in [
        "valid",
        "large-one",
        "large-two",
        "destination",
        "order",
        "version",
    ] {
        let temp = tempfile::tempdir()?;
        let mut store = open(&temp.path().join("order-event-fields.db"))?;
        store.order_page(&page(1, "requested", false, 200)?)?;
        let key = identity()?;
        let record = store
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
        store.limits.record_bytes = 2048;
        let result = store.transition_order(
            record.id,
            OrderState::Sent,
            SourceText::new("sent")?,
            102,
            |order| {
                let mut value = event(order)?;
                value.payload = vec![
                    b'x';
                    match case {
                        "valid" => 2048,
                        "large-one" => 2049,
                        "large-two" => 2050,
                        _ => 1,
                    }
                ];
                match case {
                    "destination" => {
                        value.destination = Identifier::new("wrong")
                            .map_err(|cause| OrderError::Storage(StorageError::Text(cause)))?;
                    }
                    "order" => {
                        value.order_id = Identifier::new("wrong")
                            .map_err(|cause| OrderError::Storage(StorageError::Text(cause)))?;
                    }
                    "version" => value.contract_version = 2,
                    "valid" | "large-one" | "large-two" => {}
                    _ => unreachable!("fixed cases"),
                }
                Ok(value)
            },
        );
        if case == "valid" {
            assert_eq!(result?.state, OrderState::Sent);
            assert_eq!(
                store
                    .claim_destination(102, &app)?
                    .ok_or("missing event")?
                    .payload,
                vec![b'x'; 2048]
            );
        } else {
            assert!(
                matches!(result, Err(OrderError::InvalidOrderInput)),
                "case {case}"
            );
            let retained = store.machine_order(&key)?.ok_or("missing order")?;
            assert_eq!(retained.state, OrderState::Sending);
            assert_eq!(
                retained.items.first().ok_or("missing item")?.state,
                OrderState::Sending
            );
            assert_eq!(
                store
                    .db
                    .query_row("SELECT count(*) FROM machine_order_history", [], |r| r
                        .get::<_, i64>(0))?,
                1
            );
            assert!(store.claim_destination(102, &app)?.is_none());
        }
    }
    Ok(())
}

// ADR 0003 rule 8; mutation-testing regression
#[test]
fn command_refuses_empty_bytes_keeps_original_after_restart() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("original-command.db");
    let mut store = open(&path)?;
    store.order_page(&page(1, "requested", false, 200)?)?;
    let key = identity()?;
    let owner = Identifier::new("connection")?;
    store
        .reserve_send(
            &OrderReservation {
                identity: &key,
                version: NonZeroU64::MIN,
                now: 101,
                freshness: 30,
            },
            ReserveOptions {
                owner: Some(&owner),
                ..Default::default()
            },
        )?
        .map_err(std::io::Error::other)?;
    assert!(matches!(
        store.retain_order_command(&key, &owner, vec![]),
        Err(OrderError::InvalidOrderInput)
    ));
    assert!(
        store
            .machine_order(&key)?
            .ok_or("missing order")?
            .original_command()
            .is_none()
    );
    store.retain_order_command(&key, &owner, b"exact command\0bytes".to_vec())?;
    assert!(matches!(
        store.retain_order_command(&key, &owner, b"different".to_vec()),
        Err(OrderError::Conflict)
    ));
    drop(store);
    let mut store = open(&path)?;
    assert_eq!(
        store
            .machine_order(&key)?
            .ok_or("missing order")?
            .original_command(),
        Some(b"exact command\0bytes".as_slice())
    );
    assert!(matches!(
        store.reserve_send(
            &OrderReservation {
                identity: &key,
                version: NonZeroU64::MIN,
                now: 102,
                freshness: 30
            },
            ReserveOptions::default()
        )?,
        Err(SendRefusal::AlreadyReserved)
    ));
    Ok(())
}
