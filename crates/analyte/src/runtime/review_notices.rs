//! Tells opted-in lab apps about Needs review items through the outbox, and again when staff settle them (ADR 0038).
//!
//! The notices only inform: the decision stays in Analyte. Each one is built
//! once, frozen in the outbox with its own delivery ID and sent by the lab
//! app's delivery worker like any delivery. A notice never carries a raw
//! message or a value.
use crate::installation::config::Program;
use analyte_contract::{
    ReviewChange, ReviewClosed, ReviewNotice, ReviewNoticeFields, ReviewOpened, ReviewOutcome,
    ReviewReason, Source, Text,
};
use analyte_model::Identifier;
use analyte_store::{
    Audit, Handle, ParkedKind, ReviewNoticeDelivery, ReviewPhase, ReviewSubject, ReviewWork,
};
use std::{sync::Arc, time::Duration};
use tokio::sync::watch;

/// How often the item list is read for new and settled items. Notices are
/// informational, so a few seconds of delay costs nothing.
const INTERVAL: Duration = Duration::from_secs(5);

/// One lab app that takes notices, with the machines that send to it and each
/// machine's model, read once from the program at start.
#[derive(Clone)]
pub(crate) struct Plan {
    destination: Identifier,
    machines: Vec<(Identifier, analyte_contract::document::ModelId)>,
}

/// The lab apps whose `review_notices` setting is on, each with its machines.
pub(crate) fn plans(program: &Program) -> Vec<Plan> {
    let mut plans: Vec<Plan> = Vec::new();
    for machine in program.machines() {
        let Some(settings) = &machine.snapshot().effective().site().destination else {
            continue;
        };
        if !settings.review_notices {
            continue;
        }
        let entry = (machine.id().clone(), machine.model.clone());
        match plans
            .iter_mut()
            .find(|plan| plan.destination == settings.name)
        {
            Some(plan) => plan.machines.push(entry),
            None => plans.push(Plan {
                destination: settings.name.clone(),
                machines: vec![entry],
            }),
        }
    }
    plans
}

/// Records which lab apps take notices, at engine start, before any worker
/// runs (ADR 0038).
/// # Errors
/// Reports a store refusal.
pub(crate) async fn configure(store: &Handle, plans: &[Plan]) -> Result<(), crate::runtime::Error> {
    let destinations: Vec<Identifier> = plans.iter().map(|plan| plan.destination.clone()).collect();
    store
        .request(crate::deadline()?, move |store| {
            store.configure_review_notices(&destinations)
        })
        .await?;
    Ok(())
}

/// Reads and queues notices until stopped. A failed round is logged and tried
/// again next round: an optional notice never stops results.
/// # Errors
/// None today; the result type matches the other supervised owners.
pub(crate) async fn run(
    store: Arc<Handle>,
    plans: Vec<Plan>,
    mut stop: watch::Receiver<bool>,
) -> Result<(), crate::runtime::Error> {
    let mut ticks = tokio::time::interval(INTERVAL);
    ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    while crate::runtime::until_stop(&mut ticks, &mut stop).await {
        for plan in &plans {
            if *stop.borrow() {
                return Ok(());
            }
            if let Err(error) = round(&store, plan).await {
                tracing::warn!(
                    destination = plan.destination.as_str(),
                    %error,
                    "Needs review notices were not queued this round; trying again"
                );
            }
        }
    }
    Ok(())
}

/// Why a round of notices did not finish.
#[derive(Debug, thiserror::Error)]
pub(crate) enum RoundError {
    #[error(transparent)]
    Deadline(#[from] crate::DeadlineError),
    #[error(transparent)]
    Store(#[from] analyte_store::DeliveryError),
}

/// Reads what one lab app should be told and queues it, in one write.
async fn round(store: &Handle, plan: &Plan) -> Result<usize, RoundError> {
    let destination = plan.destination.clone();
    let machines: Vec<Identifier> = plan.machines.iter().map(|(id, _)| id.clone()).collect();
    let work = store
        .request(crate::deadline()?, move |store| {
            store.review_notice_work(&destination, &machines)
        })
        .await?;
    let Some(work) = work else {
        return Ok(0);
    };
    let scanned = work.scanned;
    let notices = build(plan, work, jiff::Timestamp::now());
    let destination = plan.destination.clone();
    Ok(store
        .request(crate::deadline()?, move |store| {
            store.enqueue_review_notices(&destination, &notices, scanned)
        })
        .await?)
}

/// The notices for `work`. An item whose kind, machine model or wording this
/// version cannot state gets no notice; it stays in Needs review as always.
fn build(plan: &Plan, work: ReviewWork, now: jiff::Timestamp) -> Vec<ReviewNoticeDelivery> {
    let opened = work
        .opened
        .into_iter()
        .map(|subject| (subject, ReviewPhase::Opened));
    let closed = work
        .closed
        .into_iter()
        .map(|subject| (subject, ReviewPhase::Closed));
    opened
        .chain(closed)
        .filter_map(
            |(subject, phase)| match notice(plan, &subject, phase, now) {
                Ok(notice) => notice,
                Err(error) => {
                    tracing::warn!(
                        destination = plan.destination.as_str(),
                        item = subject.item,
                        %error,
                        "a Needs review notice could not be built; the item stays in Needs review"
                    );
                    None
                }
            },
        )
        .collect()
}

/// Why one notice could not be built.
#[derive(Debug, thiserror::Error)]
enum NoticeError {
    #[error(transparent)]
    Document(#[from] analyte_contract::DocumentError),
    #[error(transparent)]
    Text(#[from] analyte_contract::document::TextError),
    #[error(transparent)]
    Encode(#[from] analyte_sink::DeliveryError),
    #[error("a stored time is outside the supported range")]
    Time(#[from] jiff::Error),
}

/// One notice for `subject`, or none when this version cannot say it.
fn notice(
    plan: &Plan,
    subject: &ReviewSubject,
    phase: ReviewPhase,
    now: jiff::Timestamp,
) -> Result<Option<ReviewNoticeDelivery>, NoticeError> {
    let Some((_, model)) = plan.machines.iter().find(|(id, _)| *id == subject.machine) else {
        return Ok(None);
    };
    let change = match (phase, &subject.resolution) {
        (ReviewPhase::Opened, _) => {
            let Some(reason) = reason(subject) else {
                return Ok(None);
            };
            ReviewChange::Opened(ReviewOpened {
                reason,
                sample_id: subject
                    .sample
                    .as_ref()
                    .map(analyte_contract::document::SampleId::from),
                tests: subject.tests.iter().map(Into::into).collect(),
                parked_at: subject
                    .parked_at
                    .map(|at| jiff::Timestamp::from_second(at).map(|at| at.to_string()))
                    .transpose()?
                    .map(Text::try_from)
                    .transpose()?,
            })
        }
        (ReviewPhase::Closed, Some(resolution)) => ReviewChange::Closed(ReviewClosed {
            outcome: outcome(resolution),
            resolved_at: Text::try_from(
                jiff::Timestamp::from_second(resolution.time)?.to_string(),
            )?,
        }),
        (ReviewPhase::Closed, None) => return Ok(None),
    };
    let text = |value: &str| Text::try_from(value.to_owned());
    let machine = subject.machine.as_str();
    let notice = ReviewNotice::try_from(ReviewNoticeFields {
        contract_version: 1,
        delivery_id: text(&uuid::Uuid::now_v7().to_string())?,
        created_at: text(&now.to_string())?,
        source: Source {
            analyzer: text(machine)?,
            // An order or a machine's question names no connection; like an
            // order event, the notice names the machine there.
            connection: text(
                subject
                    .connection
                    .as_ref()
                    .map_or(machine, Identifier::as_str),
            )?,
            model: model.clone(),
        },
        item: analyte_contract::document::Identifier::new(&subject.item.to_string())?,
        notice: change,
    })?;
    Ok(Some(analyte_sink::encode_review_notice(
        &notice,
        plan.destination.clone(),
        subject.item,
        phase,
    )?))
}

/// The reason category of an open item, from its stored kind and reason; none
/// for a kind this version does not know.
fn reason(subject: &ReviewSubject) -> Option<ReviewReason> {
    let stored = subject.details.reason.as_str();
    Some(
        match ParkedKind::from_stored(subject.details.kind.as_str())? {
            ParkedKind::ResultReconciliation if stored == analyte_store::ASK_REPEAT_REASON => {
                ReviewReason::RepeatedResult
            }
            ParkedKind::ResultReconciliation => match analyte_store::reconciled_outcome(stored) {
                Some(analyte_store::RevisionOutcome::Unresolved) => {
                    ReviewReason::UnresolvedIdentity
                }
                Some(analyte_store::RevisionOutcome::Conflict) => ReviewReason::ResultConflict,
                Some(
                    analyte_store::RevisionOutcome::New(_)
                    | analyte_store::RevisionOutcome::Retransmission(_)
                    | analyte_store::RevisionOutcome::Correction(_),
                )
                | None => ReviewReason::ResultReview,
            },
            ParkedKind::MappingFinding => ReviewReason::MappingFinding,
            ParkedKind::InterpretationFailure => ReviewReason::ParseFailed,
            ParkedKind::IncompleteTransfer => ReviewReason::IncompleteTransfer,
            ParkedKind::DeliveryRejected => ReviewReason::DeliveryRefused,
            ParkedKind::DeliveryConflict => ReviewReason::DeliveryConflict,
            ParkedKind::CancelConflict => ReviewReason::CancelConflict,
            ParkedKind::PollIntent => ReviewReason::OrderQueryUncertain,
            ParkedKind::OrderEventModelUnknown => ReviewReason::OrderEventModelUnknown,
            // The lab app's own order feed, not a machine: it traces to no machine.
            ParkedKind::OrderVersionRefused => return None,
        },
    )
}

/// How staff settled an item, from the audited action that resolved it.
fn outcome(resolution: &Audit) -> ReviewOutcome {
    match resolution.action.as_str() {
        analyte_store::RESEND_MEASUREMENT_ACTION => ReviewOutcome::Resend,
        analyte_store::RERUN_MEASUREMENT_ACTION => ReviewOutcome::Rerun,
        analyte_store::SUPERSEDE_SAMPLE_ACTION => ReviewOutcome::Supersede,
        crate::operator::recovery::REPROCESS_ACTION => ReviewOutcome::Reprocess,
        crate::operator::recovery::RESOLVE_ACTION => ReviewOutcome::Dismissed,
        // Settled by another audited action, such as a machine's question
        // being answered: the item is settled, nothing more is claimed.
        _ => ReviewOutcome::Resolved,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use analyte_model::{NamespacedId, Reason, SampleId};
    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn plan() -> Result<Plan, Box<dyn std::error::Error>> {
        Ok(Plan {
            destination: Identifier::new("lab")?,
            machines: vec![(
                Identifier::new("bench-1")?,
                analyte_contract::document::ModelId::new("example", "synthetic-astm")?,
            )],
        })
    }

    fn subject(kind: &str, reason: &str) -> Result<ReviewSubject, Box<dyn std::error::Error>> {
        Ok(ReviewSubject {
            item: 42,
            details: analyte_store::Parked {
                kind: Identifier::new(kind)?,
                reason: Reason::new(reason)?,
                targets: vec![Identifier::new("019ab000-0000-7000-8000-000000000010")?],
            },
            parked_at: Some(1_794_744_000),
            machine: Identifier::new("bench-1")?,
            connection: Some(Identifier::new("port-1")?),
            sample: Some(SampleId::new("000Ab12")?),
            tests: vec![NamespacedId {
                namespace: Identifier::new("synthetic")?,
                value: Identifier::new("GLU")?,
            }],
            resolution: None,
        })
    }

    fn built(
        subject: &ReviewSubject,
        phase: ReviewPhase,
    ) -> Result<Option<(ReviewNoticeDelivery, serde_json::Value)>, Box<dyn std::error::Error>> {
        let Some(notice) = notice(&plan()?, subject, phase, jiff::Timestamp::UNIX_EPOCH)? else {
            return Ok(None);
        };
        let decoded: ReviewNotice = analyte_contract::decode(&notice.payload)?;
        let value = serde_json::to_value(decoded.fields())?;
        Ok(Some((notice, value)))
    }

    // ADR 0038; AGENTS.md safety rule 10: a notice names the item, the
    // machine, the sample and the test codes, and never a raw message or a
    // value, not even one quoted in the stored reason.
    #[test]
    fn a_notice_carries_no_raw_message_or_value() -> TestResult {
        let held = subject("result_reconciliation", "result reconciliation: Unresolved")?;
        let mut quoted = subject("mapping_finding", "value 7.4 mmol/L in OBX|5 not mapped")?;
        quoted.item = 43;
        for subject in [held, quoted] {
            let (delivery, value) = built(&subject, ReviewPhase::Opened)?.ok_or("a notice")?;
            let text = String::from_utf8(delivery.payload.clone())?;
            for leaked in [
                "7.4",
                "mmol",
                "OBX",
                "result reconciliation",
                "019ab000-0000-7000-8000-000000000010",
            ] {
                assert!(!text.contains(leaked), "{leaked} in {text}");
            }
            assert_eq!(value["item"], subject.item.to_string());
            assert_eq!(value["source"]["analyzer"], "bench-1");
            assert_eq!(value["source"]["connection"], "port-1");
            assert_eq!(value["source"]["model"], "example/synthetic-astm");
            let opened = &value["notice"]["opened"];
            assert_eq!(opened["sample_id"], "000Ab12");
            assert_eq!(opened["tests"][0]["value"], "GLU");
            assert_eq!(opened["parked_at"], "2026-11-15T12:00:00Z");
            assert_eq!(
                value["delivery_id"],
                uuid::Uuid::from(delivery.id).to_string()
            );
        }
        Ok(())
    }

    // ADR 0038: the reason category is read from the stored kind and reason,
    // from a closed list; a kind this version does not know gets no notice.
    #[test]
    fn reasons_come_from_the_stored_kind_and_never_a_guess() -> TestResult {
        for (kind, reason, expected) in [
            (
                "result_reconciliation",
                "result reconciliation: Unresolved",
                Some("unresolved_identity"),
            ),
            (
                "result_reconciliation",
                analyte_store::ASK_REPEAT_REASON,
                Some("repeated_result"),
            ),
            (
                "result_reconciliation",
                "result reconciliation: Conflict",
                Some("result_conflict"),
            ),
            ("interpretation_failure", "unreadable", Some("parse_failed")),
            ("parked", "app said no", Some("delivery_refused")),
            (
                "poll_intent",
                "asked for orders",
                Some("order_query_uncertain"),
            ),
            ("future_kind", "something new", None),
        ] {
            let found = built(&subject(kind, reason)?, ReviewPhase::Opened)?
                .map(|(_, value)| value["notice"]["opened"]["reason"].clone());
            assert_eq!(
                found.as_ref().and_then(serde_json::Value::as_str),
                expected,
                "{kind}: {reason}"
            );
        }
        Ok(())
    }

    // ADR 0038: a closing notice names the same item and how staff settled it.
    #[test]
    fn a_closing_notice_names_the_item_and_the_outcome() -> TestResult {
        for (action, expected) in [
            (analyte_store::RESEND_MEASUREMENT_ACTION, "resend"),
            (analyte_store::RERUN_MEASUREMENT_ACTION, "rerun"),
            (analyte_store::SUPERSEDE_SAMPLE_ACTION, "supersede"),
            (crate::operator::recovery::REPROCESS_ACTION, "reprocess"),
            (crate::operator::recovery::RESOLVE_ACTION, "dismissed"),
            ("poll-intent-confirmed", "resolved"),
        ] {
            let mut settled =
                subject("result_reconciliation", "result reconciliation: Unresolved")?;
            settled.resolution = Some(Audit {
                actor: Identifier::new("operator")?,
                action: Identifier::new(action)?,
                targets: Vec::new(),
                time: 1_794_745_800,
                reason: Reason::new("checked")?,
            });
            let (_, value) = built(&settled, ReviewPhase::Closed)?.ok_or("a notice")?;
            assert_eq!(value["item"], "42");
            assert_eq!(value["notice"]["closed"]["outcome"], expected, "{action}");
            assert_eq!(
                value["notice"]["closed"]["resolved_at"],
                "2026-11-15T12:30:00Z"
            );
        }
        Ok(())
    }

    // ADR 0038: an item from a machine this lab app's plan does not name gets
    // no notice, so no model is ever guessed.
    #[test]
    fn an_unknown_machine_gets_no_notice() -> TestResult {
        let mut other = subject("interpretation_failure", "unreadable")?;
        other.machine = Identifier::new("removed")?;
        assert!(built(&other, ReviewPhase::Opened)?.is_none());
        Ok(())
    }
}
