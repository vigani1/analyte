//! Tests parked evidence and failure reasons.
use crate::test_support::*;
use crate::*;
use analyte_model::Reason;

// Safety rule 2 (ADR 0003)
#[test]
fn parse_failure_parks_and_resolution_audited_once() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    let metadata = receipt(b"invalid")?;
    store.receive(&metadata, b"invalid", b"config")?;
    let mut reading = interpretation()?;
    reading.outcome = Reading::Failed(Reason::new("invalid syntax")?);
    store
        .reconcile(
            ReconcileAction::Interpret(metadata.id),
            reading.clone(),
            ReconcileInput::standard(&[], &[]),
            |_, _| Ok(Vec::new()),
        )
        .map(|commit| commit.version)?;
    let audit = Audit {
        actor: Identifier::new("operator")?,
        action: Identifier::new("resolve")?,
        targets: vec![Identifier::new("parked:1")?],
        time: 1,
        reason: Reason::new("reconciled using synthetic evidence")?,
    };
    store.resolve(1, &audit)?;
    assert!(matches!(
        store.resolve(1, &audit),
        Err(OperatorError::Conflict)
    ));
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM audit", [], |r| r.get::<_, i64>(0))?,
        1
    );
    assert_constraint(store.db.execute("DELETE FROM audit", []));
    Ok(())
}

// ADR 0003; mutation-testing regression
#[test]
fn park_resolve_audit_keep_exact_evidence_after_restart() -> TestResult {
    let encoded = parked::evidence("synthetic-kind", "synthetic-target", "exact reason", None)?;
    let decoded: parked::ParkingEvidence = serde_json::from_str(&encoded)?;
    assert_eq!(decoded.details.kind.as_str(), "synthetic-kind");
    assert_eq!(decoded.details.reason.as_str(), "exact reason");
    assert_eq!(
        decoded.details.targets,
        vec![Identifier::new("synthetic-target")?]
    );
    assert!(decoded.context.is_none());
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("parking-custody.db");
    let mut store = open(&path)?;
    let item = Parked {
        kind: Identifier::new("unknown-test")?,
        reason: Reason::new("exact synthetic reason")?,
        targets: vec![Identifier::new("synthetic-evidence")?],
    };
    let id = store.park(&item)?;
    let entry = Audit {
        actor: Identifier::new("operator")?,
        action: Identifier::new("reviewed")?,
        targets: vec![Identifier::new("synthetic-evidence")?],
        time: 42,
        reason: Reason::new("exact resolution reason")?,
    };
    store.audit(&entry)?;
    assert_eq!(store.audits(0)?.len(), 1);
    let before = store.parked_item(id)?.ok_or("missing item")?.evidence;
    store.db.execute_batch("CREATE TRIGGER fail_resolution_audit BEFORE INSERT ON audit BEGIN SELECT RAISE(ABORT,'synthetic audit failure'); END;")?;
    assert!(matches!(
        store.resolve(id, &entry),
        Err(OperatorError::Storage(StorageError::ConstraintViolation(_)))
    ));
    assert!(
        store
            .parked_item(id)?
            .ok_or("missing item")?
            .resolution
            .is_none()
    );
    store
        .db
        .execute_batch("DROP TRIGGER fail_resolution_audit;")?;
    store.resolve(id, &entry)?;
    assert!(matches!(
        store.resolve(id, &entry),
        Err(OperatorError::Conflict)
    ));
    assert!(matches!(
        store.resolve(i64::MAX, &entry),
        Err(OperatorError::Conflict)
    ));
    drop(store);
    let store = open(&path)?;
    let retained = store.parked_item(id)?.ok_or("missing retained item")?;
    assert_eq!(retained.evidence, before);
    let details = retained.details()?;
    assert_eq!(details.kind, item.kind);
    assert_eq!(details.reason, item.reason);
    assert_eq!(details.targets, item.targets);
    let resolution: Audit =
        serde_json::from_str(&retained.resolution.ok_or("missing resolution")?)?;
    assert_eq!(resolution.actor, entry.actor);
    assert_eq!(resolution.reason, entry.reason);
    assert_eq!(resolution.time, 42);
    assert_eq!(store.audits(0)?.len(), 2);
    Ok(())
}

// Owner request 2026-11-19 (admin restyle b1): the Needs review filter shows
// real counts of open and resolved items, and each filter pages through only its
// own items in item order. Reading changes nothing, and the open count is the
// one the parked-items alert reads.
#[test]
fn parked_items_are_counted_and_paged_as_open_or_resolved() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    assert_eq!(store.parked_counts()?, ParkedCounts::default());
    let mut ids = Vec::new();
    for reason in ["first", "second", "third", "fourth"] {
        ids.push(store.park(&Parked {
            kind: Identifier::new("synthetic")?,
            reason: Reason::new(reason)?,
            targets: vec![],
        })?);
    }
    let resolve = |store: &mut Store, id: i64| -> TestResult {
        Ok(store.resolve(
            id,
            &Audit {
                actor: Identifier::new("operator")?,
                action: Identifier::new("parked-resolve")?,
                targets: vec![Identifier::new(&id.to_string())?],
                time: 5,
                reason: Reason::new("settled")?,
            },
        )?)
    };
    resolve(&mut store, ids[1])?;
    resolve(&mut store, ids[3])?;
    let counts = store.parked_counts()?;
    assert_eq!(
        counts,
        ParkedCounts {
            open: 2,
            resolved: 2
        }
    );
    assert_eq!(
        store.alert_value(&AlertKey::ParkedItems, 0, &[])?,
        Some(counts.open)
    );
    store.limits.page_rows = 1;
    for (filter, expected) in [
        (ParkedFilter::Open, [ids[0], ids[2]]),
        (ParkedFilter::Resolved, [ids[1], ids[3]]),
    ] {
        let mut after = 0;
        let mut seen = Vec::new();
        loop {
            let page = store.parked_filtered(after, filter)?;
            let Some(last) = page.last() else { break };
            assert_eq!(page.len(), 1);
            assert_eq!(last.resolution.is_some(), filter == ParkedFilter::Resolved);
            after = last.id;
            seen.push(last.id);
        }
        assert_eq!(seen, expected, "{filter:?}");
    }
    assert_eq!(store.parked_counts()?, counts);
    Ok(())
}

// Owner review 2026-11-20 (PR 275): Needs review lists the items resolved
// today under the open ones: only those resolved inside the day asked for,
// newest first and bounded; open items and resolutions that cannot be read
// are left out, and reading changes nothing.
#[test]
fn resolved_items_of_one_day_come_newest_first_and_bounded() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    let mut ids = Vec::new();
    for reason in ["a", "b", "c", "d", "e", "f"] {
        ids.push(store.park(&Parked {
            kind: Identifier::new("synthetic")?,
            reason: Reason::new(reason)?,
            targets: vec![],
        })?);
    }
    // Resolved at 99 (the day before), 100, 150, 150, 199, and 200 (the next day).
    for (id, time) in ids.iter().zip([99, 150, 100, 150, 199, 200]) {
        store.resolve(
            *id,
            &Audit {
                actor: Identifier::new("operator")?,
                action: Identifier::new("parked-resolve")?,
                targets: vec![Identifier::new(&id.to_string())?],
                time,
                reason: Reason::new("settled")?,
            },
        )?;
    }
    let open = store.park(&Parked {
        kind: Identifier::new("synthetic")?,
        reason: Reason::new("open")?,
        targets: vec![],
    })?;
    let unreadable = store.park(&Parked {
        kind: Identifier::new("synthetic")?,
        reason: Reason::new("unreadable")?,
        targets: vec![],
    })?;
    store.db.execute(
        "UPDATE parked SET resolution='resolved by hand' WHERE id=?1",
        [unreadable],
    )?;
    let counts = store.parked_counts()?;
    let day = |store: &Store, limit| -> Result<Vec<i64>, Box<dyn std::error::Error>> {
        Ok(store
            .parked_resolved_between(100, 200, limit)?
            .iter()
            .map(|item| item.id)
            .collect())
    };
    // Newest first; the same time comes in the newer item's order first.
    let all = day(&store, 10)?;
    assert_eq!(all, [ids[4], ids[3], ids[1], ids[2]]);
    assert!(!all.contains(&open) && !all.contains(&unreadable));
    assert_eq!(day(&store, 2)?, [ids[4], ids[3]]);
    store.limits.page_rows = 1;
    assert_eq!(day(&store, 10)?, [ids[4]]);
    assert!(store.parked_resolved_between(300, 400, 10)?.is_empty());
    assert_eq!(store.parked_counts()?, counts);
    Ok(())
}
