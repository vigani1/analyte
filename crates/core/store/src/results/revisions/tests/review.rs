//! Tests the atomic operator decision, audit evidence and delivery count.
use crate::results::revisions::review::first;
use crate::test_support::*;
use crate::*;
use analyte_model::*;

fn delivery(
    result: &ResultContent,
    metadata: &ReceiptMetadata,
) -> Result<Delivery, ReconciliationError> {
    let IdentityStatus::Resolved(identity) = &result.identity else {
        return Err(ReconciliationError::InvalidInterpretationInput);
    };
    Ok(Delivery {
        id: uuid::Uuid::now_v7().try_into()?,
        destination: Identifier::new("app").map_err(StorageError::Text)?,
        ordering_key: OrderingKey {
            machine: metadata.machine.clone(),
            observation: identity.observation.clone(),
        },
        payload: serde_json::to_vec(result)?,
        contract_version: 1,
    })
}

fn record(
    store: &mut Store,
    value: &str,
    missing: bool,
) -> Result<ReceiptId, Box<dyn std::error::Error>> {
    let metadata = receipt(b"synthetic")?;
    store.receive(&metadata, b"synthetic", b"config")?;
    let mut result = production_result(metadata.id)?;
    let mut parts = result.parts.as_slice().to_vec();
    let Some(part) = parts.first_mut() else {
        return Err(ReconciliationError::Corruption.into());
    };
    part.values = EvidenceList::new(vec![ResultValue::new(
        SourceText::new(value)?,
        ValueInterpretation::Text,
    )])?;
    part.times = EvidenceList::new(vec![NamedSourceTime {
        role: Identifier::new("R.13")?,
        time: SourceTime {
            original: SourceText::new("20261123123000")?,
            parsed: None,
            offset: None,
            precision: TimePrecision::Second,
        },
    }])?;
    result.parts = NonEmptyEvidenceList::new(EvidenceList::new(parts)?)?;
    let findings = if missing {
        result.sample = None;
        result.identity = IdentityStatus::Unresolved(Reason::new("missing sample identity")?);
        vec![Parked {
            kind: Identifier::new(ParkedKind::MappingFinding.as_str())?,
            reason: Reason::new("Position { record: 3, field: 0 }: MissingSample")?,
            targets: vec![Identifier::new(&uuid::Uuid::from(metadata.id).to_string())?],
        }]
    } else {
        if let IdentityStatus::Resolved(identity) = &mut result.identity {
            identity.observation.run = SourceText::new("6:00012314:20261123123000")?;
        }
        Vec::new()
    };
    let mut evidence = interpretation()?;
    evidence.outcome = Reading::Results(vec![result.clone()]);
    store.reconcile(
        ReconcileAction::Interpret(metadata.id),
        evidence,
        ReconcileInput::standard(&[false], &findings),
        |_, outcomes| {
            if outcomes == [RevisionOutcome::New(Revision(0))] {
                Ok(vec![delivery(&result, &metadata)?])
            } else {
                Ok(Vec::new())
            }
        },
    )?;
    Ok(metadata.id)
}

fn count(store: &Store) -> rusqlite::Result<i64> {
    store
        .db
        .query_row("SELECT count(*) FROM outbox", [], |row| row.get(0))
}

// ADR 0035, operator decisions retain the two receipts and one audit.
#[test]
fn operator_resend_and_rerun_change_delivery_count_only_as_requested() -> TestResult {
    for (decision, expected, action) in [
        (MeasurementDecision::Resend, 1, "parked-measurement-resend"),
        (MeasurementDecision::Rerun, 2, "parked-measurement-rerun"),
    ] {
        let temporary = tempfile::tempdir()?;
        let mut store = open(&temporary.path().join("review.db"))?;
        let prior = record(&mut store, "positive", false)?;
        let held = record(&mut store, "negative", false)?;
        let actor = Identifier::new("operator")?;
        let reason = Reason::new("checked analyzer history")?;
        let view = store
            .measurement_review(1, actor.clone(), 6)?
            .ok_or(ReconciliationError::Corruption)?;
        assert_eq!(first(&view.held)?.receipt, held);
        assert_eq!(first(&view.other)?.receipt, prior);
        assert_eq!(count(&store)?, 1);
        store.decide_measurement(
            MeasurementConfirmation {
                item: 1,
                snapshot: &view.snapshot,
                decision,
                actor: actor.clone(),
                time: 7,
                reason: reason.clone(),
            },
            delivery,
        )?;
        assert_eq!(count(&store)?, expected);
        let resolution = store
            .parked_item(1)?
            .and_then(|item| item.resolution)
            .ok_or(ReconciliationError::Corruption)?;
        let audit: Audit = serde_json::from_str(&resolution)?;
        assert_eq!(audit.action.as_str(), action);
        assert_eq!(audit.actor, actor);
        assert_eq!(audit.reason, reason);
        assert_eq!(audit.time, 7);
        assert!(matches!(
            store.decide_measurement(
                MeasurementConfirmation {
                    item: 1,
                    snapshot: &view.snapshot,
                    decision,
                    actor,
                    time: 8,
                    reason,
                },
                delivery
            ),
            Err(ReconciliationError::ReviewChanged)
        ));
        assert_eq!(count(&store)?, expected);
        if matches!(decision, MeasurementDecision::Rerun) {
            let payload: Vec<u8> = store.db.query_row(
                "SELECT payload FROM outbox WHERE receipt=?1",
                [key(&held)?],
                |row| row.get(0),
            )?;
            let result: ResultContent = serde_json::from_slice(&payload)?;
            let IdentityStatus::Resolved(identity) = result.identity else {
                return Err(ReconciliationError::Corruption.into());
            };
            assert!(identity.observation.run.as_str().starts_with("operator:"));
            assert_eq!(result.parts, first(&view.held)?.parts);
        }
    }
    Ok(())
}

// ADR 0035: confirmation of a missing-sample supersession closes and audits the item.
#[test]
fn learned_sample_only_closes_the_item_after_confirmation() -> TestResult {
    let temporary = tempfile::tempdir()?;
    let mut store = open(&temporary.path().join("sample.db"))?;
    let held = record(&mut store, "positive", true)?;
    let actor = Identifier::new("operator")?;
    assert!(store.measurement_review(1, actor.clone(), 5)?.is_none());
    let later = record(&mut store, "positive", false)?;
    let view = store
        .measurement_review(1, actor.clone(), 6)?
        .ok_or(ReconciliationError::Corruption)?;
    assert!(view.suggestion);
    assert_eq!(
        (first(&view.held)?.receipt, first(&view.other)?.receipt),
        (held, later)
    );
    assert_eq!(store.parked_counts()?.open, 1);
    store.decide_measurement(
        MeasurementConfirmation {
            item: 1,
            snapshot: &view.snapshot,
            decision: MeasurementDecision::Supersede,
            actor,
            time: 7,
            reason: Reason::new("verified tube on analyzer")?,
        },
        delivery,
    )?;
    assert_eq!(store.parked_counts()?.open, 0);
    assert_eq!(count(&store)?, 1);
    let audits = store.audits(0)?;
    let (_, audit) = audits.last().ok_or("missing supersession audit")?;
    assert_eq!(audit.action.as_str(), "parked-sample-superseded");
    Ok(())
}

// ADR 0035: the exact snapshot must still be current at submission time.
#[test]
fn operator_decision_refuses_a_changed_comparison_while_item_is_open() -> TestResult {
    let temporary = tempfile::tempdir()?;
    let mut store = open(&temporary.path().join("stale.db"))?;
    let earlier = record(&mut store, "positive", false)?;
    record(&mut store, "negative", false)?;
    let actor = Identifier::new("operator")?;
    let view = store
        .measurement_review(1, actor.clone(), 6)?
        .ok_or(ReconciliationError::Corruption)?;
    let earlier_reading = store
        .interpretation(earlier, 1)?
        .ok_or(ReconciliationError::Corruption)?;
    store.reconcile(
        ReconcileAction::Interpret(earlier),
        earlier_reading,
        ReconcileInput::standard(&[false], &[]),
        |_, _| Ok(Vec::new()),
    )?;
    assert!(matches!(
        store.decide_measurement(
            MeasurementConfirmation {
                item: 1,
                snapshot: &view.snapshot,
                decision: MeasurementDecision::Rerun,
                actor,
                time: 7,
                reason: Reason::new("checked analyzer history")?,
            },
            delivery
        ),
        Err(ReconciliationError::ReviewChanged)
    ));
    assert_eq!(store.parked_counts()?.open, 1);
    assert_eq!(count(&store)?, 1);
    Ok(())
}

// ADR 0035 (amended 2026-11-27) and AGENTS.md clinical rules 3 and 4: staff
// confirm a held same-key change as a correction. Exactly one delivery
// carries it as revision N+1 of the same result identity; both receipts and
// their readings stay as received, and the audit names actor, time, reason
// and both receipts in the same transaction as the resolution.
#[test]
fn operator_correction_delivers_the_next_revision_of_the_same_result() -> TestResult {
    let temporary = tempfile::tempdir()?;
    let mut store = open(&temporary.path().join("correction.db"))?;
    let prior = record(&mut store, "positive", false)?;
    let held = record(&mut store, "negative", false)?;
    let before = (
        store.receipt(held)?.ok_or("held receipt")?.raw,
        serde_json::to_string(&store.interpretation(held, 1)?.ok_or("held reading")?)?,
        serde_json::to_string(&store.interpretation(prior, 1)?.ok_or("earlier reading")?)?,
    );
    let actor = Identifier::new("operator")?;
    let reason = Reason::new("analyzer corrected the value without a marker")?;
    let view = store
        .measurement_review(1, actor.clone(), 6)?
        .ok_or(ReconciliationError::Corruption)?;
    assert!(view.correctable);
    let IdentityStatus::Resolved(earlier) = &first(&view.other)?.identity else {
        return Err(ReconciliationError::Corruption.into());
    };
    assert_eq!(earlier.revision, Revision(0));
    store.decide_measurement(
        MeasurementConfirmation {
            item: 1,
            snapshot: &view.snapshot,
            decision: MeasurementDecision::Correction,
            actor: actor.clone(),
            time: 7,
            reason: reason.clone(),
        },
        delivery,
    )?;
    assert_eq!(count(&store)?, 2);
    let (payload, ordering): (Vec<u8>, String) = store.db.query_row(
        "SELECT payload,ordering_key FROM outbox WHERE receipt=?1",
        [key(&held)?],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let earlier_ordering: String = store.db.query_row(
        "SELECT ordering_key FROM outbox WHERE receipt=?1",
        [key(&prior)?],
        |row| row.get(0),
    )?;
    assert_eq!(ordering, earlier_ordering);
    let result: ResultContent = serde_json::from_slice(&payload)?;
    let IdentityStatus::Resolved(identity) = &result.identity else {
        return Err(ReconciliationError::Corruption.into());
    };
    assert_eq!(identity.observation, earlier.observation);
    assert_eq!(identity.revision, Revision(1));
    assert_eq!(result.receipt, held);
    assert_eq!(result.parts, first(&view.held)?.parts);
    assert_eq!(
        (
            store.receipt(held)?.ok_or("held receipt")?.raw,
            serde_json::to_string(&store.interpretation(held, 1)?.ok_or("held reading")?)?,
            serde_json::to_string(&store.interpretation(prior, 1)?.ok_or("earlier reading")?)?,
        ),
        before
    );
    let resolution = store
        .parked_item(1)?
        .and_then(|item| item.resolution)
        .ok_or(ReconciliationError::Corruption)?;
    let audit: Audit = serde_json::from_str(&resolution)?;
    assert_eq!(audit.action.as_str(), CORRECTION_MEASUREMENT_ACTION);
    assert_eq!((audit.actor.clone(), audit.time), (actor.clone(), 7));
    assert_eq!(audit.reason, reason);
    let targets: Vec<&str> = audit.targets.iter().map(Identifier::as_str).collect();
    for receipt in [held, prior] {
        assert!(targets.contains(&uuid::Uuid::from(receipt).to_string().as_str()));
    }
    let audits = store.audits(0)?;
    let (_, logged) = audits.last().ok_or("missing correction audit")?;
    assert_eq!(logged.action, audit.action);
    assert_eq!(logged.targets, audit.targets);
    assert!(matches!(
        store.decide_measurement(
            MeasurementConfirmation {
                item: 1,
                snapshot: &view.snapshot,
                decision: MeasurementDecision::Correction,
                actor,
                time: 8,
                reason,
            },
            delivery
        ),
        Err(ReconciliationError::ReviewChanged)
    ));
    assert_eq!(count(&store)?, 2);
    Ok(())
}

// ADR 0035 (amended 2026-11-27): a correction rechecks the displayed snapshot
// like the other decisions; a changed comparison is refused, nothing is
// queued and the item stays open to be reviewed again.
#[test]
fn operator_correction_refuses_a_changed_comparison() -> TestResult {
    let temporary = tempfile::tempdir()?;
    let mut store = open(&temporary.path().join("stale-correction.db"))?;
    let earlier = record(&mut store, "positive", false)?;
    record(&mut store, "negative", false)?;
    let actor = Identifier::new("operator")?;
    let view = store
        .measurement_review(1, actor.clone(), 6)?
        .ok_or(ReconciliationError::Corruption)?;
    let earlier_reading = store
        .interpretation(earlier, 1)?
        .ok_or(ReconciliationError::Corruption)?;
    store.reconcile(
        ReconcileAction::Interpret(earlier),
        earlier_reading,
        ReconcileInput::standard(&[false], &[]),
        |_, _| Ok(Vec::new()),
    )?;
    assert!(matches!(
        store.decide_measurement(
            MeasurementConfirmation {
                item: 1,
                snapshot: &view.snapshot,
                decision: MeasurementDecision::Correction,
                actor: actor.clone(),
                time: 7,
                reason: Reason::new("analyzer corrected the value")?,
            },
            delivery
        ),
        Err(ReconciliationError::ReviewChanged)
    ));
    assert_eq!(store.parked_counts()?.open, 1);
    assert_eq!(count(&store)?, 1);
    let reopened = store
        .measurement_review(1, actor, 8)?
        .ok_or("the item is open with a fresh comparison")?;
    assert_ne!(reopened.snapshot, view.snapshot);
    Ok(())
}

// ADR 0035 (amended 2026-11-27): a missing-sample suggestion is not a held
// change under a retained key, so it offers no correction and refuses one.
#[test]
fn missing_sample_suggestion_is_not_correctable() -> TestResult {
    let temporary = tempfile::tempdir()?;
    let mut store = open(&temporary.path().join("sample-correction.db"))?;
    record(&mut store, "positive", true)?;
    record(&mut store, "positive", false)?;
    let actor = Identifier::new("operator")?;
    let view = store
        .measurement_review(1, actor.clone(), 6)?
        .ok_or(ReconciliationError::Corruption)?;
    assert!(view.suggestion);
    assert!(!view.correctable);
    assert!(matches!(
        store.decide_measurement(
            MeasurementConfirmation {
                item: 1,
                snapshot: &view.snapshot,
                decision: MeasurementDecision::Correction,
                actor,
                time: 7,
                reason: Reason::new("analyzer corrected the value")?,
            },
            delivery
        ),
        Err(ReconciliationError::ReviewChanged)
    ));
    assert_eq!(store.parked_counts()?.open, 1);
    assert_eq!(count(&store)?, 1);
    Ok(())
}
