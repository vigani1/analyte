//! Tests allowed lifecycle transitions and retained states.
use crate::test_support::*;
use crate::*;

#[test]
fn adr0011_caller_status_table_is_generic_and_correction_cannot_bypass_it() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("status-table.db"))?;
    let policy = StatusLifecycle {
        transitions: &[("draft", "ready")],
        withheld: &["draft"],
        single_run: &[],
        run: |_| None,
    };
    for (index, status, expected) in [
        (0, "draft", RevisionOutcome::New(analyte_model::Revision(0))),
        (
            1,
            "ready",
            RevisionOutcome::Correction(analyte_model::Revision(1)),
        ),
        (2, "draft", RevisionOutcome::Conflict),
    ] {
        let metadata = receipt(b"synthetic")?;
        store.receive(&metadata, b"synthetic", b"config")?;
        let mut result = production_result(metadata.id)?;
        let mut parts = result.parts.as_slice().to_vec();
        parts
            .first_mut()
            .ok_or(ReconciliationError::Corruption)?
            .source_status = analyte_model::SourceText::new(status)?;
        result.parts =
            analyte_model::NonEmptyEvidenceList::new(analyte_model::EvidenceList::new(parts)?)?;
        if let Some(sample) = &mut result.sample {
            sample.id = SampleId::new(&format!("sample-{index}"))?;
        }
        assert_eq!(policy.may_deliver(&result), status != "draft");
        let mut evidence = interpretation()?;
        evidence.outcome = Reading::Results(vec![result]);
        let commit = store.reconcile(
            ReconcileAction::Interpret(metadata.id),
            evidence,
            ReconcileInput {
                corrections: &[true],
                findings: &[],
                mode: ReconcileMode::Lifecycle(policy),
            },
            |_, _| Ok(vec![]),
        )?;
        assert_eq!(commit.outcomes, [expected]);
    }
    Ok(())
}

#[test]
fn adr0011_latest_status_uses_exact_revision_when_one_batch_advances_twice() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("batch-status.db"))?;
    let policy = StatusLifecycle {
        transitions: &[("draft", "draft"), ("draft", "ready")],
        withheld: &[],
        single_run: &[],
        run: |_| None,
    };
    let make = |receipt,
                status: &str|
     -> Result<analyte_model::ResultContent, Box<dyn std::error::Error>> {
        let mut result = production_result(receipt)?;
        let mut parts = result.parts.as_slice().to_vec();
        parts
            .first_mut()
            .ok_or(ReconciliationError::Corruption)?
            .source_status = analyte_model::SourceText::new(status)?;
        result.parts =
            analyte_model::NonEmptyEvidenceList::new(analyte_model::EvidenceList::new(parts)?)?;
        Ok(result)
    };
    let first = receipt(b"synthetic")?;
    store.receive(&first, b"synthetic", b"config")?;
    let mut evidence = interpretation()?;
    evidence.outcome = Reading::Results(vec![make(first.id, "draft")?, make(first.id, "ready")?]);
    let commit = store.reconcile(
        ReconcileAction::Interpret(first.id),
        evidence,
        ReconcileInput {
            corrections: &[false, false],
            findings: &[],
            mode: ReconcileMode::Lifecycle(policy),
        },
        |_, _| Ok(vec![]),
    )?;
    assert_eq!(
        commit.outcomes,
        [
            RevisionOutcome::New(analyte_model::Revision(0)),
            RevisionOutcome::Correction(analyte_model::Revision(1))
        ]
    );
    let second = receipt(b"synthetic")?;
    store.receive(&second, b"synthetic", b"config")?;
    let mut result = make(second.id, "draft")?;
    result
        .sample
        .as_mut()
        .ok_or(ReconciliationError::Corruption)?
        .id = SampleId::new("changed")?;
    let mut evidence = interpretation()?;
    evidence.outcome = Reading::Results(vec![result]);
    let commit = store.reconcile(
        ReconcileAction::Interpret(second.id),
        evidence,
        ReconcileInput {
            corrections: &[false],
            findings: &[],
            mode: ReconcileMode::Lifecycle(policy),
        },
        |_, _| Ok(vec![]),
    )?;
    assert_eq!(commit.outcomes, [RevisionOutcome::Conflict]);
    Ok(())
}

#[test]
fn adr0011_withheld_delivery_builder_is_refused_atomically() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("withheld.db"))?;
    let metadata = receipt(b"synthetic")?;
    store.receive(&metadata, b"synthetic", b"config")?;
    let mut evidence = interpretation()?;
    evidence.outcome = Reading::Results(vec![production_result(metadata.id)?]);
    let policy = StatusLifecycle {
        transitions: &[],
        withheld: &["F"],
        single_run: &[],
        run: |_| None,
    };
    let result = store.reconcile(
        ReconcileAction::Interpret(metadata.id),
        evidence,
        ReconcileInput {
            corrections: &[false],
            findings: &[],
            mode: ReconcileMode::Lifecycle(policy),
        },
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
                id: DeliveryId::try_from(uuid::Uuid::now_v7())
                    .map_err(ReconciliationError::Identity)?,
                destination: Identifier::new("app").map_err(ReconciliationError::Text)?,
                ordering_key: OrderingKey {
                    machine: metadata.machine.clone(),
                    observation: identity.observation.clone(),
                },
                payload: b"must not deliver".to_vec(),
                contract_version: 1,
            }])
        },
    );
    assert!(matches!(
        result,
        Err(ReconciliationError::InvalidInterpretationInput)
    ));
    for table in ["interpretations", "observation_revisions", "outbox"] {
        assert_eq!(
            store
                .db
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                    .get::<_, i64>(0))?,
            0
        );
    }
    Ok(())
}

// ADR 0003; POCT01-A2 B 3.2 pp.71-72: provenance offsets must not create results.
#[test]
fn mutation_adr0011_single_final_run_is_scoped_by_machine_order_and_test() -> TestResult {
    use analyte_model::{IdentityStatus, Revision, SourceText};
    let policy = StatusLifecycle {
        transitions: &[],
        withheld: &[],
        single_run: &["F"],
        run: |id| (!id.run.as_str().is_empty()).then(|| id.run.as_str().to_owned()),
    };
    for case in [
        "same",
        "other-run",
        "other-machine",
        "other-order",
        "other-test",
        "preliminary",
        "missing-run",
    ] {
        let temp = tempfile::tempdir()?;
        let mut store = open(&temp.path().join("single-final.db"))?;
        for index in 0..2 {
            store.limits.page_rows = 1;
            let mut metadata = receipt(b"raw")?;
            if index == 1 && case == "other-machine" {
                metadata.machine = Identifier::new("other-machine")?;
            }
            store.receive(&metadata, b"raw", b"config")?;
            let mut result = production_result(metadata.id)?;
            let IdentityStatus::Resolved(id) = &mut result.identity else {
                return Err("missing identity".into());
            };
            id.observation.run = SourceText::new(if index == 0 || case == "same" {
                "run-a"
            } else if case == "missing-run" {
                ""
            } else {
                "run-b"
            })?;
            if index == 1 && case == "other-order" {
                id.observation.order.value = Identifier::new("other-order")?;
            }
            if index == 1 && case == "other-test" {
                id.observation.code.value = Identifier::new("other-test")?;
            }
            if index == 1 && case == "preliminary" {
                let mut parts = result.parts.as_slice().to_vec();
                let part = parts.first_mut().ok_or("missing part")?;
                part.source_status = SourceText::new("P")?;
                part.status = analyte_model::ResultStatus::Preliminary;
                result.parts = analyte_model::NonEmptyEvidenceList::new(
                    analyte_model::EvidenceList::new(parts)?,
                )?;
            }
            let mut evidence = interpretation()?;
            evidence.outcome = Reading::Results(vec![result]);
            let commit = store.reconcile(
                ReconcileAction::Interpret(metadata.id),
                evidence,
                ReconcileInput {
                    corrections: &[false],
                    findings: &[],
                    mode: ReconcileMode::Lifecycle(policy),
                },
                |_, _| Ok(vec![]),
            )?;
            let expected = if index == 0 {
                RevisionOutcome::New(Revision(0))
            } else {
                match case {
                    "same" => RevisionOutcome::Retransmission(Revision(0)),
                    "other-run" | "missing-run" => RevisionOutcome::Conflict,
                    "other-machine" | "other-order" | "other-test" | "preliminary" => {
                        RevisionOutcome::New(Revision(0))
                    }
                    _ => unreachable!("fixed case list"),
                }
            };
            assert_eq!(
                commit.outcomes,
                vec![expected],
                "case {case}, receipt {index}"
            );
        }
    }
    Ok(())
}

#[test]
fn mutation_adr0003_adr0011_retained_interpretation_byte_bound_is_inclusive() -> TestResult {
    use analyte_model::{IdentityStatus, Revision, SourceText};
    for (exclusive, delta) in [(false, 0), (false, 1), (true, 0), (true, 1)] {
        let temp = tempfile::tempdir()?;
        let mut store = open(&temp.path().join("retained-bound.db"))?;
        let first = receipt(b"raw")?;
        store.receive(&first, b"raw", b"config")?;
        let mut original = production_result(first.id)?;
        original.sample.as_mut().ok_or("missing sample")?.id = SampleId::new(&"s".repeat(64))?;
        let IdentityStatus::Resolved(id) = &mut original.identity else {
            return Err("missing identity".into());
        };
        id.observation.run = SourceText::new("run-a")?;
        let mut evidence = interpretation()?;
        evidence.outcome = Reading::Results(vec![original]);
        store.reconcile(
            ReconcileAction::Interpret(first.id),
            evidence,
            ReconcileInput::standard(&[false], &[]),
            |_, _| Ok(vec![]),
        )?;
        let encoded: String =
            store
                .db
                .query_row("SELECT evidence FROM interpretations", [], |r| r.get(0))?;
        let second = receipt(b"raw")?;
        store.receive(&second, b"raw", b"config")?;
        let mut result = production_result(second.id)?;
        result.sample.as_mut().ok_or("missing sample")?.id = SampleId::new("s")?;
        let IdentityStatus::Resolved(id) = &mut result.identity else {
            return Err("missing identity".into());
        };
        id.observation.run = SourceText::new(if exclusive { "run-b" } else { "run-a" })?;
        let mut evidence = interpretation()?;
        evidence.outcome = Reading::Results(vec![result]);
        store.limits.record_bytes = encoded.len().checked_sub(delta).ok_or("limit")?;
        let result = if exclusive {
            store.reconcile(
                ReconcileAction::Interpret(second.id),
                evidence,
                ReconcileInput {
                    corrections: &[false],
                    findings: &[],
                    mode: ReconcileMode::Lifecycle(StatusLifecycle {
                        transitions: &[],
                        withheld: &[],
                        single_run: &["F"],
                        run: |id| Some(id.run.as_str().to_owned()),
                    }),
                },
                |_, _| Ok(vec![]),
            )
        } else {
            store.reconcile(
                ReconcileAction::Interpret(second.id),
                evidence,
                ReconcileInput::standard(&[true], &[]),
                |_, _| Ok(vec![]),
            )
        };
        if delta == 0 {
            let expected = if exclusive {
                RevisionOutcome::Conflict
            } else {
                RevisionOutcome::Correction(Revision(1))
            };
            assert_eq!(result?.outcomes, vec![expected]);
        } else {
            assert!(matches!(result, Err(ReconciliationError::Corruption)));
            assert!(store.interpretation(second.id, 1)?.is_none());
        }
        assert_eq!(
            store.db.query_row(
                "SELECT evidence FROM interpretations WHERE receipt=?1",
                [key(&first.id)?],
                |r| r.get::<_, String>(0)
            )?,
            encoded
        );
    }
    Ok(())
}

#[test]
fn mutation_adr0011_progression_uses_the_requested_observation_and_unanimous_source_status()
-> TestResult {
    use analyte_model::{IdentityStatus, Revision, SourceText};
    for mixed_after in [false, true] {
        let temp = tempfile::tempdir()?;
        let mut store = open(&temp.path().join("progression-selection.db"))?;
        let policy = StatusLifecycle {
            transitions: &[("draft", "ready")],
            withheld: &[],
            single_run: &[],
            run: |_| None,
        };
        let first = receipt(b"raw")?;
        store.receive(&first, b"raw", b"config")?;
        let make = |owner,
                    statuses: &[&str]|
         -> Result<analyte_model::ResultContent, Box<dyn std::error::Error>> {
            let mut result = production_result(owner)?;
            let template = result
                .parts
                .as_slice()
                .first()
                .ok_or("missing part")?
                .clone();
            let mut parts = vec![];
            for status in statuses {
                let mut part = template.clone();
                part.source_status = SourceText::new(status)?;
                parts.push(part);
            }
            result.parts =
                analyte_model::NonEmptyEvidenceList::new(analyte_model::EvidenceList::new(parts)?)?;
            Ok(result)
        };
        let mut other = make(first.id, &["ready"])?;
        let IdentityStatus::Resolved(id) = &mut other.identity else {
            return Err("missing identity".into());
        };
        id.observation.code.value = Identifier::new("other-test")?;
        let wanted = make(first.id, &["draft", "draft"])?;
        let mut evidence = interpretation()?;
        evidence.outcome = Reading::Results(vec![other, wanted]);
        store.reconcile(
            ReconcileAction::Interpret(first.id),
            evidence,
            ReconcileInput {
                corrections: &[false, false],
                findings: &[],
                mode: ReconcileMode::Lifecycle(policy),
            },
            |_, _| Ok(vec![]),
        )?;
        let second = receipt(b"raw")?;
        store.receive(&second, b"raw", b"config")?;
        let mut evidence = interpretation()?;
        evidence.outcome = Reading::Results(vec![make(
            second.id,
            if mixed_after {
                &["ready", "draft"]
            } else {
                &["ready", "ready"]
            },
        )?]);
        let commit = store.reconcile(
            ReconcileAction::Interpret(second.id),
            evidence,
            ReconcileInput {
                corrections: &[false],
                findings: &[],
                mode: ReconcileMode::Lifecycle(policy),
            },
            |_, _| Ok(vec![]),
        )?;
        assert_eq!(
            commit.outcomes,
            vec![if mixed_after {
                RevisionOutcome::Conflict
            } else {
                RevisionOutcome::Correction(Revision(1))
            }]
        );
    }
    Ok(())
}
