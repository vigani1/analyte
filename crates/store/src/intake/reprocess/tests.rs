//! intake/reprocess regression tests kept beside the storage behavior they guard.
use crate::test_support::*;
use crate::*;
use analyte_model::Reason;

#[test]
fn plan11_adr0003_reprocess_provenance_resolution_failure_and_audit_rollback() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("reprocess.db"))?;
    let metadata = receipt(b"original")?;
    store.receive(&metadata, b"original", b"config")?;
    let mut original = interpretation()?;
    original.outcome = Reading::Failed(Reason::new("test mapping missing")?);
    store
        .interpret_reconciled(metadata.id, original.clone(), &[], &[], |_, _| {
            Ok(Vec::new())
        })
        .map(|commit| commit.version)?;
    let before: String = store
        .db
        .query_row("SELECT evidence FROM interpretations", [], |r| r.get(0))?;
    let parked = store.parked(0)?.first().ok_or("missing parked")?.id;
    let mut current = interpretation()?;
    current.config_version = Identifier::new("current-config")?;
    current.driver.value = Identifier::new("current-driver")?;
    current.config = b"current mapping".to_vec();
    current.outcome = Reading::Results(vec![production_result(metadata.id)?]);
    let action = Reprocess {
        receipt: metadata.id,
        parked: Some(parked),
        audit: Audit {
            actor: Identifier::new("operator")?,
            action: Identifier::new("receipt-reprocess")?,
            targets: vec![],
            time: 42,
            reason: Reason::new("test mapping repaired")?,
        },
    };
    store.db.execute_batch("CREATE TRIGGER fail_reprocess_audit BEFORE INSERT ON audit BEGIN SELECT RAISE(ABORT,'synthetic audit failure'); END;")?;
    assert!(
        store
            .reprocess_reconciled(&action, current.clone(), &[false], &[], |_, _| Ok(vec![]))
            .is_err()
    );
    assert!(store.interpretation(metadata.id, 2)?.is_none());
    assert!(
        store
            .parked_item(parked)?
            .ok_or("missing parked")?
            .resolution
            .is_none()
    );
    assert!(store.audits(0)?.is_empty());
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM observation_revisions", [], |r| r
                .get::<_, i64>(0))?,
        0
    );
    store
        .db
        .execute_batch("DROP TRIGGER fail_reprocess_audit;")?;
    let commit =
        store.reprocess_reconciled(&action, current.clone(), &[false], &[], |_, _| Ok(vec![]))?;
    assert_eq!(commit.version, 2);
    assert!(matches!(
        commit.outcomes.as_slice(),
        [RevisionOutcome::New(_)]
    ));
    let retained = store
        .interpretation(metadata.id, 2)?
        .ok_or("missing version")?;
    assert_eq!(retained.driver, current.driver);
    assert_eq!(retained.config_version, current.config_version);
    assert_eq!(retained.config, current.config);
    let wire: serde_json::Value = serde_json::from_str(&store.db.query_row(
        "SELECT evidence FROM interpretations WHERE version=2",
        [],
        |r| r.get::<_, String>(0),
    )?)?;
    assert_eq!(
        wire["config_digest"],
        serde_json::to_value(digest(&current.config))?
    );
    let after: String = store.db.query_row(
        "SELECT evidence FROM interpretations WHERE version=1",
        [],
        |r| r.get(0),
    )?;
    assert_eq!(before, after);
    assert_eq!(
        store.receipt(metadata.id)?.ok_or("missing receipt")?.raw,
        b"original"
    );
    assert_eq!(store.audits(0)?.len(), 1);
    let resolution: Audit = serde_json::from_str(
        &store
            .parked_item(parked)?
            .ok_or("missing parked")?
            .resolution
            .ok_or("not resolved")?,
    )?;
    assert_eq!(
        resolution.targets,
        store.audits(0)?.first().ok_or("missing audit")?.1.targets
    );
    assert!(
        resolution
            .targets
            .iter()
            .any(|t| t.as_str().ends_with(":2"))
    );
    assert!(matches!(
        store.reprocess_reconciled(&action, current.clone(), &[false], &[], |_, _| Ok(vec![])),
        Err(Error::ReprocessParked)
    ));
    let other = store.park(&Parked {
        kind: Identifier::new("unrelated")?,
        reason: Reason::new("other receipt")?,
        targets: vec![],
    })?;
    let mut action = action;
    action.parked = Some(other);
    assert!(matches!(
        store.reprocess_reconciled(&action, current.clone(), &[false], &[], |_, _| Ok(vec![])),
        Err(Error::ReprocessParked)
    ));
    action.parked = None;
    current.outcome = Reading::Results(vec![]);
    store.reprocess_reconciled(&action, current, &[], &[], |_, _| Ok(vec![]))?;
    assert!(
        store
            .parked(0)?
            .iter()
            .any(|p| p.evidence.contains("reprocessing produced no results"))
    );
    assert_eq!(store.audits(0)?.len(), 2);
    Ok(())
}

#[test]
fn mutation_adr0003_adr0011_lifecycle_reprocessing_requires_audit_and_keeps_original() -> TestResult
{
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("lifecycle-reprocess.db"))?;
    let metadata = receipt(b"raw")?;
    store.receive(&metadata, b"raw", b"config")?;
    let mut failed = interpretation()?;
    failed.outcome = Reading::Failed(Reason::new("unknown test")?);
    store.interpret_reconciled(metadata.id, failed, &[], &[], |_, _| Ok(vec![]))?;
    let parked = store.parked(0)?.first().ok_or("missing parked")?.id;
    let mut action = Reprocess {
        receipt: metadata.id,
        parked: Some(parked),
        audit: Audit {
            actor: Identifier::new("operator")?,
            action: Identifier::new("wrong-action")?,
            targets: vec![],
            time: 42,
            reason: Reason::new("mapping repaired")?,
        },
    };
    let policy = StatusLifecycle {
        transitions: &[],
        withheld: &[],
        single_run: &[],
        run: |_| None,
    };
    let mut evidence = interpretation()?;
    evidence.driver.value = Identifier::new("new-driver")?;
    evidence.config_version = Identifier::new("new-version")?;
    evidence.config = b"new config".to_vec();
    evidence.outcome = Reading::Results(vec![production_result(metadata.id)?]);
    assert!(matches!(
        store.reprocess_with_lifecycle(
            &action,
            evidence.clone(),
            (&[false], &[]),
            policy,
            |_, _| Ok(vec![])
        ),
        Err(Error::InvalidInput)
    ));
    assert!(store.interpretation(metadata.id, 2)?.is_none());
    assert!(store.audits(0)?.is_empty());
    action.audit.action = Identifier::new("receipt-reprocess")?;
    let commit =
        store.reprocess_with_lifecycle(&action, evidence, (&[false], &[]), policy, |_, _| {
            Ok(vec![])
        })?;
    assert_eq!(commit.version, 2);
    assert_eq!(
        commit.outcomes,
        vec![RevisionOutcome::New(analyte_model::Revision(0))]
    );
    let retained = store
        .interpretation(metadata.id, 2)?
        .ok_or("missing reprocessing")?;
    assert_eq!(retained.config, b"new config");
    assert_eq!(retained.driver.value.as_str(), "new-driver");
    assert_eq!(retained.config_version.as_str(), "new-version");
    assert_eq!(store.audits(0)?.len(), 1);
    assert!(
        store
            .parked_item(parked)?
            .ok_or("missing parked")?
            .resolution
            .is_some()
    );
    let original = store.receipt(metadata.id)?.ok_or("missing original")?;
    assert_eq!(original.raw, b"raw");
    assert_eq!(original.config, b"config");
    assert_eq!(original.metadata.config_version, metadata.config_version);
    Ok(())
}

#[test]
fn mutation_adr0003_reprocessing_never_resolves_failure_empty_unresolved_or_flagged_evidence()
-> TestResult {
    for case in ["failed", "empty", "unresolved", "finding", "missing-sample"] {
        let temp = tempfile::tempdir()?;
        let mut store = open(&temp.path().join("failed-reprocessing.db"))?;
        let metadata = receipt(b"raw")?;
        store.receive(&metadata, b"raw", b"config")?;
        let mut failed = interpretation()?;
        failed.outcome = Reading::Failed(Reason::new("unknown test")?);
        store.interpret_reconciled(metadata.id, failed.clone(), &[], &[], |_, _| Ok(vec![]))?;
        let parked = store.parked(0)?.first().ok_or("missing parked")?.id;
        let action = Reprocess {
            receipt: metadata.id,
            parked: Some(parked),
            audit: Audit {
                actor: Identifier::new("operator")?,
                action: Identifier::new("receipt-reprocess")?,
                targets: vec![],
                time: 42,
                reason: Reason::new("synthetic retry")?,
            },
        };
        let mut evidence = interpretation()?;
        let mut corrections = vec![];
        let mut findings = vec![];
        match case {
            "failed" => evidence = failed,
            "empty" => {}
            "unresolved" => {
                let mut result = production_result(metadata.id)?;
                result.identity =
                    analyte_model::IdentityStatus::Unresolved(Reason::new("missing run identity")?);
                evidence.outcome = Reading::Results(vec![result]);
                corrections.push(false);
            }
            "missing-sample" => {
                let mut result = production_result(metadata.id)?;
                result.sample = None;
                evidence.outcome = Reading::Results(vec![result]);
                corrections.push(false);
            }
            "finding" => {
                evidence.outcome = Reading::Results(vec![production_result(metadata.id)?]);
                corrections.push(false);
                findings.push(Parked {
                    kind: Identifier::new("unknown-test")?,
                    reason: Reason::new("unmapped evidence")?,
                    targets: vec![],
                });
            }
            _ => unreachable!("fixed cases"),
        }
        store.reprocess_reconciled(&action, evidence, &corrections, &findings, |_, _| {
            Ok(vec![])
        })?;
        assert!(
            store
                .parked_item(parked)?
                .ok_or("missing original parked")?
                .resolution
                .is_none(),
            "case {case}"
        );
        assert_eq!(
            store.parked(0)?.len(),
            2,
            "case {case}: retain new operator evidence"
        );
        let audits = store.audits(0)?;
        assert_eq!(audits.len(), 1);
        assert!(
            audits
                .first()
                .ok_or("missing audit")?
                .1
                .targets
                .iter()
                .any(|t| t.as_str() == "reprocess:parked")
        );
        assert_eq!(store.raw(metadata.id)?, Some(b"raw".to_vec()));
    }
    Ok(())
}
