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
        .reserve_send(&key, NonZeroU64::MIN, 101, 30)?
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
fn plan11_schema3_to6_preserves_rows_and_triggers() -> TestResult {
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
        6
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
                .order_candidates(&identity()?.destination, &identity()?.machine, "", 101, 30)?
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

#[test]
fn plan11_rule2_restore_keeps_current_and_rule8_requires_audited_resume() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("store.db");
    let backup = temp.path().join("backup.db");
    let mut store = open(&path)?;
    store.order_page(&page(1, "requested", false, 200)?)?;
    store.backup(&backup, Instant::now() + Duration::from_secs(5))?;
    let order = store
        .reserve_send(&identity()?, NonZeroU64::MIN, 101, 30)?
        .map_err(std::io::Error::other)?;
    assert_eq!(order.state, OrderState::Sending);
    let mut audit = Audit {
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
            tests::limits(),
            Box::new(tests::Space(u64::MAX)),
            &audit,
            Instant::now() + Duration::from_secs(5)
        ),
        Err(Error::InUse)
    ));
    drop(store);
    let kept = Store::restore(
        &path,
        &backup,
        tests::limits(),
        Box::new(tests::Space(u64::MAX)),
        &audit,
        Instant::now() + Duration::from_secs(5),
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
        store.reserve_send(&identity()?, NonZeroU64::MIN, 103, 30)?,
        Err(SendRefusal::RestoreUnreconciled)
    ));
    assert!(matches!(
        store.orders(&Identifier::new("app")?, &SampleId::new("001a")?, 103, 30)?,
        Lookup::NotTrusted
    ));
    audit.action = Identifier::new("orders-resume-after-restore")?;
    store.resume_after_restore(&audit)?;
    assert!(!store.restore_unreconciled()?);
    assert_eq!(
        store
            .audits(0)?
            .last()
            .ok_or("missing audit")?
            .1
            .action
            .as_str(),
        "orders-resume-after-restore"
    );
    assert!(
        store
            .reserve_send(&identity()?, NonZeroU64::MIN, 103, 30)?
            .is_ok()
    );
    Ok(())
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
        store.reserve_send_items(
            &candidate,
            OrderItems::PerTest {
                namespace: &Identifier::new("lab")?,
                identity_bytes: 22
            }
        )?,
        Err(SendRefusal::RestoreUnreconciled)
    ));
    assert!(matches!(
        store.reserve_send_owned(&candidate, &Identifier::new("connection")?)?,
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
            .reserve_send_items(
                &candidate,
                OrderItems::PerTest {
                    namespace: &Identifier::new("lab")?,
                    identity_bytes: 22
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
        store.reserve_send_items_with(
            &candidate,
            OrderItems::PerTest {
                namespace: &namespace,
                identity_bytes: 22
            },
            &owner,
            |record| -> Result<(), Error> {
                assert_eq!(
                    record.items.first().ok_or(Error::InvalidInput)?.placer,
                    format!("{}-1", record.id)
                );
                Err(Error::InvalidInput)
            }
        ),
        Err(Error::InvalidInput)
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
        store.reserve_send_items_with(
            &candidate,
            OrderItems::PerTest {
                namespace: &namespace,
                identity_bytes: 22
            },
            &owner,
            |_| -> Result<(), Error> { Err(Error::InvalidInput) }
        )?,
        Err(SendRefusal::RestoreUnreconciled)
    ));
    Ok(())
}

#[test]
fn adr0003_0008_item_reasons_are_lossless_when_aggregate_summary_is_bounded() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("reasons.db"))?;
    let record = reserve_two(&mut store)?;
    let reason = SourceText::new(&format!("HL7-UA:{}", "r".repeat(9000)))?;
    let outcomes = [1, 2].map(|index| ItemOutcome {
        index,
        decision: ItemDecision::Rejected,
        code: reason.clone(),
    });
    let rejected = store.finish_order_items(record.id, &outcomes, 102, event)?;
    assert_eq!(rejected.state, OrderState::Rejected);
    assert!(rejected.source_code.as_str().len() < 200);
    assert!(rejected.items.iter().all(|item| item.source_code == reason));
    Ok(())
}

// ADR 0003 rule 8: an aggregate Uncertain state never authorizes rewriting a terminal item.
#[test]
fn adr0003_rule8_operator_reconciliation_checks_items_and_rolls_back() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("reconcile-items.db"))?;
    let record = reserve_two(&mut store)?;
    let outcomes = [ItemDecision::Uncertain, ItemDecision::Rejected]
        .into_iter()
        .zip([1, 2])
        .map(|(decision, index)| {
            Ok(ItemOutcome {
                index,
                decision,
                code: SourceText::new("peer-evidence")?,
            })
        })
        .collect::<Result<Vec<_>, analyte_model::TextError>>()?;
    let uncertain = store.finish_order_items(record.id, &outcomes, 102, event)?;
    assert_eq!(uncertain.state, OrderState::Uncertain);
    let history: i64 =
        store
            .db
            .query_row("SELECT count(*) FROM machine_order_item_history", [], |r| {
                r.get(0)
            })?;
    let audit_count: i64 = store
        .db
        .query_row("SELECT count(*) FROM audit", [], |r| r.get(0))?;
    let audit = Audit {
        actor: Identifier::new("operator")?,
        action: Identifier::new("orders-reconcile")?,
        targets: vec![Identifier::new(&record.id.to_string())?],
        time: 103,
        reason: analyte_model::Reason::new("synthetic operator evidence")?,
    };
    assert!(matches!(
        store.reconcile_order(record.id, OrderState::Accepted, &audit, event),
        Err(Error::OrderTransition(analyte_model::TransitionError {
            from: OrderState::Rejected,
            to: OrderState::Accepted
        }))
    ));
    let retained = store.machine_order(&record.key)?.ok_or("missing order")?;
    assert_eq!(
        serde_json::to_value(&retained)?,
        serde_json::to_value(&uncertain)?
    );
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM machine_order_item_history", [], |r| r
                .get::<_, i64>(0))?,
        history
    );
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM audit", [], |r| r.get::<_, i64>(0))?,
        audit_count
    );
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM outbox", [], |r| r.get::<_, i64>(0))?,
        1
    );
    Ok(())
}

// ADR 0003 rule 8 / ORDERS.md: refusal restores retained item state, not a past aggregate snapshot.
#[test]
fn adr0003_rule8_cancel_refusal_uses_cancel_from_not_history_json() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("cancel-from.db"))?;
    let record = reserve_two(&mut store)?;
    let outcomes = [1, 2].map(|index| ItemOutcome {
        index,
        decision: ItemDecision::Sent,
        code: SourceText::new("AA").expect("constant"),
    });
    store.finish_order_items(record.id, &outcomes, 102, event)?;
    let cancelled = store.transition_order(
        record.id,
        OrderState::CancelRequested,
        SourceText::new("CA")?,
        103,
        event,
    )?;
    assert!(
        cancelled
            .items
            .iter()
            .all(|item| item.cancel_from == Some(OrderState::Sent))
    );
    // Deliberately conflicting historical aggregate: the current per-item evidence must win.
    let mut past = cancelled.clone();
    past.state = OrderState::Accepted;
    for snapshot in [&past, &cancelled] {
        store.db.execute(
            "INSERT INTO machine_order_history(machine_order,evidence) VALUES (?1,?2)",
            rusqlite::params![record.id, serde_json::to_string(snapshot)?],
        )?;
    }
    let outcomes = [1, 2].map(|index| ItemOutcome {
        index,
        decision: ItemDecision::CancelRefused,
        code: SourceText::new("UC").expect("constant"),
    });
    let restored = store.finish_order_items(record.id, &outcomes, 104, event)?;
    assert_eq!(restored.state, OrderState::Sent);
    assert!(
        restored
            .items
            .iter()
            .all(|item| item.state == OrderState::Sent)
    );
    Ok(())
}

// ADR 0004 / safety rule 9: all order readers, reservations and alerts share one trust decision.
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
                .reserve_send(&key, NonZeroU64::MIN, 130, freshness)?
                .is_ok(),
            expected.is_trusted()
        );
    }
    Ok(())
}
