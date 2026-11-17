//! Tests atomic LAW custody without sending machine commands or allocating status results.
use super::*;
use crate::law::{Custody, Disposition, Rejection, Work};
use analyte_model::AwosExecution;

fn policy() -> StatusLifecycle<'static> {
    StatusLifecycle {
        transitions: &[],
        withheld: &[],
        single_run: &[],
        run: |_| None,
    }
}
fn setup(store: &mut Store) -> Result<MachineOrder, Box<dyn std::error::Error>> {
    let mut order = reserve_two(store)?;
    order.original_command = Some(b"synthetic retained command".to_vec());
    let tx = store.db.transaction()?;
    machine_orders::append(&tx, &order, None, 65536)?;
    tx.commit()?;
    Ok(order)
}
fn notice(
    order: &MachineOrder,
    state: AwosExecution,
    message: &str,
) -> Result<Custody, Box<dyn std::error::Error>> {
    Ok(Custody {
        scope: SourceText::new("machine/sender")?,
        message: Identifier::new(message)?,
        work: vec![Work {
            order: order.id,
            item: 1,
            placer: Identifier::new(&format!("{}-1", order.id))?,
            command: digest(b"synthetic retained command"),
            occurrence: 1,
            execution: Some(state),
            complete: false,
            source: SourceText::new("synthetic ORC and estimate evidence")?,
        }],
        rejection: None,
        recorded_at: 105,
    })
}
fn commit(
    store: &mut Store,
    custody: &Custody,
    raw: &[u8],
) -> Result<InterpretationCommit, Box<dyn std::error::Error>> {
    let mut metadata = test_support::receipt(raw)?;
    metadata.machine = identity()?.machine;
    store.receive(&metadata, raw, b"config")?;
    Ok(store.reconcile(
        None.map_or(
            ReconcileAction::Interpret(metadata.id),
            ReconcileAction::Reprocess,
        ),
        test_support::interpretation()?,
        ReconcileInput {
            corrections: &[],
            findings: &[],
            mode: ReconcileMode::Law {
                lifecycle: policy(),
                custody,
            },
        },
        |_, _| Ok(Vec::new()),
    )?)
}

#[test]
fn palm_tf2b_c8_figure_c8_1_adr0003_0011_notices_commit_once_across_restart() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("custody.db");
    let mut store = open(&path)?;
    let order = setup(&mut store)?;
    let sc = notice(&order, AwosExecution::Scheduled, "scheduled")?;
    assert!(matches!(
        commit(&mut store, &sc, b"scheduled")?.law,
        Some(Disposition::Accepted)
    ));
    let ip = notice(&order, AwosExecution::InProgress, "processing")?;
    assert!(matches!(
        commit(&mut store, &ip, b"processing")?.law,
        Some(Disposition::Accepted)
    ));
    drop(store);
    let mut store = open(&path)?;
    assert!(matches!(
        commit(&mut store, &sc, b"scheduled")?.law,
        Some(Disposition::Accepted)
    ));
    let late = notice(&order, AwosExecution::Scheduled, "ambiguous-late")?;
    assert!(matches!(
        commit(&mut store, &late, b"late")?.law,
        Some(Disposition::Rejected(_))
    ));
    let ca = notice(&order, AwosExecution::AnalyzerCancelled, "cancel-after-ip")?;
    assert!(matches!(
        commit(&mut store, &ca, b"ca")?.law,
        Some(Disposition::Rejected(_))
    ));
    assert!(matches!(
        commit(&mut store, &ip, b"different bytes")?.law,
        Some(Disposition::Rejected(_))
    ));
    let restored = store.machine_order(&identity()?)?.ok_or("order")?;
    assert_eq!(
        restored
            .items
            .first()
            .ok_or("item")?
            .execution
            .as_ref()
            .ok_or("execution")?
            .state,
        AwosExecution::InProgress
    );
    assert!(restored.items.get(1).ok_or("sibling")?.execution.is_none());
    assert_eq!(restored.state, OrderState::Sending);
    for (table, expected) in [
        ("awos_execution_history", 2),
        ("law_work_history", 2),
        ("outbox", 0),
        ("observation_revisions", 0),
    ] {
        assert_eq!(
            store
                .db
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                    .get::<_, i64>(0))?,
            expected
        );
    }
    Ok(())
}

#[test]
fn palm_tf2b_b7_6_adr0003_conflicting_second_awos_rejects_whole_notice() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("atomic.db"))?;
    let order = setup(&mut store)?;
    let mut custody = notice(&order, AwosExecution::Scheduled, "conflict")?;
    let mut unknown = custody.work.first().ok_or("work")?.clone();
    unknown.item = 2;
    unknown.placer = Identifier::new("unknown")?;
    unknown.occurrence = 2;
    custody.work.push(unknown);
    let result = commit(&mut store, &custody, b"mixed valid and unknown")?;
    assert!(matches!(
        result.law,
        Some(Disposition::Rejected(Rejection { occurrence: 2, .. }))
    ));
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM awos_execution_history", [], |r| r
                .get::<_, i64>(0))?,
        0
    );
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM law_work_history", [], |r| r
                .get::<_, i64>(0))?,
        0
    );
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM parked", [], |r| r.get::<_, i64>(0))?,
        1
    );
    commit(&mut store, &custody, b"mixed valid and unknown")?;
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM parked", [], |r| r.get::<_, i64>(0))?,
        1
    );
    Ok(())
}

#[test]
fn adr0003_0010_law_projection_commit_failure_leaves_raw_pending_and_retry_safe() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("rollback.db"))?;
    let order = setup(&mut store)?;
    let custody = notice(&order, AwosExecution::AnalyzerCancelled, "ca")?;
    store.db.execute_batch("CREATE TRIGGER fail_execution BEFORE INSERT ON awos_execution_history BEGIN SELECT RAISE(ABORT,'synthetic disk failure'); END;")?;
    {
        let refusal = commit(&mut store, &custody, b"ca")
            .map(|_| ())
            .expect_err("input must be refused");
        assert!(
            matches!(
                refusal.downcast_ref::<ReconciliationError>(),
                Some(ReconciliationError::ConstraintViolation(_))
            ),
            "unexpected refusal: {refusal:?}"
        );
    }
    for table in [
        "interpretations",
        "law_messages",
        "law_work_history",
        "awos_execution_history",
        "outbox",
        "observation_revisions",
    ] {
        assert_eq!(
            store
                .db
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                    .get::<_, i64>(0))?,
            0
        );
    }
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM receipts", [], |r| r.get::<_, i64>(0))?,
        1
    );
    store.db.execute_batch("DROP TRIGGER fail_execution")?;
    assert!(matches!(
        commit(&mut store, &custody, b"ca")?.law,
        Some(Disposition::Accepted)
    ));
    Ok(())
}

fn accept(store: &mut Store, order: &MachineOrder) -> TestResult {
    store.finish_order_items(
        order.id,
        &[
            ItemOutcome {
                index: 1,
                decision: ItemDecision::Accepted,
                code: SourceText::new("LAW-OK:SC")?,
            },
            ItemOutcome {
                index: 2,
                decision: ItemDecision::Accepted,
                code: SourceText::new("LAW-OK:SC")?,
            },
        ],
        102,
        event,
    )?;
    Ok(())
}

#[test]
fn palm_tf2b_c8_adr0003_analyzer_ca_blocks_new_cancel_without_clearing_app_intent() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("admission.db"))?;
    let order = setup(&mut store)?;
    accept(&mut store, &order)?;
    let custody = notice(&order, AwosExecution::AnalyzerCancelled, "cancelled")?;
    commit(&mut store, &custody, b"cancelled")?;
    store.order_page(&page(2, "cancel_requested", false, 200)?)?;
    let version = NonZeroU64::new(2).ok_or("version")?;
    assert!(matches!(
        store.request_order_cancel(
            order.id,
            version,
            106,
            &Identifier::new("owner")?,
            30,
            event
        ),
        Err(OrderError::AnalyzerCancelled)
    ));
    assert!(
        store
            .cancel_candidates(&identity()?.destination, &identity()?.machine, 106, 30)?
            .is_empty()
    );
    let retained = store.machine_order(&identity()?)?.ok_or("order")?;
    assert_eq!(retained.state, OrderState::Accepted);
    assert_eq!(
        retained.items.get(1).ok_or("sibling")?.state,
        OrderState::Accepted
    );
    assert!(retained.items.get(1).ok_or("sibling")?.execution.is_none());
    assert_eq!(store.db.query_row("SELECT json_extract(json_extract(evidence,'$.document'),'$.state') FROM orders WHERE id='order'",[],|r|r.get::<_,String>(0))?,"cancel_requested");
    Ok(())
}

#[test]
fn palm_tf2b_c8_adr0003_ca_during_pending_cancel_needs_correlated_cr_or_uc() -> TestResult {
    for decision in [ItemDecision::CancelConfirmed, ItemDecision::CancelRefused] {
        let temp = tempfile::tempdir()?;
        let mut store = open(&temp.path().join("pending-cancel.db"))?;
        let order = setup(&mut store)?;
        accept(&mut store, &order)?;
        store.order_page(&page(2, "cancel_requested", false, 200)?)?;
        store.request_order_cancel(
            order.id,
            NonZeroU64::new(2).ok_or("version")?,
            103,
            &Identifier::new("owner")?,
            30,
            event,
        )?;
        let custody = notice(&order, AwosExecution::AnalyzerCancelled, "cancelled")?;
        commit(&mut store, &custody, b"cancelled")?;
        let pending = store.machine_order(&identity()?)?.ok_or("pending")?;
        assert_eq!(pending.state, OrderState::CancelRequested);
        assert!(
            pending
                .items
                .first()
                .ok_or("item")?
                .execution
                .as_ref()
                .ok_or("projection")?
                .cancel_outstanding
        );
        assert_eq!(
            pending.items.get(1).ok_or("sibling")?.state,
            OrderState::CancelRequested
        );
        let outcome = store.finish_order_items(
            order.id,
            &[
                ItemOutcome {
                    index: 1,
                    decision,
                    code: SourceText::new("LAB28 correlated response")?,
                },
                ItemOutcome {
                    index: 2,
                    decision,
                    code: SourceText::new("LAB28 correlated response")?,
                },
            ],
            106,
            event,
        )?;
        assert_eq!(
            outcome.state,
            match decision {
                ItemDecision::CancelConfirmed => OrderState::CancelConfirmed,
                ItemDecision::CancelRefused => OrderState::Accepted,
                ItemDecision::Sent
                | ItemDecision::Accepted
                | ItemDecision::Rejected
                | ItemDecision::Uncertain => return Err("test decision".into()),
            }
        );
        assert!(outcome.items.first().ok_or("item")?.analyzer_cancelled());
        assert_eq!(
            store
                .db
                .query_row("SELECT count(*) FROM parked", [], |r| r.get::<_, i64>(0))?,
            i64::from(matches!(decision, ItemDecision::CancelRefused))
        );
    }
    Ok(())
}

#[test]
fn adr0016_admin_cancellation_is_one_condition_per_awos_with_bounded_full_history() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("listing.db"))?;
    let order = setup(&mut store)?;
    let first = notice(&order, AwosExecution::AnalyzerCancelled, "first-ca")?;
    commit(&mut store, &first, b"first")?;
    let page = store.analyzer_cancelled_items(0)?;
    let anchor = page.first().ok_or("condition")?.cursor;
    assert_eq!(store.analyzer_cancelled_count()?, 1);
    let second = notice(&order, AwosExecution::AnalyzerCancelled, "second-ca")?;
    commit(&mut store, &second, b"second")?;
    assert_eq!(store.analyzer_cancelled_count()?, 1);
    let page = store.analyzer_cancelled_items(0)?;
    assert_eq!(page.len(), 1);
    assert_eq!(page.first().ok_or("condition")?.cursor, anchor);
    assert!(store.analyzer_cancelled_items(anchor)?.is_empty());
    store.limits.page_rows = 1;
    let evidence = store.law_work_evidence(order.id, 1, 0)?;
    assert_eq!(evidence.len(), 1);
    let next = store.law_work_evidence(order.id, 1, evidence.first().ok_or("history")?.cursor)?;
    assert_eq!(next.len(), 1);
    assert_eq!(
        next.first().ok_or("history")?.custody.message.as_str(),
        "second-ca"
    );
    assert_eq!(
        next.first().ok_or("history")?.received_at.as_str(),
        "1970-01-01T00:00:00Z"
    );
    assert!(store.law_work_evidence(order.id, 2, 0)?.is_empty());
    Ok(())
}
