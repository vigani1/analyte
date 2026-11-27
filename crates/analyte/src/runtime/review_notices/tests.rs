//! Tests what a Needs review notice says, and that it never carries a raw message or a value.
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
        let mut settled = subject("result_reconciliation", "result reconciliation: Unresolved")?;
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
