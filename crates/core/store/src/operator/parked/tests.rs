//! Tests parked evidence and failure reasons.
use crate::test_support::*;
use crate::*;
use analyte_model::Reason;

#[test]
fn rule2_parse_failure_parks_and_resolution_is_audited_once() -> TestResult {
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

#[test]
fn mutation_adr0003_park_resolve_and_audit_keep_exact_evidence_once_after_restart() -> TestResult {
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
