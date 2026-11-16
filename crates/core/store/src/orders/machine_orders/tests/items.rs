//! Tests individual machine order outcomes.
use super::*;

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
fn mutation_adr0003_rule8_order_item_limits_keep_exact_coverage_and_accept_the_host_ceiling()
-> TestResult {
    for (count, shape, succeeds) in [
        (0, "together", false),
        (100, "together", true),
        (101, "together", false),
        (100, "separate", true),
        (100, "single-group", true),
        (1, "empty-groups", false),
        (1, "empty-member", false),
        (1, "over-groups", false),
    ] {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("order-item-bounds.db");
        let mut store = open(&path)?;
        let (tests, groups) = seed_mutation_rule8_item_groups(&mut store, count, shape)?;
        let key = identity()?;
        let options = ReserveOptions {
            items: if shape == "together" {
                OrderItems::Together
            } else {
                OrderItems::Groups {
                    groups: &groups,
                    namespace: None,
                    max_identity_bytes: 64,
                }
            },
            ..Default::default()
        };
        let result = store.reserve_send(
            &OrderReservation {
                identity: &key,
                version: NonZeroU64::MIN,
                now: 101,
                freshness: 30,
            },
            options,
        );
        if succeeds {
            let record = result?.map_err(std::io::Error::other)?;
            assert_eq!(
                record
                    .items
                    .iter()
                    .map(|item| item.tests.len())
                    .sum::<usize>(),
                count
            );
            assert_eq!(
                record.items.len(),
                if shape == "separate" { 100 } else { 1 }
            );
            drop(store);
            let mut store = open(&path)?;
            let retained = store.machine_order(&key)?.ok_or("lost reservation")?;
            assert_eq!(retained.items.len(), record.items.len());
            assert_eq!(
                retained
                    .items
                    .iter()
                    .flat_map(|item| item.tests.iter())
                    .collect::<Vec<_>>(),
                tests.iter().collect::<Vec<_>>()
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
        } else {
            assert!(
                matches!(result, Err(Error::InvalidInput)),
                "case {count}/{shape}"
            );
            assert!(store.machine_order(&key)?.is_none());
            for table in [
                "machine_order_history",
                "machine_order_items",
                "machine_order_item_history",
                "outbox",
            ] {
                assert_eq!(
                    store
                        .db
                        .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                            .get::<_, i64>(0))?,
                    0
                );
            }
        }
    }
    Ok(())
}

#[test]
fn mutation_adr0003_rule8_order_item_completion_has_its_own_clock_and_exchange_state() -> TestResult
{
    for now in [100, 101, 102] {
        let temp = tempfile::tempdir()?;
        let mut store = open(&temp.path().join("item-exchange-clock.db"))?;
        let record = reserve_two(&mut store)?;
        assert!(matches!(
            store.transition_order(
                record.id,
                OrderState::Sent,
                SourceText::new("whole-order-send")?,
                102,
                event
            ),
            Err(Error::Conflict)
        ));
        let outcomes = [1, 2]
            .into_iter()
            .map(|index| {
                Ok(ItemOutcome {
                    index,
                    decision: ItemDecision::Accepted,
                    code: SourceText::new("accepted")?,
                })
            })
            .collect::<Result<Vec<_>, analyte_model::TextError>>()?;
        let result = store.finish_order_items(record.id, &outcomes, now, event);
        if now == 100 {
            assert!(matches!(result, Err(Error::InvalidInput)));
            assert_eq!(
                store
                    .machine_order(&record.key)?
                    .ok_or("missing order")?
                    .state,
                OrderState::Sending
            );
            assert_eq!(
                store
                    .db
                    .query_row("SELECT count(*) FROM outbox", [], |r| r.get::<_, i64>(0))?,
                0
            );
        } else {
            let accepted = result?;
            assert_eq!(accepted.state, OrderState::Accepted);
            assert!(
                accepted
                    .items
                    .iter()
                    .all(|item| item.state == OrderState::Accepted && item.updated_at == now)
            );
            let late = [ItemOutcome {
                index: 1,
                decision: ItemDecision::Rejected,
                code: SourceText::new("late reply")?,
            }];
            assert!(matches!(
                store.finish_order_items(record.id, &late, 103, event),
                Err(Error::InvalidInput)
            ));
            assert_eq!(
                store
                    .machine_order(&record.key)?
                    .ok_or("missing order")?
                    .state,
                OrderState::Accepted
            );
            assert_eq!(
                store
                    .db
                    .query_row("SELECT count(*) FROM outbox", [], |r| r.get::<_, i64>(0))?,
                1
            );
        }
    }
    Ok(())
}

#[test]
fn mutation_adr0003_rule8_order_item_outcomes_remain_bound_to_their_exact_index() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("item-index.db");
    let mut store = open(&path)?;
    let record = reserve_two(&mut store)?;
    for indices in [[1, 1, 2], [1, 2, 3]] {
        let outcomes = indices
            .into_iter()
            .map(|index| {
                Ok(ItemOutcome {
                    index,
                    decision: ItemDecision::Accepted,
                    code: SourceText::new("invalid coverage")?,
                })
            })
            .collect::<Result<Vec<_>, analyte_model::TextError>>()?;
        assert!(matches!(
            store.finish_order_items(record.id, &outcomes, 102, event),
            Err(Error::InvalidInput)
        ));
        assert_eq!(
            store
                .machine_order(&record.key)?
                .ok_or("missing order")?
                .state,
            OrderState::Sending
        );
        assert_eq!(
            store
                .db
                .query_row("SELECT count(*) FROM machine_order_item_history", [], |r| r
                    .get::<_, i64>(0))?,
            2
        );
    }
    let outcomes = [
        ItemOutcome {
            index: 1,
            decision: ItemDecision::Accepted,
            code: SourceText::new("accept-one")?,
        },
        ItemOutcome {
            index: 2,
            decision: ItemDecision::Rejected,
            code: SourceText::new("reject-two")?,
        },
    ];
    let completed = store.finish_order_items(record.id, &outcomes, 102, event)?;
    assert_eq!(completed.state, OrderState::Uncertain);
    drop(store);
    let mut store = open(&path)?;
    let retained = store.machine_order(&record.key)?.ok_or("missing order")?;
    let actual = retained
        .items
        .iter()
        .map(|item| (item.index, item.state, item.source_code.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(
        actual,
        vec![
            (1, OrderState::Accepted, "accept-one"),
            (2, OrderState::Rejected, "reject-two")
        ]
    );
    let claim = store.claim(102)?.ok_or("missing outcome event")?;
    let delivered: MachineOrder = serde_json::from_slice(&claim.payload)?;
    assert_eq!(
        delivered
            .items
            .iter()
            .map(|item| (item.index, item.state, item.source_code.as_str()))
            .collect::<Vec<_>>(),
        actual
    );
    assert!(matches!(
        store.reserve_send(
            &OrderReservation {
                identity: &record.key,
                version: NonZeroU64::MIN,
                now: 103,
                freshness: 30
            },
            ReserveOptions::default()
        )?,
        Err(SendRefusal::AlreadyReserved)
    ));
    Ok(())
}

#[test]
fn adr0016_adr0003_v12_migration_keeps_unknown_execution_and_command_evidence() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("execution-migration.db");
    let mut store = open(&path)?;
    let record = reserve_two(&mut store)?;
    let before: String = store.db.query_row(
        "SELECT evidence FROM machine_order_history ORDER BY sequence DESC LIMIT 1",
        [],
        |r| r.get(0),
    )?;
    store
        .db
        .execute_batch("DROP TABLE law_work_history; DROP TABLE law_messages; DROP TABLE awos_execution_history; PRAGMA user_version=12;")?;
    drop(store);
    let store = open(&path)?;
    assert_eq!(
        store
            .db
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))?,
        SCHEMA_VERSION
    );
    let restored = store.machine_order(&identity()?)?.ok_or("missing")?;
    assert_eq!(restored.id, record.id);
    assert!(restored.items.iter().all(|i| i.execution.is_none()));
    assert!(
        restored
            .items
            .iter()
            .all(|i| i.state == OrderState::Sending)
    );
    let after: String = store.db.query_row(
        "SELECT evidence FROM machine_order_history ORDER BY sequence DESC LIMIT 1",
        [],
        |r| r.get(0),
    )?;
    assert_eq!(before, after);
    Ok(())
}

#[test]
fn palm_tf2b_c8_adr0003_0011_projection_restart_keeps_command_state_and_no_results() -> TestResult {
    use analyte_model::AwosExecution;
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("execution.db");
    let mut store = open(&path)?;
    let record = reserve_two(&mut store)?;
    let metadata = test_support::receipt(b"synthetic status evidence")?;
    store.receive(&metadata, b"synthetic status evidence", b"config")?;
    let encoded = key(&test_support::interpretation()?)?;
    store.db.execute(
        "INSERT INTO interpretations(receipt,version,evidence) VALUES (?1,1,?2)",
        params![key(&metadata.id)?, encoded],
    )?;
    // The notification transaction will own admission; this fixture tests storage only.
    store.db.execute("INSERT INTO awos_execution_history(machine_order,item_index,receipt,interpretation,state,recorded_at,cancel_outstanding) VALUES (?1,1,?2,1,'AnalyzerCancelled',105,1)",params![record.id,key(&metadata.id)?])?;
    for sql in [
        "UPDATE awos_execution_history SET state='Scheduled'",
        "DELETE FROM awos_execution_history",
    ] {
        assert!(store.db.execute(sql, []).is_err());
    }
    assert!(store.db.execute("INSERT INTO awos_execution_history(machine_order,item_index,receipt,interpretation,state,recorded_at,cancel_outstanding) VALUES (?1,1,?2,1,'AnalyzerCancelled',105,1)",params![record.id,key(&metadata.id)?]).is_err());
    drop(store);
    let store = open(&path)?;
    let restored = store.machine_order(&identity()?)?.ok_or("missing")?;
    let item = restored.items.first().ok_or("item")?;
    let execution = item.execution.as_ref().ok_or("projection")?;
    assert_eq!(execution.state, AwosExecution::AnalyzerCancelled);
    assert_eq!(execution.receipt, metadata.id);
    assert!(execution.cancel_outstanding);
    assert_eq!(execution.recorded_at, 105);
    assert_eq!(item.state, OrderState::Sending);
    assert!(restored.items.get(1).ok_or("sibling")?.execution.is_none());
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM outbox", [], |r| r.get::<_, i64>(0))?,
        0
    );
    Ok(())
}
