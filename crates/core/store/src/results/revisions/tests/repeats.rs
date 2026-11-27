//! Tests the site's repeated-results choice: automatic or always ask (ADR 0035, 2026-11-26).
use crate::results::revisions::review::first;
use crate::test_support::*;
use crate::*;
use analyte_model::*;

/// A recipe run: length-prefixed source fields, as definitions write them.
const RUN: &str = "6:00012314:20261123123000";

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

/// One upload of one result with `value` under `run`, delivering what is new
/// or corrected as the host does; returns its receipt and outcome.
fn upload(
    store: &mut Store,
    (value, run, correction): (&str, &str, bool),
    repeats: RepeatPolicy,
) -> Result<(ReceiptId, RevisionOutcome), Box<dyn std::error::Error>> {
    let metadata = receipt(b"synthetic")?;
    store.receive(&metadata, b"synthetic", b"config")?;
    let mut result = production_result(metadata.id)?;
    let mut parts = result.parts.as_slice().to_vec();
    let part = parts.first_mut().ok_or(ReconciliationError::Corruption)?;
    part.values = EvidenceList::new(vec![ResultValue::new(
        SourceText::new(value)?,
        ValueInterpretation::Text,
    )])?;
    result.parts = NonEmptyEvidenceList::new(EvidenceList::new(parts)?)?;
    if let IdentityStatus::Resolved(identity) = &mut result.identity {
        identity.observation.run = SourceText::new(run)?;
    }
    let mut evidence = interpretation()?;
    evidence.outcome = Reading::Results(vec![result]);
    let commit = store.reconcile(
        ReconcileAction::Interpret(metadata.id),
        evidence,
        ReconcileInput {
            mode: ReconcileMode::Standard(repeats),
            ..ReconcileInput::standard(&[correction], &[])
        },
        |assigned, outcomes| {
            let Reading::Results(results) = &assigned.outcome else {
                return Ok(Vec::new());
            };
            results
                .iter()
                .zip(outcomes)
                .filter(|(_, outcome)| {
                    matches!(
                        outcome,
                        RevisionOutcome::New(_) | RevisionOutcome::Correction(_)
                    )
                })
                .map(|(result, _)| delivery(result, &metadata))
                .collect()
        },
    )?;
    let outcome = *commit
        .outcomes
        .first()
        .ok_or(ReconciliationError::Corruption)?;
    Ok((metadata.id, outcome))
}

fn deliveries(store: &Store) -> rusqlite::Result<i64> {
    store
        .db
        .query_row("SELECT count(*) FROM outbox", [], |row| row.get(0))
}

/// The open item parked about `held`, with the reason its result carries.
fn held_item(store: &Store, held: ReceiptId) -> Result<i64, Box<dyn std::error::Error>> {
    let target = uuid::Uuid::from(held).to_string();
    let item = store
        .parked(0)?
        .into_iter()
        .filter(|item| item.resolution.is_none())
        .find(|item| item.evidence.contains(&target))
        .ok_or("no open item for the held upload")?;
    let reading = store
        .interpretation(held, 1)?
        .ok_or(ReconciliationError::Corruption)?;
    let Reading::Results(results) = reading.outcome else {
        return Err(ReconciliationError::Corruption.into());
    };
    assert!(matches!(
        &first(&results)?.identity,
        IdentityStatus::Unresolved(reason) if reason.as_str() == ASK_REPEAT_REASON
    ));
    Ok(item.id)
}

// ADR 0035 (2026-11-26): with "always ask me" a byte-identical resend, a
// correction and a same-key change are each held, and none is delivered.
#[test]
fn always_ask_holds_every_repeat_without_a_delivery() -> TestResult {
    let temporary = tempfile::tempdir()?;
    let mut store = open(&temporary.path().join("ask.db"))?;
    let ask = RepeatPolicy::AlwaysAsk;
    let (_, first_seen) = upload(&mut store, ("positive", RUN, false), ask)?;
    assert_eq!(first_seen, RevisionOutcome::New(Revision(0)));
    assert_eq!(deliveries(&store)?, 1);
    for (label, repeat) in [
        ("resend", ("positive", RUN, false)),
        ("correction", ("negative", RUN, true)),
        ("same-key change", ("borderline", RUN, false)),
    ] {
        let (held, outcome) = upload(&mut store, repeat, ask)?;
        assert_eq!(outcome, RevisionOutcome::Conflict, "{label}");
        assert_eq!(deliveries(&store)?, 1, "{label}");
        held_item(&store, held)?;
    }
    assert_eq!(store.parked_counts()?.open, 3);
    Ok(())
}

// ADR 0035 (2026-11-26): "always ask me" leaves a first-seen key flowing normally.
#[test]
fn always_ask_still_delivers_a_first_seen_key() -> TestResult {
    let temporary = tempfile::tempdir()?;
    let mut store = open(&temporary.path().join("first.db"))?;
    let ask = RepeatPolicy::AlwaysAsk;
    assert_eq!(
        upload(&mut store, ("positive", RUN, false), ask)?.1,
        RevisionOutcome::New(Revision(0))
    );
    assert_eq!(
        upload(
            &mut store,
            ("positive", "6:00012314:20261123124500", false),
            ask
        )?
        .1,
        RevisionOutcome::New(Revision(0))
    );
    assert_eq!(deliveries(&store)?, 2);
    assert_eq!(store.parked_counts()?.open, 0);
    Ok(())
}

// ADR 0035 (2026-11-26): an upload is not a repeat of itself; reading the same
// receipt again under "always ask me" is a retransmission, as before.
#[test]
fn always_ask_does_not_hold_a_receipt_read_again() -> TestResult {
    let temporary = tempfile::tempdir()?;
    let mut store = open(&temporary.path().join("again.db"))?;
    let (id, _) = upload(
        &mut store,
        ("positive", RUN, false),
        RepeatPolicy::AlwaysAsk,
    )?;
    let reading = store
        .interpretation(id, 1)?
        .ok_or(ReconciliationError::Corruption)?;
    let commit = store.reconcile(
        ReconcileAction::Interpret(id),
        reading,
        ReconcileInput {
            mode: ReconcileMode::Standard(RepeatPolicy::AlwaysAsk),
            ..ReconcileInput::standard(&[false], &[])
        },
        |_, _| Ok(Vec::new()),
    )?;
    assert_eq!(
        commit.outcomes,
        [RevisionOutcome::Retransmission(Revision(0))]
    );
    assert_eq!(store.parked_counts()?.open, 0);
    Ok(())
}

// ADR 0035 (2026-11-26) and AGENTS.md clinical rule 4: a repeat held by
// "always ask me" resolves with the existing decisions: a resend adds no
// delivery, a rerun queues exactly one new result.
#[test]
fn held_repeats_resolve_as_resend_or_rerun() -> TestResult {
    for (decision, value, expected) in [
        (MeasurementDecision::Resend, "positive", 1),
        (MeasurementDecision::Rerun, "positive", 2),
        (MeasurementDecision::Resend, "negative", 1),
        (MeasurementDecision::Rerun, "negative", 2),
    ] {
        let temporary = tempfile::tempdir()?;
        let mut store = open(&temporary.path().join("resolve.db"))?;
        let ask = RepeatPolicy::AlwaysAsk;
        let (prior, _) = upload(&mut store, ("positive", RUN, false), ask)?;
        let (held, _) = upload(&mut store, (value, RUN, false), ask)?;
        let item = held_item(&store, held)?;
        let actor = Identifier::new("operator")?;
        let view = store
            .measurement_review(item, actor.clone(), 6)?
            .ok_or("the held repeat has no comparison")?;
        assert_eq!(first(&view.held)?.receipt, held);
        assert_eq!(first(&view.other)?.receipt, prior);
        store.decide_measurement(
            MeasurementConfirmation {
                item,
                snapshot: &view.snapshot,
                decision,
                actor,
                time: 7,
                reason: Reason::new("checked analyzer history")?,
            },
            delivery,
        )?;
        assert_eq!(deliveries(&store)?, expected);
        assert_eq!(store.parked_counts()?.open, 0);
    }
    Ok(())
}

// ADR 0035 (amended 2026-11-27): under "always ask me" a held change can be
// confirmed as a correction, which queues it as the next revision of the same
// result. A byte-identical repeat has nothing to correct: it offers only
// resend and rerun, and a correction is refused without a delivery.
#[test]
fn held_repeats_are_correctable_only_when_the_content_changed() -> TestResult {
    for (value, correctable, expected, remaining) in
        [("negative", true, 2, 0), ("positive", false, 1, 1)]
    {
        let temporary = tempfile::tempdir()?;
        let mut store = open(&temporary.path().join("correct.db"))?;
        let ask = RepeatPolicy::AlwaysAsk;
        upload(&mut store, ("positive", RUN, false), ask)?;
        let (held, _) = upload(&mut store, (value, RUN, false), ask)?;
        let item = held_item(&store, held)?;
        let actor = Identifier::new("operator")?;
        let view = store
            .measurement_review(item, actor.clone(), 6)?
            .ok_or("the held repeat has no comparison")?;
        assert_eq!(view.correctable, correctable, "{value}");
        let decided = store.decide_measurement(
            MeasurementConfirmation {
                item,
                snapshot: &view.snapshot,
                decision: MeasurementDecision::Correction,
                actor,
                time: 7,
                reason: Reason::new("analyzer corrected the value")?,
            },
            delivery,
        );
        assert_eq!(decided.is_ok(), correctable, "{value}");
        assert_eq!(deliveries(&store)?, expected, "{value}");
        assert_eq!(store.parked_counts()?.open, remaining, "{value}");
    }
    Ok(())
}

// ADR 0035: "automatic" keeps the recipe decisions exactly: a resend is kept
// once, a correction advances the revision, a same-key change is held, and a
// new key is a new result.
#[test]
fn automatic_keeps_the_recipe_decisions() -> TestResult {
    let temporary = tempfile::tempdir()?;
    let mut store = open(&temporary.path().join("automatic.db"))?;
    let automatic = RepeatPolicy::Automatic;
    let steps = [
        (
            ("positive", RUN, false),
            RevisionOutcome::New(Revision(0)),
            1,
        ),
        (
            ("positive", RUN, false),
            RevisionOutcome::Retransmission(Revision(0)),
            1,
        ),
        (
            ("negative", RUN, true),
            RevisionOutcome::Correction(Revision(1)),
            2,
        ),
        (("borderline", RUN, false), RevisionOutcome::Conflict, 2),
        (
            ("positive", "6:00012314:20261123124500", false),
            RevisionOutcome::New(Revision(0)),
            3,
        ),
    ];
    for (upload_of, outcome, delivered) in steps {
        assert_eq!(upload(&mut store, upload_of, automatic)?.1, outcome);
        assert_eq!(deliveries(&store)?, delivered);
    }
    assert_eq!(store.parked_counts()?.open, 1);
    Ok(())
}
