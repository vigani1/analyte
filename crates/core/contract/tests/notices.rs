//! Tests the Needs review notice body and how Analyte reads a lab app's answer to it (ADR 0038).
use analyte_contract::{
    ResponseAction, ReviewChange, ReviewNotice, classify_notice_response, decode,
};
use serde_json::Value;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn vector(text: &str) -> Result<Value, serde_json::Error> {
    serde_json::from_str(text)
}

// ADR 0038, contract v1 "POST /review-notices": the opened and closed vectors
// decode, name the same item and carry no value.
#[test]
fn notice_vectors_open_and_close_the_same_item() -> TestResult {
    let opened: ReviewNotice = decode(include_bytes!(
        "../../../../docs/contract/vectors/review-notice.json"
    ))?;
    let closed: ReviewNotice = decode(include_bytes!(
        "../../../../docs/contract/vectors/review-notice-closed.json"
    ))?;
    assert_eq!(opened.fields().item, closed.fields().item);
    assert!(matches!(opened.fields().notice, ReviewChange::Opened(_)));
    assert!(matches!(closed.fields().notice, ReviewChange::Closed(_)));
    assert_ne!(opened.fields().delivery_id, closed.fields().delivery_id);
    Ok(())
}

// ADR 0038: a notice is bounded like every body: at most 256 test codes, a
// version 7 delivery ID, offset times and a known reason.
#[test]
fn notices_outside_the_bounds_are_refused() -> TestResult {
    let base = vector(include_str!(
        "../../../../docs/contract/vectors/review-notice.json"
    ))?;
    let test = base["notice"]["opened"]["tests"][0].clone();
    let mut too_many = base.clone();
    too_many["notice"]["opened"]["tests"] = Value::Array(vec![test; 257]);
    let mut wrong_id = base.clone();
    wrong_id["delivery_id"] = "019ab000-0000-4000-8000-000000000020".into();
    let mut local_time = base.clone();
    local_time["notice"]["opened"]["parked_at"] = "2026-11-15T12:00:00".into();
    let mut unknown_reason = base.clone();
    unknown_reason["notice"]["opened"]["reason"] = "auto_verified".into();
    let mut both = base.clone();
    both["notice"]["closed"] =
        serde_json::json!({"outcome": "rerun", "resolved_at": "2026-11-15T12:30:00Z"});
    for (name, document) in [
        ("257 tests", too_many),
        ("version 4 ID", wrong_id),
        ("time without offset", local_time),
        ("unknown reason", unknown_reason),
        ("opened and closed at once", both),
    ] {
        assert!(
            decode::<ReviewNotice>(&serde_json::to_vec(&document)?).is_err(),
            "{name}"
        );
    }
    let mut most = base;
    let test = most["notice"]["opened"]["tests"][0].clone();
    most["notice"]["opened"]["tests"] = Value::Array(vec![test; 256]);
    decode::<ReviewNotice>(&serde_json::to_vec(&most)?)?;
    Ok(())
}

// ADR 0038: 404 and 501 decline notices; every other answer means what it
// means for any delivery, so a wrong key still pauses the lab app.
#[test]
fn notice_answers_follow_the_published_vector() -> TestResult {
    let cases = vector(include_str!(
        "../../../../docs/contract/vectors/review-notice-responses.json"
    ))?;
    for case in cases.as_array().ok_or("cases")? {
        let status = u16::try_from(case["status"].as_u64().ok_or("status")?)?;
        let body = if status == 200 {
            br#"{"contract_version":1,"status":"accepted"}"#.to_vec()
        } else {
            br#"{"contract_version":1,"error":"example","detail":"Synthetic rejection"}"#.to_vec()
        };
        let action = classify_notice_response(status, &body, None);
        let expected = case["expected"].as_str().ok_or("expected")?;
        assert!(
            matches!(
                (expected, &action),
                ("done", ResponseAction::Done(_))
                    | ("declined", ResponseAction::Declined { .. })
                    | ("pause", ResponseAction::PauseForSetup { .. })
                    | ("conflict", ResponseAction::Conflict)
                    | ("parked", ResponseAction::Parked { .. })
                    | ("retry", ResponseAction::Retry { .. })
            ),
            "{status}: expected {expected}, got {action:?}"
        );
    }
    Ok(())
}
