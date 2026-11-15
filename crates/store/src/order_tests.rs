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
            document: serde_json::json!({"version":version,"state":state,"tombstone":tombstone,"tests":[{"namespace":"local","value":"test"}]})
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
        .reserve_send(&identity()?, NonZeroU64::MIN, 101, 30)?
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
        store.reserve_send(&identity()?, NonZeroU64::MIN, 101, 30)?,
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
        store.reserve_send(&identity()?, version, 103, 30)?,
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
        2
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
            matches!(store.reserve_send(&identity()?,NonZeroU64::MIN,now,30)?,Err(reason) if reason == expected)
        );
    }
    store.order_page(&page(1, "requested", false, 200)?)?;
    assert!(
        store
            .reject_unsent(
                &OrderReservation {
                    identity: &identity()?,
                    version: NonZeroU64::MIN,
                    now: 101,
                    freshness: 30
                },
                SourceText::new("mapping rejected")?,
                |_| Err(Error::InvalidInput)
            )
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
fn plan11_schema3_to5_preserves_rows_and_triggers() -> TestResult {
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
        5
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
        .reserve_send(&identity()?, NonZeroU64::MIN, 101, 30)?
        .map_err(std::io::Error::other)?;
    let mut second_key = identity()?;
    second_key.machine = Identifier::new("second-machine")?;
    let second = store
        .reserve_send(&second_key, NonZeroU64::MIN, 101, 30)?
        .map_err(std::io::Error::other)?;
    drop(store);
    let mut store = open(&path)?;
    assert!(matches!(
        store.reserve_send(&identity()?, NonZeroU64::MIN, 102, 30)?,
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

fn reserve_two(store: &mut Store) -> Result<MachineOrder, Box<dyn std::error::Error>> {
    let mut page = page(1, "requested", false, 200)?;
    page.orders.first_mut().ok_or("missing fixture")?.document = serde_json::json!({"version":1,"state":"requested","tombstone":false,"tests":[{"namespace":"local","value":"a"},{"namespace":"local","value":"b"}]}).to_string();
    store.order_page(&page)?;
    Ok(store
        .reserve_send_items(
            &OrderReservation {
                identity: &identity()?,
                version: NonZeroU64::MIN,
                now: 101,
                freshness: 30,
            },
            OrderItems::PerTest {
                namespace: &Identifier::new("lab")?,
                identity_bytes: 22,
            },
        )?
        .map_err(std::io::Error::other)?)
}

#[test]
fn adr0003_0004_items_are_atomic_retained_and_never_reserved_again() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("items.db");
    let mut store = open(&path)?;
    let record = reserve_two(&mut store)?;
    assert_eq!(record.items.len(), 2);
    for item in &record.items {
        assert_eq!(item.placer, format!("{}-{}", record.id, item.index));
        assert_eq!(item.tests.len(), 1);
        assert_eq!(
            item.placer_namespace.as_ref().map(Identifier::as_str),
            Some("lab")
        );
        assert!(item.coverage_known);
        assert_eq!(item.state, OrderState::Sending);
    }
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM outbox", [], |r| r.get::<_, i64>(0))?,
        0
    );
    for sql in [
        "DELETE FROM machine_order_items",
        "UPDATE machine_order_items SET placer='collision'",
        "UPDATE machine_order_items SET placer_namespace='changed'",
        "UPDATE machine_order_items SET tests='[]'",
        "DELETE FROM machine_order_item_history",
        "UPDATE machine_order_item_history SET evidence='{}'",
    ] {
        assert!(store.db.execute(sql, []).is_err());
    }
    drop(store);
    let mut store = open(&path)?;
    let restored = store.machine_orders(&MachineOrderFilter {
        after: 0,
        machine: None,
        state: None,
    })?;
    assert_eq!(restored.first().ok_or("missing order")?.items.len(), 2);
    assert!(matches!(
        store.reserve_send(&identity()?, NonZeroU64::MIN, 102, 30)?,
        Err(SendRefusal::AlreadyReserved)
    ));
    store.transition_order(
        record.id,
        OrderState::Uncertain,
        SourceText::new("host-restart")?,
        102,
        event,
    )?;
    assert!(
        store
            .machine_orders(&MachineOrderFilter {
                after: 0,
                machine: None,
                state: None
            })?
            .first()
            .ok_or("missing order")?
            .items
            .iter()
            .all(|item| item.state == OrderState::Uncertain)
    );
    Ok(())
}

#[test]
fn adr0008_every_item_aggregation_row_is_atomic_and_emits_one_event() -> TestResult {
    use ItemDecision::{Accepted as A, Rejected as R, Sent as S, Uncertain as U};
    for (decisions, expected) in [
        ([A, A], OrderState::Accepted),
        ([R, R], OrderState::Rejected),
        ([S, S], OrderState::Sent),
        ([A, R], OrderState::Uncertain),
        ([A, U], OrderState::Uncertain),
        ([S, A], OrderState::Uncertain),
        ([U, U], OrderState::Uncertain),
    ] {
        let temp = tempfile::tempdir()?;
        let mut store = open(&temp.path().join("items.db"))?;
        let record = reserve_two(&mut store)?;
        let outcomes = decisions
            .into_iter()
            .zip([1, 2])
            .map(|(decision, index)| {
                Ok(ItemOutcome {
                    index,
                    decision,
                    code: SourceText::new("synthetic-peer-evidence")?,
                })
            })
            .collect::<Result<Vec<_>, analyte_model::TextError>>()?;
        assert!(
            store
                .finish_order_items(record.id, &outcomes, 102, |_| Err(Error::InvalidInput))
                .is_err()
        );
        assert_eq!(
            store
                .db
                .query_row("SELECT count(*) FROM machine_order_item_history", [], |r| r
                    .get::<_, i64>(0))?,
            2
        );
        let result = store.finish_order_items(record.id, &outcomes, 102, event)?;
        assert_eq!(result.state, expected);
        assert_eq!(
            store
                .db
                .query_row("SELECT count(*) FROM outbox", [], |r| r.get::<_, i64>(0))?,
            1
        );
    }
    Ok(())
}

#[test]
fn adr0004_missing_answers_and_cancel_all_confirm_or_refuse() -> TestResult {
    for refused in [false, true] {
        let temp = tempfile::tempdir()?;
        let mut store = open(&temp.path().join("cancel.db"))?;
        let record = reserve_two(&mut store)?;
        let outcomes = [1, 2].map(|index| ItemOutcome {
            index,
            decision: ItemDecision::Accepted,
            code: SourceText::new("OK").expect("constant code"),
        });
        store.finish_order_items(record.id, &outcomes, 102, event)?;
        store.transition_order(
            record.id,
            OrderState::CancelRequested,
            SourceText::new("CA")?,
            103,
            event,
        )?;
        let outcomes = [1, 2].map(|index| ItemOutcome {
            index,
            decision: if refused && index == 1 {
                ItemDecision::CancelRefused
            } else {
                ItemDecision::CancelConfirmed
            },
            code: SourceText::new(if refused && index == 1 { "UC" } else { "CR" })
                .expect("constant code"),
        });
        let result = store.finish_order_items(record.id, &outcomes, 104, event)?;
        assert_eq!(
            result.state,
            if refused {
                OrderState::Accepted
            } else {
                OrderState::CancelConfirmed
            }
        );
        assert_eq!(
            result.source_code.as_str().contains("CancelRefused"),
            refused
        );
        assert_eq!(
            store
                .db
                .query_row("SELECT count(*) FROM outbox", [], |r| r.get::<_, i64>(0))?,
            3
        );
    }
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("missing.db"))?;
    let record = reserve_two(&mut store)?;
    let result = store.finish_order_items(
        record.id,
        &[ItemOutcome {
            index: 1,
            decision: ItemDecision::Accepted,
            code: SourceText::new("OK")?,
        }],
        102,
        event,
    )?;
    assert_eq!(result.state, OrderState::Uncertain);
    assert!(
        result
            .source_code
            .as_str()
            .contains("exchange-answer-missing")
    );
    Ok(())
}

#[test]
fn adr0003_v4_migration_preserves_orders_and_flags_unrecoverable_coverage() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("legacy.db");
    let mut db = Connection::open(&path)?;
    db.execute_batch(include_str!("schema.sql"))?;
    Store::migrate_v2(&mut db)?;
    Store::migrate_v3(&mut db, 65_536)?;
    db.execute_batch(include_str!("migration_v4.sql"))?;
    db.execute("INSERT INTO machine_orders(destination,order_id,machine,version,state,created_at,updated_at,source_code) VALUES ('app','order','machine','1',?1,101,101,'reserved')",[key(&OrderState::Sending)?])?;
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
            store.reserve_send_items(
                &OrderReservation {
                    identity: &identity()?,
                    version: NonZeroU64::MIN,
                    now: 101,
                    freshness: 30
                },
                OrderItems::PerTest {
                    namespace: &Identifier::new(&namespace)?,
                    identity_bytes: 22
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
fn adr0004_cancel_refusal_restores_sent_and_missing_cancel_is_uncertain() -> TestResult {
    for missing in [false, true] {
        let temp = tempfile::tempdir()?;
        let mut store = open(&temp.path().join("cancel-sent.db"))?;
        let record = reserve_two(&mut store)?;
        let outcomes = [1, 2].map(|index| ItemOutcome {
            index,
            decision: ItemDecision::Sent,
            code: SourceText::new("AA").expect("constant code"),
        });
        store.finish_order_items(record.id, &outcomes, 102, event)?;
        store.transition_order(
            record.id,
            OrderState::CancelRequested,
            SourceText::new("CA")?,
            103,
            event,
        )?;
        let mut outcomes = vec![ItemOutcome {
            index: 1,
            decision: ItemDecision::CancelRefused,
            code: SourceText::new("UC")?,
        }];
        if !missing {
            outcomes.push(ItemOutcome {
                index: 2,
                decision: ItemDecision::CancelConfirmed,
                code: SourceText::new("CR")?,
            });
        }
        let result = store.finish_order_items(record.id, &outcomes, 104, event)?;
        assert_eq!(
            result.state,
            if missing {
                OrderState::Uncertain
            } else {
                OrderState::Sent
            }
        );
        assert!(result.source_code.as_str().contains("CancelRefused"));
        assert!(
            store
                .order_candidates(&identity()?.destination, &identity()?.machine, "")?
                .is_empty()
        );
    }
    Ok(())
}

#[test]
fn adr0003_cancelled_query_wait_can_only_claim_its_retained_reservation_owner() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("owned.db");
    let mut store = open(&path)?;
    store.order_page(&page(1, "requested", false, 200)?)?;
    let candidate = identity()?;
    let reservation = OrderReservation {
        identity: &candidate,
        version: NonZeroU64::MIN,
        now: 101,
        freshness: 30,
    };
    let first = Identifier::new("connection-one")?;
    let second = Identifier::new("connection-two")?;
    let record = store
        .reserve_send_owned(&reservation, &first)?
        .map_err(std::io::Error::other)?;
    assert_eq!(record.source_code.as_str(), "host-reserved:connection-one");
    assert!(matches!(
        store.reserve_send_owned(&reservation, &second)?,
        Err(SendRefusal::AlreadyReserved)
    ));
    drop(store);
    let store = open(&path)?;
    let restored = store
        .machine_order(&candidate)?
        .ok_or("reservation absent")?;
    assert_eq!(
        restored.source_code.as_str(),
        "host-reserved:connection-one"
    );
    assert_eq!(restored.state, OrderState::Sending);
    assert_eq!(restored.items.len(), 1);
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM outbox", [], |r| r.get::<_, i64>(0))?,
        0
    );
    Ok(())
}
