//! Tests result identity, retransmission and atomic revision writes.
use crate::test_support::*;
use crate::*;
use analyte_model::Reason;

// ADR 0003
#[test]
fn revisions_tell_new_retransmission_correction_and_runs() -> TestResult {
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
    assert_constraint(store.db.execute("DELETE FROM observation_revisions", []));
    assert_constraint(
        store
            .db
            .execute("UPDATE observation_revisions SET revision=10", []),
    );
    Ok(())
}

// ADR 0003
#[test]
fn builder_failure_rolls_back_interpretation_and_history() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("rollback.db"))?;
    let metadata = receipt(b"raw")?;
    store.receive(&metadata, b"raw", b"config")?;
    let mut evidence = interpretation()?;
    evidence.outcome = Reading::Results(vec![production_result(metadata.id)?]);
    assert!(matches!(
        store.reconcile(
            ReconcileAction::Interpret(metadata.id),
            evidence,
            ReconcileInput::standard(&[false], &[]),
            |_, _| Err(ReconciliationError::InvalidInterpretationInput)
        ),
        Err(ReconciliationError::InvalidInterpretationInput)
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

// ADR 0003
#[test]
fn legacy_offset_digest_reconciles_without_rewriting() -> TestResult {
    use analyte_model::{EvidenceList, Identifier, NonEmptyEvidenceList, Revision, SourceNode};
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("legacy-offsets.db");
    let mut store = open(&path)?;
    let first = receipt(b"original")?;
    store.receive(&first, b"original", b"config")?;
    let mut result = production_result(first.id)?;
    let mut parts = result.parts.as_slice().to_vec();
    parts
        .first_mut()
        .ok_or(ReconciliationError::Corruption)?
        .source_path = EvidenceList::new(vec![SourceNode {
        occurrence: 5,
        kind: Identifier::new("OBS")?,
        sequence: None,
        status: None,
    }])?;
    result.parts = NonEmptyEvidenceList::new(EvidenceList::new(parts)?)?;
    let mut old_parts = serde_json::to_value(&result.parts)?;
    for part in old_parts
        .as_array_mut()
        .ok_or(ReconciliationError::Corruption)?
    {
        part.as_object_mut()
            .ok_or(ReconciliationError::Corruption)?
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
        return Err(ReconciliationError::Corruption.into());
    };
    let receipt_key = key(&first.id)?;
    let identity_key = key(&(&first.machine, &identity.observation))?;
    store.db.execute(
        "INSERT INTO interpretations(receipt,version,evidence) VALUES (?1,1,?2)",
        params![receipt_key, document(&evidence)?],
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
    parts
        .first_mut()
        .ok_or(ReconciliationError::Corruption)?
        .source_path = EvidenceList::new(vec![SourceNode {
        occurrence: 6,
        kind: Identifier::new("OBS")?,
        sequence: None,
        status: None,
    }])?;
    result.parts = NonEmptyEvidenceList::new(EvidenceList::new(parts)?)?;
    evidence.outcome = Reading::Results(vec![result]);
    let commit = store.reconcile(
        ReconcileAction::Interpret(second.id),
        evidence,
        ReconcileInput::standard(&[false], &[]),
        |_, outcomes| {
            assert_eq!(outcomes, &[RevisionOutcome::Retransmission(Revision(0))]);
            Ok(vec![])
        },
    )?;
    assert_eq!(
        commit.outcomes,
        vec![RevisionOutcome::Retransmission(Revision(0))]
    );
    assert_eq!(
        store
            .receipt(first.id)?
            .ok_or(ReconciliationError::Corruption)?
            .raw,
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

// ADR 0003; mutation-testing regression
#[test]
fn reconciliation_rejects_each_provenance_mismatch() -> TestResult {
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
            store.reconcile(
                ReconcileAction::Interpret(metadata.id),
                evidence,
                ReconcileInput::standard(&[], &[]),
                |_, _| Ok(vec![])
            ),
            Err(ReconciliationError::InvalidInterpretationInput)
        ));
        assert!(store.interpretation(metadata.id, 1)?.is_none());
        assert!(store.parked(0)?.is_empty());
        assert_eq!(store.raw(metadata.id)?, Some(b"raw".to_vec()));
    }
    Ok(())
}

// ADR 0003; mutation-testing regression
#[test]
fn reconciliation_refuses_wrong_receipt_or_correction_count() -> TestResult {
    let app = Identifier::new("app")?;
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("result-owner.db"))?;
    let metadata = receipt(b"raw")?;
    store.receive(&metadata, b"raw", b"config")?;
    for (owner, corrections) in [(metadata.id, vec![]), (receipt(b"other")?.id, vec![false])] {
        let mut evidence = interpretation()?;
        evidence.outcome = Reading::Results(vec![production_result(owner)?]);
        assert!(matches!(
            store.reconcile(
                ReconcileAction::Interpret(metadata.id),
                evidence,
                ReconcileInput::standard(&corrections, &[]),
                |_, _| Ok(vec![])
            ),
            Err(ReconciliationError::InvalidInterpretationInput)
        ));
        assert!(store.interpretation(metadata.id, 1)?.is_none());
        assert_eq!(
            store
                .db
                .query_row("SELECT count(*) FROM observation_revisions", [], |r| r
                    .get::<_, i64>(0))?,
            0
        );
        assert!(store.claim_destination(0, &app)?.is_none());
    }
    Ok(())
}

// ADR 0003 rule 7; mutation-testing regression
#[test]
fn reconciliation_bounds_results_and_findings_separately() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("reconcile-bounds.db"))?;
    let metadata = receipt(b"raw")?;
    store.receive(&metadata, b"raw", b"config")?;
    store.limits.page_rows = 1;
    let result = production_result(metadata.id)?;
    let mut evidence = interpretation()?;
    evidence.outcome = Reading::Results(vec![result.clone(), result.clone()]);
    assert!(matches!(
        store.reconcile(
            ReconcileAction::Interpret(metadata.id),
            evidence,
            ReconcileInput {
                corrections: &[false, false],
                findings: &[],
                held_back: None,
                mode: ReconcileMode::Standard
            },
            |_, _| Ok(vec![])
        ),
        Err(ReconciliationError::InvalidInterpretationInput)
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
        store.reconcile(
            ReconcileAction::Interpret(metadata.id),
            interpretation()?,
            ReconcileInput {
                corrections: &[],
                findings: &[finding.clone(), finding.clone()],
                held_back: None,
                mode: ReconcileMode::Standard
            },
            |_, _| Ok(vec![])
        ),
        Err(ReconciliationError::InvalidInterpretationInput)
    ));
    assert!(store.interpretation(metadata.id, 1)?.is_none());
    assert!(store.parked(0)?.is_empty());
    let mut evidence = interpretation()?;
    evidence.outcome = Reading::Results(vec![result]);
    let commit = store.reconcile(
        ReconcileAction::Interpret(metadata.id),
        evidence,
        ReconcileInput::standard(&[false], &[finding]),
        |_, _| Ok(vec![]),
    )?;
    assert_eq!(
        commit.outcomes,
        vec![RevisionOutcome::New(analyte_model::Revision(0))]
    );
    assert_eq!(store.parked(0)?.len(), 1);
    Ok(())
}

// ADR 0003; mutation-testing regression
#[test]
fn batch_duplicates_reuse_revision_keep_runs_distinct() -> TestResult {
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
    let commit = store.reconcile(
        ReconcileAction::Interpret(metadata.id),
        evidence,
        ReconcileInput {
            corrections: &[false, false, true, false, false],
            findings: &[],
            held_back: None,
            mode: ReconcileMode::Standard,
        },
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

// ADR 0003 rule 7; mutation-testing regression
#[test]
fn interpretation_and_audit_byte_limits_are_inclusive() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("encoded-bounds.db"))?;
    let metadata = receipt(b"raw")?;
    store.receive(&metadata, b"raw", b"config")?;
    let mut evidence = interpretation()?;
    evidence.outcome = Reading::Results(vec![production_result(metadata.id)?]);
    store.limits.record_bytes = document(&evidence)?.len();
    store.reconcile(
        ReconcileAction::Interpret(metadata.id),
        evidence,
        ReconcileInput::standard(&[false], &[]),
        |_, _| Ok(vec![]),
    )?;
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
        let result = store.reconcile(
            ReconcileAction::Reprocess(&action),
            interpretation()?,
            ReconcileInput::standard(&[], &[]),
            |_, _| Ok(vec![]),
        );
        if delta == 0 {
            assert_eq!(result?.version, 2);
        } else {
            assert!(matches!(
                result,
                Err(ReconciliationError::InvalidInterpretationInput)
            ));
        }
    }
    Ok(())
}

// ADR 0003; mutation-testing regression
#[test]
fn delivery_bounds_and_machine_owner_refuse_atomically() -> TestResult {
    let app = Identifier::new("app")?;
    for (length, wrong_machine, succeeds) in [
        (2048, false, true),
        (2049, false, false),
        (2050, false, false),
        (16, true, false),
    ] {
        let temp = tempfile::tempdir()?;
        let mut store = Store::open(
            &temp.path().join("delivery-bounds.db"),
            {
                let mut policy = limits();
                policy.record_bytes = 2048;
                policy
            },
            Box::new(Space(u64::MAX)),
        )?;
        let metadata = receipt(b"raw")?;
        store.receive(&metadata, b"raw", b"config")?;
        let mut evidence = interpretation()?;
        evidence.outcome = Reading::Results(vec![production_result(metadata.id)?]);
        let id = DeliveryId::try_from(uuid::Uuid::now_v7())?;
        let result = store.reconcile(
            ReconcileAction::Interpret(metadata.id),
            evidence,
            ReconcileInput::standard(&[false], &[]),
            |evidence, _| {
                let Reading::Results(results) = &evidence.outcome else {
                    return Err(ReconciliationError::InvalidInterpretationInput);
                };
                let Some(analyte_model::IdentityStatus::Resolved(identity)) =
                    results.first().map(|r| &r.identity)
                else {
                    return Err(ReconciliationError::InvalidInterpretationInput);
                };
                Ok(vec![Delivery {
                    id,
                    destination: Identifier::new("app")
                        .map_err(|cause| ReconciliationError::Storage(StorageError::Text(cause)))?,
                    ordering_key: OrderingKey {
                        machine: if wrong_machine {
                            Identifier::new("wrong-machine").map_err(|cause| {
                                ReconciliationError::Storage(StorageError::Text(cause))
                            })?
                        } else {
                            metadata.machine.clone()
                        },
                        observation: identity.observation.clone(),
                    },
                    payload: vec![b'x'; length],
                    contract_version: 1,
                }])
            },
        );
        if succeeds {
            assert_eq!(result?.version, 1);
            let claim = store
                .claim_destination(0, &app)?
                .ok_or("missing delivery")?;
            assert_eq!(claim.id, id);
            assert_eq!(claim.payload, vec![b'x'; length]);
        } else {
            assert!(matches!(
                result,
                Err(ReconciliationError::InvalidInterpretationInput)
            ));
            assert!(store.interpretation(metadata.id, 1)?.is_none());
            assert!(store.claim_destination(0, &app)?.is_none());
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

/// The open items: their kind and stored reason, in item order.
fn open_reasons(store: &Store) -> Result<Vec<(String, String)>, Box<dyn std::error::Error>> {
    store
        .parked_filtered(0, ParkedFilter::Open)?
        .iter()
        .map(|item| {
            let details = item.details()?;
            Ok((
                details.kind.as_str().to_owned(),
                details.reason.as_str().to_owned(),
            ))
        })
        .collect()
}

// AGENTS.md safety rules 2 and 4; owner report 2026-11-21. A result that
// arrives without an identity is explained by the reading's findings: it gets
// no item of its own then, and one when no finding was given. Each problem is
// one open item, also when the same reading repeats it.
#[test]
fn a_result_without_identity_is_parked_through_its_finding_once() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("one-item.db"))?;
    let unresolved = |receipt| -> Result<analyte_model::ResultContent, Box<dyn std::error::Error>> {
        let mut result = production_result(receipt)?;
        result.identity = analyte_model::IdentityStatus::Unresolved(Reason::new("no identity")?);
        Ok(result)
    };
    let finding =
        |receipt: ReceiptId, reason: &str| -> Result<Parked, Box<dyn std::error::Error>> {
            Ok(Parked {
                kind: Identifier::new(ParkedKind::MappingFinding.as_str())?,
                reason: Reason::new(reason)?,
                targets: vec![Identifier::new(&uuid::Uuid::from(receipt).to_string())?],
            })
        };
    // With a finding: the finding is the one item, given twice or not.
    let explained = receipt(b"explained")?;
    store.receive(&explained, b"explained", b"config")?;
    let mut evidence = interpretation()?;
    evidence.outcome = Reading::Results(vec![unresolved(explained.id)?, unresolved(explained.id)?]);
    let findings = [
        finding(explained.id, "unknown test")?,
        finding(explained.id, "unknown test")?,
    ];
    let commit = store.reconcile(
        ReconcileAction::Interpret(explained.id),
        evidence,
        ReconcileInput::standard(&[false, false], &findings),
        |_, _| Ok(Vec::new()),
    )?;
    assert_eq!(
        commit.outcomes,
        vec![RevisionOutcome::Unresolved, RevisionOutcome::Unresolved]
    );
    assert_eq!(
        open_reasons(&store)?,
        vec![("mapping_finding".to_owned(), "unknown test".to_owned())]
    );
    // Without a finding: the store's own item, so the result is never unexplained.
    let bare = receipt(b"bare")?;
    store.receive(&bare, b"bare", b"config")?;
    let mut evidence = interpretation()?;
    evidence.outcome = Reading::Results(vec![unresolved(bare.id)?]);
    store.reconcile(
        ReconcileAction::Interpret(bare.id),
        evidence,
        ReconcileInput::standard(&[false], &[]),
        |_, _| Ok(Vec::new()),
    )?;
    assert_eq!(
        open_reasons(&store)?.get(1),
        Some(&(
            "result_reconciliation".to_owned(),
            "result reconciliation: Unresolved".to_owned()
        ))
    );
    assert_eq!(open_reasons(&store)?.len(), 2);
    Ok(())
}

// AGENTS.md safety rule 4: a problem the store finds itself is separate from
// the message's findings and keeps its own item, one per result.
#[test]
fn conflicting_results_keep_their_own_items_beside_findings() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("conflicts.db"))?;
    assert_eq!(
        revised_receipt(&mut store, "001", false, "run1")?,
        RevisionOutcome::New(analyte_model::Revision(0))
    );
    let metadata = receipt(b"conflicts")?;
    store.receive(&metadata, b"conflicts", b"config")?;
    let changed =
        |sample: &str| -> Result<analyte_model::ResultContent, Box<dyn std::error::Error>> {
            let mut result = production_result(metadata.id)?;
            if let Some(id) = &mut result.sample {
                id.id = SampleId::new(sample)?;
            }
            if let analyte_model::IdentityStatus::Resolved(identity) = &mut result.identity {
                identity.observation.run = analyte_model::SourceText::new("run1")?;
            }
            Ok(result)
        };
    let mut evidence = interpretation()?;
    evidence.outcome = Reading::Results(vec![changed("002")?, changed("003")?]);
    let findings = [Parked {
        kind: Identifier::new(ParkedKind::MappingFinding.as_str())?,
        reason: Reason::new("unit not mapped")?,
        targets: vec![Identifier::new(&uuid::Uuid::from(metadata.id).to_string())?],
    }];
    let commit = store.reconcile(
        ReconcileAction::Interpret(metadata.id),
        evidence,
        ReconcileInput::standard(&[false, false], &findings),
        |_, _| Ok(Vec::new()),
    )?;
    assert_eq!(
        commit.outcomes,
        vec![RevisionOutcome::Conflict, RevisionOutcome::Conflict]
    );
    let conflict = (
        "result_reconciliation".to_owned(),
        "result reconciliation: Conflict".to_owned(),
    );
    assert_eq!(
        open_reasons(&store)?,
        vec![
            ("mapping_finding".to_owned(), "unit not mapped".to_owned()),
            conflict.clone(),
            conflict,
        ]
    );
    Ok(())
}

// Owner report 2026-11-21 (IHE LAW held all results back for one unknown test
// code): a reading that fails only because its findings held the results back
// is explained by those findings. Any other failure keeps its own item, and
// without findings the held-back reading is still parked, never left unseen.
#[test]
fn a_reading_held_back_by_its_findings_parks_only_the_findings() -> TestResult {
    const HELD: &str = "results held back by findings";
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("held-back.db"))?;
    let mut read = |name: &[u8], failure: &str, with_finding: bool| -> TestResult {
        let metadata = receipt(name)?;
        store.receive(&metadata, name, b"config")?;
        let mut evidence = interpretation()?;
        evidence.outcome = Reading::Failed(Reason::new(failure)?);
        let findings = if with_finding {
            vec![Parked {
                kind: Identifier::new(ParkedKind::MappingFinding.as_str())?,
                reason: Reason::new("unknown test")?,
                targets: vec![Identifier::new(&uuid::Uuid::from(metadata.id).to_string())?],
            }]
        } else {
            Vec::new()
        };
        store.reconcile(
            ReconcileAction::Interpret(metadata.id),
            evidence,
            ReconcileInput {
                corrections: &[],
                findings: &findings,
                held_back: Some(HELD),
                mode: ReconcileMode::Standard,
            },
            |_, _| Ok(Vec::new()),
        )?;
        Ok(())
    };
    read(b"held", HELD, true)?;
    read(b"other", "evidence too large", true)?;
    read(b"bare", HELD, false)?;
    let finding = ("mapping_finding".to_owned(), "unknown test".to_owned());
    let failure = |reason: &str| ("interpretation_failure".to_owned(), reason.to_owned());
    assert_eq!(
        open_reasons(&store)?,
        vec![
            finding.clone(),
            finding,
            failure("evidence too large"),
            failure(HELD),
        ]
    );
    Ok(())
}
