//! Checks result and order-event deliveries: acceptance, retries, conflicts, refusals and the size limit.
use crate::client::{Answer, App, Key, RequestError, Signature};
use crate::{Outcome, Report, RunError, limit, vectors};
use analyte_contract::{DeliveryStatus, ErrorBody, ResponseAction, StatusBody, classify_response};
use serde_json::Value;

const RESULTS: &str = "POST /results";
const EVENTS: &str = "POST /order-events";
const AUTHENTICATION: &str = "HTTP and authentication";
const RESPONSES: &str = "Delivery responses and retries";
const WRONG_KEY: &str = "analyte-contract-check-wrong-key";

/// What a case expects from one answer.
#[derive(Clone, Copy)]
enum Expect<'a> {
    /// HTTP 200 with this durable status.
    Stored(DeliveryStatus),
    /// One of these statuses; the body is not checked.
    Status(&'a [u16]),
    /// 400 or 422 with a `contract_version` 1 error body, so Analyte parks with the app's reason.
    Refused,
}

pub(crate) async fn check(app: &App, report: &mut Report) -> Result<(), RunError> {
    idempotency(app, report).await?;
    for (name, vector, path, rule) in [
        (
            "results.unknown-fields",
            vectors::UNKNOWN_FIELDS,
            "results",
            AUTHENTICATION,
        ),
        (
            "results.source-path-occurrence",
            vectors::SOURCE_PATH_OCCURRENCE,
            "results",
            RESULTS,
        ),
        (
            "order-events.accepted",
            vectors::EVENT,
            "order-events",
            EVENTS,
        ),
    ] {
        let id = fresh_id();
        let body = document(vector, &id, None)?;
        let answer = app
            .post(path, &id, &body, Key::Correct, Signature::Correct)
            .await;
        report.add(
            name,
            rule,
            judge(answer, Expect::Stored(DeliveryStatus::Accepted)),
        );
    }
    invalid_documents(app, report).await?;
    authentication(app, report).await?;
    rotated_signature(app, report).await?;
    size_limit(app, report).await
}

/// Every Retry-After the app sent during the run must be seconds or an HTTP date.
pub(crate) fn check_retry_after(app: &App, report: &mut Report) {
    report.add(
        "retry-after.format",
        RESPONSES,
        retry_after_outcome(&app.retry_after_seen()),
    );
}

/// Judges each value by the contract classifier Analyte's sender uses, so the
/// checker fails exactly the values Analyte ignores.
fn retry_after_outcome(seen: &[String]) -> Outcome {
    if seen.is_empty() {
        Outcome::Skip("the app sent no Retry-After header during this run".into())
    } else if let Some(bad) = seen.iter().find(|value| {
        !matches!(
            classify_response(503, &[], Some(value)),
            ResponseAction::Retry { after: Some(_) }
        )
    }) {
        Outcome::Fail(format!(
            "Retry-After {bad:?} is not delay seconds or an IMF-fixdate of at most 64 characters; Analyte ignores it and uses its own backoff"
        ))
    } else {
        Outcome::Pass
    }
}

async fn idempotency(app: &App, report: &mut Report) -> Result<(), RunError> {
    const FILE: &str = "idempotency.json";
    let steps: Vec<Value> = parse(vectors::IDEMPOTENCY, FILE)?;
    let id = fresh_id();
    for step in &steps {
        let text = |field: &str| step.get(field).and_then(Value::as_str);
        let name = text("step").ok_or(RunError::Vector(FILE))?;
        let (vector, path) = match (text("document"), text("endpoint")) {
            (Some("delivery.json"), Some("results")) => (vectors::DELIVERY, "results"),
            (Some("event.json"), Some("order-events")) => (vectors::EVENT, "order-events"),
            _ => return Err(RunError::Vector(FILE)),
        };
        let body = document(vector, &id, step.get("change").and_then(Value::as_object))?;
        let expected_status = step
            .get("expected_status")
            .and_then(Value::as_u64)
            .and_then(|status| u16::try_from(status).ok())
            .ok_or(RunError::Vector(FILE))?;
        let statuses = [expected_status];
        let expect = match step
            .pointer("/expected_body/status")
            .and_then(Value::as_str)
        {
            Some("accepted") => Expect::Stored(DeliveryStatus::Accepted),
            Some("duplicate") => Expect::Stored(DeliveryStatus::Duplicate),
            Some(_) => return Err(RunError::Vector(FILE)),
            None => Expect::Status(&statuses),
        };
        let answer = app
            .post(path, &id, &body, Key::Correct, Signature::Correct)
            .await;
        report.add(
            format!("idempotency.{name}"),
            RESPONSES,
            judge(answer, expect),
        );
    }
    Ok(())
}

async fn invalid_documents(app: &App, report: &mut Report) -> Result<(), RunError> {
    const FILE: &str = "invalid-documents.json";
    let entries: Vec<Value> = parse(vectors::INVALID_DOCUMENTS, FILE)?;
    let mut cases = vec![(
        "invalid-version.refused".to_owned(),
        vectors::INVALID_VERSION.to_vec(),
    )];
    for entry in &entries {
        let name = entry
            .get("name")
            .and_then(Value::as_str)
            .ok_or(RunError::Vector(FILE))?;
        let document = entry.get("document").ok_or(RunError::Vector(FILE))?;
        let bytes = serde_json::to_vec(document).map_err(|_| RunError::Vector(FILE))?;
        cases.push((format!("invalid-documents.{name}"), bytes));
    }
    for (name, vector) in cases {
        let id = fresh_id();
        let body = document(&vector, &id, None)?;
        let answer = app
            .post("results", &id, &body, Key::Correct, Signature::Correct)
            .await;
        report.add(name, RESULTS, judge(answer, Expect::Refused));
    }
    Ok(())
}

async fn authentication(app: &App, report: &mut Report) -> Result<(), RunError> {
    const FILE: &str = "authentication.json";
    let entries: Vec<Value> = parse(vectors::AUTHENTICATION, FILE)?;
    let invalid: Value = parse(vectors::INVALID_SIGNATURE, "invalid-signature.json")?;
    let bad_signature = invalid
        .get("signature")
        .and_then(Value::as_str)
        .ok_or(RunError::Vector("invalid-signature.json"))?;
    for entry in &entries {
        let name = entry
            .get("name")
            .and_then(Value::as_str)
            .ok_or(RunError::Vector(FILE))?;
        let expected: Vec<u16> = entry
            .get("expected")
            .and_then(Value::as_array)
            .map(|statuses| {
                statuses
                    .iter()
                    .filter_map(|status| u16::try_from(status.as_u64()?).ok())
                    .collect()
            })
            .filter(|statuses: &Vec<u16>| !statuses.is_empty())
            .ok_or(RunError::Vector(FILE))?;
        let request = match name {
            "wrong-api-key" => Some((Key::Other(WRONG_KEY), Signature::Correct)),
            "missing-api-key" => Some((Key::Missing, Signature::Correct)),
            "invalid-signature" => Some((Key::Correct, Signature::Fixed(bad_signature))),
            "missing-signature" => Some((Key::Correct, Signature::Missing)),
            "stale-timestamp" => Some((Key::Correct, Signature::Shifted(-600))),
            "future-timestamp" => Some((Key::Correct, Signature::Shifted(600))),
            "orders-wrong-api-key" | "orders-missing-api-key" => None,
            _ => return Err(RunError::Vector(FILE)),
        };
        let outcome = match request {
            Some((key, signature)) => {
                refused_and_not_stored(app, key, signature, &expected).await?
            }
            None => {
                let key = if name == "orders-wrong-api-key" {
                    Key::Other(WRONG_KEY)
                } else {
                    Key::Missing
                };
                judge(
                    app.orders(&[("limit", "1")], key).await,
                    Expect::Status(&expected),
                )
            }
        };
        report.add(format!("authentication.{name}"), AUTHENTICATION, outcome);
    }
    Ok(())
}

/// The refused request must leave nothing behind: a correct send of it is then new.
async fn refused_and_not_stored(
    app: &App,
    key: Key<'_>,
    signature: Signature<'_>,
    expected: &[u16],
) -> Result<Outcome, RunError> {
    let id = fresh_id();
    let body = document(vectors::DELIVERY, &id, None)?;
    let refused = judge(
        app.post("results", &id, &body, key, signature).await,
        Expect::Status(expected),
    );
    if refused != Outcome::Pass {
        return Ok(refused);
    }
    let correct = app
        .post("results", &id, &body, Key::Correct, Signature::Correct)
        .await;
    Ok(
        match judge(correct, Expect::Stored(DeliveryStatus::Accepted)) {
            Outcome::Pass => Outcome::Pass,
            Outcome::Fail(reason) | Outcome::Skip(reason) => Outcome::Fail(format!(
                "refused as expected, but the same delivery sent correctly then failed: {reason}; a refused request must not be stored"
            )),
        },
    )
}

async fn rotated_signature(app: &App, report: &mut Report) -> Result<(), RunError> {
    const FILE: &str = "rotated-signature.json";
    let vector: Value = parse(vectors::ROTATED_SIGNATURE, FILE)?;
    let other = vector
        .get("other_secret")
        .and_then(Value::as_str)
        .and_then(|secret| analyte_contract::SigningSecret::parse(secret).ok())
        .ok_or(RunError::Vector(FILE))?;
    let id = fresh_id();
    let body = document(vectors::DELIVERY, &id, None)?;
    let answer = app
        .post(
            "results",
            &id,
            &body,
            Key::Correct,
            Signature::Rotated(&other),
        )
        .await;
    report.add(
        "rotated-signature.accepted",
        AUTHENTICATION,
        judge(answer, Expect::Stored(DeliveryStatus::Accepted)),
    );
    Ok(())
}

async fn size_limit(app: &App, report: &mut Report) -> Result<(), RunError> {
    let max = limit("max_document_bytes")?;
    let id = fresh_id();
    let at_limit = padded(&id, max)?;
    let answer = app
        .post("results", &id, &at_limit, Key::Correct, Signature::Correct)
        .await;
    report.add(
        "limits.max-document-bytes",
        AUTHENTICATION,
        match judge(answer, Expect::Stored(DeliveryStatus::Accepted)) {
            Outcome::Fail(reason) => {
                Outcome::Fail(format!("a {max}-byte document must be accepted: {reason}"))
            }
            Outcome::Pass => Outcome::Pass,
            Outcome::Skip(reason) => Outcome::Skip(reason),
        },
    );
    let id = fresh_id();
    let over = padded(&id, max.saturating_add(1))?;
    let outcome = match app.post_oversized("results", &id, &over).await {
        Ok(answer) if matches!(answer.status, 400 | 413 | 422) => Outcome::Pass,
        Ok(answer) => Outcome::Fail(format!(
            "expected HTTP 400, 413 or 422 for a document one byte over the limit; {}",
            answer.describe()
        )),
        // Closing the connection is not acceptance, but no answer was seen either.
        Err(RequestError::Http(_)) => Outcome::Skip(
            "the app closed the connection without an answer; Analyte never sends a larger document".into(),
        ),
        Err(error) => Outcome::Fail(error.to_string()),
    };
    report.add("limits.over-max-document-bytes", AUTHENTICATION, outcome);
    Ok(())
}

/// The delivery vector with an unknown `padding` field sized to exactly `size` bytes.
fn padded(id: &str, size: usize) -> Result<Vec<u8>, RunError> {
    let mut value: Value = parse(vectors::DELIVERY, "delivery.json")?;
    let object = value
        .as_object_mut()
        .ok_or(RunError::Vector("delivery.json"))?;
    object.insert("delivery_id".into(), Value::String(id.to_owned()));
    object.insert("padding".into(), Value::String(String::new()));
    let empty = serde_json::to_vec(&value).map_err(|_| RunError::Vector("delivery.json"))?;
    let fill = size
        .checked_sub(empty.len())
        .ok_or(RunError::Vector("delivery.json"))?;
    if let Some(object) = value.as_object_mut() {
        object.insert("padding".into(), Value::String("x".repeat(fill)));
    }
    let bytes = serde_json::to_vec(&value).map_err(|_| RunError::Vector("delivery.json"))?;
    if bytes.len() == size {
        Ok(bytes)
    } else {
        Err(RunError::Vector("delivery.json"))
    }
}

/// The vector document with a fresh delivery ID and the given top-level changes.
fn document(
    vector: &[u8],
    id: &str,
    change: Option<&serde_json::Map<String, Value>>,
) -> Result<Vec<u8>, RunError> {
    let mut value: Value = parse(vector, "document")?;
    let object = value.as_object_mut().ok_or(RunError::Vector("document"))?;
    object.insert("delivery_id".into(), Value::String(id.to_owned()));
    for (field, new) in change.into_iter().flatten() {
        object.insert(field.clone(), new.clone());
    }
    serde_json::to_vec(&value).map_err(|_| RunError::Vector("document"))
}

fn parse<T: serde::de::DeserializeOwned>(bytes: &[u8], file: &'static str) -> Result<T, RunError> {
    serde_json::from_slice(bytes).map_err(|_| RunError::Vector(file))
}

/// A new delivery ID per run, so a second run against the same app starts fresh.
fn fresh_id() -> String {
    uuid::Uuid::now_v7().to_string()
}

fn judge(answer: Result<Answer, RequestError>, expect: Expect<'_>) -> Outcome {
    let answer = match answer {
        Ok(answer) => answer,
        Err(error) => return Outcome::Fail(error.to_string()),
    };
    match expect {
        Expect::Stored(wanted) => {
            let body = analyte_contract::decode::<StatusBody>(&answer.body).ok();
            if answer.status == 200
                && body.is_some_and(|b| b.contract_version == 1 && b.status == wanted)
            {
                Outcome::Pass
            } else {
                Outcome::Fail(format!(
                    "expected HTTP 200 with contract_version 1 and status {}; {}",
                    status_name(wanted),
                    answer.describe()
                ))
            }
        }
        Expect::Status(wanted) if wanted.contains(&answer.status) => Outcome::Pass,
        Expect::Status(wanted) => Outcome::Fail(format!(
            "expected HTTP {}; {}",
            list(wanted),
            answer.describe()
        )),
        Expect::Refused => {
            let body = analyte_contract::decode::<ErrorBody>(&answer.body).ok();
            match (answer.status, body) {
                (400 | 422, Some(body)) if body.contract_version == 1 => Outcome::Pass,
                (400 | 422, _) => Outcome::Fail(format!(
                    "HTTP {} without a contract_version 1 error body (error, detail); Analyte parks it without the app's reason",
                    answer.status
                )),
                _ => Outcome::Fail(format!(
                    "expected HTTP 400 or 422 with an error body; {}",
                    answer.describe()
                )),
            }
        }
    }
}

fn status_name(status: DeliveryStatus) -> &'static str {
    match status {
        DeliveryStatus::Accepted => "accepted",
        DeliveryStatus::Duplicate => "duplicate",
    }
}

fn list(statuses: &[u16]) -> String {
    statuses
        .iter()
        .map(u16::to_string)
        .collect::<Vec<_>>()
        .join(" or ")
}

#[cfg(test)]
mod tests {
    use super::*;

    // Contract v1 "HTTP and authentication": a document of exactly 1,048,576 bytes is in bounds
    #[test]
    fn padding_hits_the_exact_size() -> Result<(), RunError> {
        let id = fresh_id();
        let bytes = padded(&id, 1_048_576)?;
        assert_eq!(bytes.len(), 1_048_576);
        let delivery: analyte_contract::Delivery =
            analyte_contract::decode(&bytes).map_err(|_| RunError::Vector("padded"))?;
        assert_eq!(delivery.fields().delivery_id.as_str(), id);
        Ok(())
    }

    // Contract v1 "Delivery responses and retries": Retry-After longer than 64
    // characters is malformed, as Analyte's sender treats it; RFC 9110 §5.4.
    #[test]
    fn retry_after_over_64_characters_fails() {
        let at_limit = format!("{}45", "0".repeat(62));
        assert_eq!(retry_after_outcome(&[at_limit]), Outcome::Pass);
        let over = format!("{}45", "0".repeat(63));
        let outcome = retry_after_outcome(&["30".into(), over]);
        assert!(
            matches!(&outcome, Outcome::Fail(text) if text.contains("at most 64 characters")),
            "{outcome:?}"
        );
    }

    // Contract v1 "Delivery responses and retries": 400 or 422 must carry the versioned error body
    #[test]
    fn refusal_without_error_body_fails() {
        let bare = Answer {
            status: 422,
            retry_after: None,
            body: Vec::new(),
        };
        assert!(matches!(judge(Ok(bare), Expect::Refused), Outcome::Fail(_)));
        let body = br#"{"contract_version":1,"error":"invalid","detail":"bad field"}"#.to_vec();
        let full = Answer {
            status: 422,
            retry_after: None,
            body,
        };
        assert_eq!(judge(Ok(full), Expect::Refused), Outcome::Pass);
    }
}
