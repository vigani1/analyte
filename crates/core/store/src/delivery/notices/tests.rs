//! Tests that each Needs review item gets one opening and one closing notice per lab app, kept under one ID, and that a lab app that declines them stops getting them.
use crate::test_support::*;
use crate::*;
use analyte_model::Reason;

fn app() -> Result<Identifier, analyte_model::TextError> {
    Identifier::new("app")
}

fn machines() -> Result<Vec<Identifier>, analyte_model::TextError> {
    Ok(vec![Identifier::new("synthetic-machine")?])
}

/// A store whose lab app `app` takes notices, holding one result without an
/// identity, which Analyte parks for review.
fn held(path: &std::path::Path) -> Result<(Store, ReceiptId), Box<dyn std::error::Error>> {
    let mut store = open(path)?;
    store.configure_review_notices(&[app()?])?;
    let metadata = receipt(b"held")?;
    store.receive(&metadata, b"held", b"config")?;
    let mut result = production_result(metadata.id)?;
    result.identity = analyte_model::IdentityStatus::Unresolved(Reason::new("no run evidence")?);
    let mut evidence = interpretation()?;
    evidence.outcome = Reading::Results(vec![result]);
    store.reconcile(
        ReconcileAction::Interpret(metadata.id),
        evidence,
        ReconcileInput::standard(&[false], &[]),
        |_, _| Ok(Vec::new()),
    )?;
    Ok((store, metadata.id))
}

fn notice(
    item: i64,
    phase: ReviewPhase,
) -> Result<ReviewNoticeDelivery, Box<dyn std::error::Error>> {
    Ok(ReviewNoticeDelivery {
        id: DeliveryId::try_from(uuid::Uuid::now_v7())?,
        destination: app()?,
        item,
        phase,
        payload: format!("{{\"item\":\"{item}\"}}").into_bytes(),
        contract_version: 1,
    })
}

fn notices(store: &Store) -> rusqlite::Result<i64> {
    store.db.query_row(
        "SELECT count(*) FROM outbox WHERE kind='review_notice'",
        [],
        |r| r.get(0),
    )
}

// ADR 0038; contract v1 "POST /review-notices": a held result is told to an
// opted-in lab app once, and the notice keeps its ID and bytes on every retry,
// as every delivery does (ADR 0008).
#[test]
fn a_held_result_gets_one_notice_with_one_id_on_every_retry() -> TestResult {
    let temp = tempfile::tempdir()?;
    let (mut store, _) = held(&temp.path().join("held.db"))?;
    let work = store
        .review_notice_work(&app()?, &machines()?)?
        .ok_or("the lab app takes notices")?;
    let [subject] = work.opened.as_slice() else {
        return Err(format!("expected one item, got {}", work.opened.len()).into());
    };
    assert!(work.closed.is_empty());
    assert_eq!(subject.details.kind.as_str(), "result_reconciliation");
    assert_eq!(subject.machine.as_str(), "synthetic-machine");
    assert_eq!(
        subject.connection.as_ref().map(Identifier::as_str),
        Some("synthetic-connection")
    );
    assert_eq!(
        subject.sample.as_ref().map(SampleId::as_str),
        Some("000123")
    );
    assert_eq!(
        subject
            .tests
            .iter()
            .map(|test| (test.namespace.as_str(), test.value.as_str()))
            .collect::<Vec<_>>(),
        [("synthetic", "test")]
    );
    let first = notice(subject.item, ReviewPhase::Opened)?;
    let id = first.id;
    let bytes = first.payload.clone();
    assert_eq!(
        store.enqueue_review_notices(&app()?, &[first], work.scanned)?,
        1
    );
    // Read again, the item is not offered; offered anyway, it is not queued twice.
    let again = store
        .review_notice_work(&app()?, &machines()?)?
        .ok_or("the lab app takes notices")?;
    assert!(again.opened.is_empty());
    assert_eq!(
        store.enqueue_review_notices(
            &app()?,
            &[notice(subject.item, ReviewPhase::Opened)?],
            again.scanned
        )?,
        0
    );
    assert_eq!(notices(&store)?, 1);
    let claim = store
        .claim_destination(0, &app()?)?
        .ok_or("a notice is due")?;
    assert_eq!((claim.id, claim.kind), (id, DeliveryKind::ReviewNotice));
    store.attempted(id, &Attempt::RetryAt(5), 0)?;
    let retry = store
        .claim_destination(5, &app()?)?
        .ok_or("the retry is due")?;
    assert_eq!((retry.id, retry.payload), (id, bytes));
    Ok(())
}

// ADR 0038: settling the item queues one closing notice, only once its
// opening notice reached the lab app, and never a second one.
#[test]
fn settling_an_item_queues_one_closing_notice() -> TestResult {
    let temp = tempfile::tempdir()?;
    let (mut store, _) = held(&temp.path().join("settled.db"))?;
    let work = store
        .review_notice_work(&app()?, &machines()?)?
        .ok_or("the lab app takes notices")?;
    let item = work.opened.first().ok_or("one item")?.item;
    let opening = notice(item, ReviewPhase::Opened)?;
    let opening_id = opening.id;
    store.enqueue_review_notices(&app()?, &[opening], work.scanned)?;
    let audit = Audit {
        actor: Identifier::new("operator")?,
        action: Identifier::new("parked-resolve")?,
        targets: vec![Identifier::new(&item.to_string())?],
        time: 120,
        reason: Reason::new("checked on the machine")?,
    };
    store.resolve(item, &audit)?;
    let waiting = store
        .review_notice_work(&app()?, &machines()?)?
        .ok_or("the lab app takes notices")?;
    assert!(
        waiting.closed.is_empty(),
        "the opening notice was not delivered yet"
    );
    store
        .claim_destination(0, &app()?)?
        .ok_or("the opening notice")?;
    store.attempted(opening_id, &Attempt::Done, 1)?;
    let work = store
        .review_notice_work(&app()?, &machines()?)?
        .ok_or("the lab app takes notices")?;
    let [closing] = work.closed.as_slice() else {
        return Err(format!("expected one closing, got {}", work.closed.len()).into());
    };
    assert_eq!(closing.item, item);
    let resolution = closing.resolution.as_ref().ok_or("its resolution")?;
    assert_eq!(
        (resolution.action.as_str(), resolution.time),
        ("parked-resolve", 120)
    );
    assert_eq!(
        store.enqueue_review_notices(
            &app()?,
            &[notice(item, ReviewPhase::Closed)?],
            work.scanned
        )?,
        1
    );
    let after = store
        .review_notice_work(&app()?, &machines()?)?
        .ok_or("the lab app takes notices")?;
    assert!(after.closed.is_empty());
    assert_eq!(
        store.enqueue_review_notices(
            &app()?,
            &[notice(item, ReviewPhase::Closed)?],
            after.scanned
        )?,
        0
    );
    assert_eq!(notices(&store)?, 2);
    Ok(())
}

// ADR 0038: notices are opt-in per lab app. A lab app whose setting is off,
// or another lab app's machines, get none.
#[test]
fn a_lab_app_without_the_setting_gets_no_notices() -> TestResult {
    let temp = tempfile::tempdir()?;
    let (mut store, _) = held(&temp.path().join("off.db"))?;
    store.configure_review_notices(&[Identifier::new("other")?])?;
    assert!(store.review_notice_work(&app()?, &machines()?)?.is_none());
    assert!(store.review_notice_status(&app()?)?.is_none());
    let other = store
        .review_notice_work(&Identifier::new("other")?, &[])?
        .ok_or("the other lab app takes notices")?;
    assert!(
        other.opened.is_empty(),
        "the machine sends to app, not other"
    );
    assert_eq!(notices(&store)?, 0);
    Ok(())
}

// ADR 0038: a lab app that answers 404 or 501 takes no notices. Its waiting
// notices are declined, nothing more is queued, nothing is parked, and its
// results keep going; the setting asks again at the next start.
#[test]
fn a_declined_notice_stops_notices_and_leaves_results_alone() -> TestResult {
    let temp = tempfile::tempdir()?;
    let (mut store, receipt) = held(&temp.path().join("declined.db"))?;
    let work = store
        .review_notice_work(&app()?, &machines()?)?
        .ok_or("the lab app takes notices")?;
    let item = work.opened.first().ok_or("one item")?.item;
    let first = notice(item, ReviewPhase::Opened)?;
    let first_id = first.id;
    let mut second = notice(item, ReviewPhase::Opened)?;
    second.item = store.park(&Parked {
        kind: Identifier::new(ParkedKind::InterpretationFailure.as_str())?,
        reason: Reason::new("unreadable")?,
        targets: vec![Identifier::new(&uuid::Uuid::from(receipt).to_string())?],
    })?;
    store.enqueue_review_notices(&app()?, &[first, second], work.scanned)?;
    let result = DeliveryId::try_from(uuid::Uuid::now_v7())?;
    seed_delivery(&store, receipt, result, "synthetic-machine/observation")?;
    let parked_before = store.parked_counts()?;
    let claim = store
        .claim_destination(0, &app()?)?
        .ok_or("the first notice")?;
    assert_eq!(claim.id, first_id);
    store.attempted(
        first_id,
        &Attempt::Declined(Reason::new("no /review-notices endpoint")?),
        7,
    )?;
    assert_eq!(notices(&store)?, 2);
    let declined: i64 = store.db.query_row(
        "SELECT count(*) FROM outbox WHERE kind='review_notice' AND state='declined'",
        [],
        |r| r.get(0),
    )?;
    assert_eq!(declined, 2);
    assert_eq!(store.parked_counts()?, parked_before);
    let status = store
        .review_notice_status(&app()?)?
        .ok_or("the setting is on")?;
    let (at, reason) = status.declined.ok_or("declined")?;
    assert_eq!((at, reason.as_str()), (7, "no /review-notices endpoint"));
    assert!(store.review_notice_work(&app()?, &machines()?)?.is_none());
    // The result is the only thing waiting, and it is still sent.
    assert_eq!(store.undelivered(&app()?)?, 1);
    let next = store.claim_destination(8, &app()?)?.ok_or("the result")?;
    assert_eq!((next.id, next.kind), (result, DeliveryKind::Result));
    // Only a notice can be declined.
    assert!(matches!(
        store.attempted(result, &Attempt::Declined(Reason::new("not a notice")?), 9),
        Err(DeliveryError::InvalidDeliveryInput)
    ));
    // The next start asks again.
    store.configure_review_notices(&[app()?])?;
    assert_eq!(
        store.review_notice_status(&app()?)?,
        Some(ReviewNoticeStatus { declined: None })
    );
    assert!(store.review_notice_work(&app()?, &machines()?)?.is_some());
    Ok(())
}
