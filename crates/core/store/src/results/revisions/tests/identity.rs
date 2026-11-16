//! Tests result identity, retransmission and atomic revision writes.
use crate::test_support::*;
use crate::*;
use analyte_model::Reason;

#[test]
fn adr0003_revisions_new_retransmission_correction_conflict_runs_and_restart() -> TestResult {
    use analyte_model::Revision;
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("revisions.db");
    let mut store = open(&path)?;
    assert_eq!(
        revised_receipt(&mut store, "001", false, "run1")?,
        RevisionOutcome::New(Revision(0))
    );
    drop(store);
    let mut store = open(&path)?;
    assert_eq!(
        revised_receipt(&mut store, "001", false, "run1")?,
        RevisionOutcome::Retransmission(Revision(0))
    );
    assert_eq!(
        revised_receipt(&mut store, "002", true, "run1")?,
        RevisionOutcome::Correction(Revision(1))
    );
    assert_eq!(
        revised_receipt(&mut store, "003", false, "run1")?,
        RevisionOutcome::Conflict
    );
    assert_eq!(store.parked(0)?.len(), 1);
    assert_eq!(
        revised_receipt(&mut store, "001", false, "run1")?,
        RevisionOutcome::Retransmission(Revision(0))
    );
    assert_eq!(
        revised_receipt(&mut store, "001", false, "run2")?,
        RevisionOutcome::New(Revision(0))
    );
    assert_eq!(
        revised_receipt(&mut store, "004", true, "correction-without-original")?,
        RevisionOutcome::New(Revision(0))
    );
    assert!(
        store
            .db
            .execute("DELETE FROM observation_revisions", [])
            .is_err()
    );
    assert!(
        store
            .db
            .execute("UPDATE observation_revisions SET revision=10", [])
            .is_err()
    );
    Ok(())
}

#[test]
fn adr0003_revision_builder_failure_rolls_back_interpretation_and_history() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("rollback.db"))?;
    let metadata = receipt(b"raw")?;
    store.receive(&metadata, b"raw", b"config")?;
    let mut evidence = interpretation()?;
    evidence.outcome = Reading::Results(vec![production_result(metadata.id)?]);
    assert!(matches!(
        store.interpret_reconciled(metadata.id, evidence, &[false], &[], |_, _| Err(
            Error::InvalidInput
        )),
        Err(Error::InvalidInput)
    ));
    assert!(store.interpretation(metadata.id, 1)?.is_none());
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM observation_revisions", [], |r| r
                .get::<_, i64>(0))?,
        0
    );
    Ok(())
}

#[test]
fn adr0003_legacy_offset_digest_reconciles_after_restart_without_rewriting_evidence() -> TestResult
{
    use analyte_model::{EvidenceList, Identifier, NonEmptyEvidenceList, Revision, SourceNode};
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("legacy-offsets.db");
    let mut store = open(&path)?;
    let first = receipt(b"original")?;
    store.receive(&first, b"original", b"config")?;
    let mut result = production_result(first.id)?;
    let mut parts = result.parts.as_slice().to_vec();
    parts.first_mut().ok_or(Error::Corruption)?.source_path =
        EvidenceList::new(vec![SourceNode {
            occurrence: 5,
            kind: Identifier::new("OBS")?,
            sequence: None,
            status: None,
        }])?;
    result.parts = NonEmptyEvidenceList::new(EvidenceList::new(parts)?)?;
    let mut old_parts = serde_json::to_value(&result.parts)?;
    for part in old_parts.as_array_mut().ok_or(Error::Corruption)? {
        part.as_object_mut()
            .ok_or(Error::Corruption)?
            .remove("received_at");
    }
    // Construct the previous released algorithm's digest, including source offsets.
    let old_digest = digest(&serde_json::to_vec(&(
        &result.sample,
        result.processing,
        old_parts,
    ))?);
    let mut evidence = interpretation()?;
    evidence.outcome = Reading::Results(vec![result.clone()]);
    let analyte_model::IdentityStatus::Resolved(identity) = &result.identity else {
        return Err(Error::Corruption.into());
    };
    let receipt_key = key(&first.id)?;
    let identity_key = key(&(&first.machine, &identity.observation))?;
    store.db.execute(
        "INSERT INTO interpretations(receipt,version,evidence) VALUES (?1,1,?2)",
        params![receipt_key, key(&evidence)?],
    )?;
    store.db.execute(
        "INSERT INTO observation_revisions VALUES (?1,0,?2,1,?3)",
        params![identity_key, receipt_key, old_digest.0.as_slice()],
    )?;
    drop(store);
    let mut store = open(&path)?;
    let second = receipt(b"resent")?;
    store.receive(&second, b"resent", b"config")?;
    result.receipt = second.id;
    let mut parts = result.parts.as_slice().to_vec();
    parts.first_mut().ok_or(Error::Corruption)?.source_path =
        EvidenceList::new(vec![SourceNode {
            occurrence: 6,
            kind: Identifier::new("OBS")?,
            sequence: None,
            status: None,
        }])?;
    result.parts = NonEmptyEvidenceList::new(EvidenceList::new(parts)?)?;
    evidence.outcome = Reading::Results(vec![result]);
    let commit =
        store.interpret_reconciled(second.id, evidence, &[false], &[], |_, outcomes| {
            assert_eq!(outcomes, &[RevisionOutcome::Retransmission(Revision(0))]);
            Ok(vec![])
        })?;
    assert_eq!(
        commit.outcomes,
        vec![RevisionOutcome::Retransmission(Revision(0))]
    );
    assert_eq!(
        store.receipt(first.id)?.ok_or(Error::Corruption)?.raw,
        b"original"
    );
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM observation_revisions", [], |r| r
                .get::<_, i64>(0))?,
        1
    );
    Ok(())
}

#[test]
fn mutation_adr0003_reconciliation_rejects_each_provenance_mismatch_without_interpretation()
-> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("provenance.db"))?;
    let metadata = receipt(b"raw")?;
    store.receive(&metadata, b"raw", b"config")?;
    for field in 0..3 {
        let mut evidence = interpretation()?;
        match field {
            0 => evidence.config = b"different".to_vec(),
            1 => evidence.driver.value = Identifier::new("different")?,
            2 => evidence.config_version = Identifier::new("different")?,
            _ => unreachable!("fixed field cases"),
        }
        assert!(matches!(
            store.interpret_reconciled(metadata.id, evidence, &[], &[], |_, _| Ok(vec![])),
            Err(Error::InvalidInput)
        ));
        assert!(store.interpretation(metadata.id, 1)?.is_none());
        assert!(store.parked(0)?.is_empty());
        assert_eq!(store.raw(metadata.id)?, Some(b"raw".to_vec()));
    }
    Ok(())
}

#[test]
fn mutation_adr0003_reconciliation_refuses_wrong_result_receipt_and_correction_count_atomically()
-> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("result-owner.db"))?;
    let metadata = receipt(b"raw")?;
    store.receive(&metadata, b"raw", b"config")?;
    for (owner, corrections) in [(metadata.id, vec![]), (receipt(b"other")?.id, vec![false])] {
        let mut evidence = interpretation()?;
        evidence.outcome = Reading::Results(vec![production_result(owner)?]);
        assert!(matches!(
            store.interpret_reconciled(metadata.id, evidence, &corrections, &[], |_, _| Ok(vec![])),
            Err(Error::InvalidInput)
        ));
        assert!(store.interpretation(metadata.id, 1)?.is_none());
        assert_eq!(
            store
                .db
                .query_row("SELECT count(*) FROM observation_revisions", [], |r| r
                    .get::<_, i64>(0))?,
            0
        );
        assert!(store.claim(0)?.is_none());
    }
    Ok(())
}

#[test]
fn mutation_adr0003_rule7_reconciliation_bounds_results_and_findings_independently() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("reconcile-bounds.db"))?;
    let metadata = receipt(b"raw")?;
    store.receive(&metadata, b"raw", b"config")?;
    store.limits.page_rows = 1;
    let result = production_result(metadata.id)?;
    let mut evidence = interpretation()?;
    evidence.outcome = Reading::Results(vec![result.clone(), result.clone()]);
    assert!(matches!(
        store.interpret_reconciled(metadata.id, evidence, &[false, false], &[], |_, _| Ok(
            vec![]
        )),
        Err(Error::InvalidInput)
    ));
    assert!(store.interpretation(metadata.id, 1)?.is_none());
    let finding = Parked {
        kind: Identifier::new("unknown-test")?,
        reason: Reason::new("unmapped synthetic evidence")?,
        targets: vec![Identifier::new(&serde_json::from_str::<String>(&key(
            &metadata.id,
        )?)?)?],
    };
    assert!(matches!(
        store.interpret_reconciled(
            metadata.id,
            interpretation()?,
            &[],
            &[finding.clone(), finding.clone()],
            |_, _| Ok(vec![])
        ),
        Err(Error::InvalidInput)
    ));
    assert!(store.interpretation(metadata.id, 1)?.is_none());
    assert!(store.parked(0)?.is_empty());
    let mut evidence = interpretation()?;
    evidence.outcome = Reading::Results(vec![result]);
    let commit =
        store.interpret_reconciled(metadata.id, evidence, &[false], &[finding], |_, _| {
            Ok(vec![])
        })?;
    assert_eq!(
        commit.outcomes,
        vec![RevisionOutcome::New(analyte_model::Revision(0))]
    );
    assert_eq!(store.parked(0)?.len(), 1);
    Ok(())
}

#[test]
fn mutation_adr0003_batch_duplicates_reuse_the_matching_revision_and_keep_runs_distinct()
-> TestResult {
    use analyte_model::Revision;
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("batch-identity.db"))?;
    let metadata = receipt(b"raw")?;
    store.receive(&metadata, b"raw", b"config")?;
    let original = production_result(metadata.id)?;
    let mut corrected = original.clone();
    corrected.sample.as_mut().ok_or("missing sample")?.id = SampleId::new("changed")?;
    let mut rerun = original.clone();
    let analyte_model::IdentityStatus::Resolved(identity) = &mut rerun.identity else {
        return Err("missing identity".into());
    };
    identity.observation.run = analyte_model::SourceText::new("new-run")?;
    let mut evidence = interpretation()?;
    evidence.outcome = Reading::Results(vec![
        original.clone(),
        original.clone(),
        corrected,
        original,
        rerun,
    ]);
    let commit = store.interpret_reconciled(
        metadata.id,
        evidence,
        &[false, false, true, false, false],
        &[],
        |_, _| Ok(vec![]),
    )?;
    assert_eq!(
        commit.outcomes,
        vec![
            RevisionOutcome::New(Revision(0)),
            RevisionOutcome::Retransmission(Revision(0)),
            RevisionOutcome::Correction(Revision(1)),
            RevisionOutcome::Retransmission(Revision(0)),
            RevisionOutcome::New(Revision(0))
        ]
    );
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM observation_revisions", [], |r| r
                .get::<_, i64>(0))?,
        3
    );
    assert!(store.parked(0)?.is_empty());
    Ok(())
}

#[test]
fn mutation_adr0003_rule7_interpretation_and_audit_byte_limits_are_inclusive() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("encoded-bounds.db"))?;
    let metadata = receipt(b"raw")?;
    store.receive(&metadata, b"raw", b"config")?;
    let mut evidence = interpretation()?;
    evidence.outcome = Reading::Results(vec![production_result(metadata.id)?]);
    store.limits.record_bytes = key(&evidence)?.len();
    store.interpret_reconciled(metadata.id, evidence, &[false], &[], |_, _| Ok(vec![]))?;
    for delta in [0, 1] {
        let action = Reprocess {
            receipt: metadata.id,
            parked: None,
            audit: Audit {
                actor: Identifier::new("operator")?,
                action: Identifier::new("receipt-reprocess")?,
                targets: vec![],
                time: 42,
                reason: Reason::new(&"r".repeat(1024))?,
            },
        };
        let size = reprocess::audit_evidence(&action, 2, false)?.len();
        store.limits.record_bytes = size.checked_sub(delta).ok_or("bound")?;
        let result =
            store.reprocess_reconciled(&action, interpretation()?, &[], &[], |_, _| Ok(vec![]));
        if delta == 0 {
            assert_eq!(result?.version, 2);
        } else {
            assert!(matches!(result, Err(Error::InvalidInput)));
        }
    }
    Ok(())
}

#[test]
fn mutation_adr0003_delivery_bounds_and_machine_owner_refuse_atomically() -> TestResult {
    for (length, wrong_machine, succeeds) in [
        (2048, false, true),
        (2049, false, false),
        (2050, false, false),
        (16, true, false),
    ] {
        let temp = tempfile::tempdir()?;
        let mut store = Store::open(
            &temp.path().join("delivery-bounds.db"),
            Limits {
                record_bytes: 2048,
                ..limits()
            },
            Box::new(Space(u64::MAX)),
        )?;
        let metadata = receipt(b"raw")?;
        store.receive(&metadata, b"raw", b"config")?;
        let mut evidence = interpretation()?;
        evidence.outcome = Reading::Results(vec![production_result(metadata.id)?]);
        let id = DeliveryId::try_from(uuid::Uuid::now_v7())?;
        let result =
            store.interpret_reconciled(metadata.id, evidence, &[false], &[], |evidence, _| {
                let Reading::Results(results) = &evidence.outcome else {
                    return Err(Error::InvalidInput);
                };
                let Some(analyte_model::IdentityStatus::Resolved(identity)) =
                    results.first().map(|r| &r.identity)
                else {
                    return Err(Error::InvalidInput);
                };
                Ok(vec![Delivery {
                    id,
                    destination: Identifier::new("app").map_err(|_| Error::InvalidInput)?,
                    ordering_key: OrderingKey {
                        machine: if wrong_machine {
                            Identifier::new("wrong-machine").map_err(|_| Error::InvalidInput)?
                        } else {
                            metadata.machine.clone()
                        },
                        observation: identity.observation.clone(),
                    },
                    payload: vec![b'x'; length],
                    contract_version: 1,
                }])
            });
        if succeeds {
            assert_eq!(result?.version, 1);
            let claim = store.claim(0)?.ok_or("missing delivery")?;
            assert_eq!(claim.id, id);
            assert_eq!(claim.payload, vec![b'x'; length]);
        } else {
            assert!(matches!(result, Err(Error::InvalidInput)));
            assert!(store.interpretation(metadata.id, 1)?.is_none());
            assert!(store.claim(0)?.is_none());
            assert_eq!(
                store
                    .db
                    .query_row("SELECT count(*) FROM observation_revisions", [], |r| r
                        .get::<_, i64>(0))?,
                0
            );
        }
        assert_eq!(store.raw(metadata.id)?, Some(b"raw".to_vec()));
    }
    Ok(())
}
