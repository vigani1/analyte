//! Tests atomic LAW custody without sending machine commands or allocating status results.
use super::*;
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
) -> Result<LawCustody, Box<dyn std::error::Error>> {
    Ok(LawCustody {
        scope: SourceText::new("machine/sender")?,
        message: Identifier::new(message)?,
        work: vec![LawWork {
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
    custody: &LawCustody,
    raw: &[u8],
) -> Result<InterpretationCommit, Box<dyn std::error::Error>> {
    let mut metadata = test_support::receipt(raw)?;
    metadata.machine = identity()?.machine;
    store.receive(&metadata, raw, b"config")?;
    Ok(store.interpret_law(
        metadata.id,
        test_support::interpretation()?,
        (&[], &[]),
        policy(),
        (custody, None),
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
        Some(LawDisposition::Accepted)
    ));
    let ip = notice(&order, AwosExecution::InProgress, "processing")?;
    assert!(matches!(
        commit(&mut store, &ip, b"processing")?.law,
        Some(LawDisposition::Accepted)
    ));
    drop(store);
    let mut store = open(&path)?;
    assert!(matches!(
        commit(&mut store, &sc, b"scheduled")?.law,
        Some(LawDisposition::Accepted)
    ));
    let late = notice(&order, AwosExecution::Scheduled, "ambiguous-late")?;
    assert!(matches!(
        commit(&mut store, &late, b"late")?.law,
        Some(LawDisposition::Rejected(_))
    ));
    let ca = notice(&order, AwosExecution::AnalyzerCancelled, "cancel-after-ip")?;
    assert!(matches!(
        commit(&mut store, &ca, b"ca")?.law,
        Some(LawDisposition::Rejected(_))
    ));
    assert!(matches!(
        commit(&mut store, &ip, b"different bytes")?.law,
        Some(LawDisposition::Rejected(_))
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
        Some(LawDisposition::Rejected(LawRejection { occurrence: 2, .. }))
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
    assert!(commit(&mut store, &custody, b"ca").is_err());
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
        Some(LawDisposition::Accepted)
    ));
    Ok(())
}
