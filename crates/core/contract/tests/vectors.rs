//! Tests the published vectors added for the contract checker against the contract types.
#![forbid(unsafe_code)]
use analyte_contract::*;
use serde_json::Value;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn json(bytes: &[u8]) -> Result<Value, serde_json::Error> {
    serde_json::from_slice(bytes)
}

fn text<'a>(value: &'a Value, field: &str) -> Result<&'a str, Box<dyn std::error::Error>> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing {field}").into())
}

// Contract v1 "HTTP and authentication": unknown fields are ignored at every level
#[test]
fn unknown_fields_vector_is_accepted() -> TestResult {
    let extended: Delivery = decode(include_bytes!(
        "../../../../docs/contract/vectors/unknown-fields.json"
    ))?;
    let plain: Delivery = decode(include_bytes!(
        "../../../../docs/contract/vectors/delivery.json"
    ))?;
    assert_eq!(
        serde_json::to_value(extended)?,
        serde_json::to_value(plain)?
    );
    Ok(())
}

// Contract v1 "POST /results": each hierarchy node keeps its one-based occurrence
#[test]
fn occurrence_vector_keeps_positions() -> TestResult {
    let delivery: Delivery = decode(include_bytes!(
        "../../../../docs/contract/vectors/source-path-occurrence.json"
    ))?;
    let result = delivery.fields().results.first().ok_or("result")?;
    let part = result.parts.as_slice().first().ok_or("part")?;
    let occurrences: Vec<u64> = part
        .source_path
        .as_slice()
        .iter()
        .map(|node| node.occurrence)
        .collect();
    assert_eq!(occurrences, [1, 3, 5]);
    Ok(())
}

// Contract v1 "POST /results": every invalid-documents.json entry is refused, and only
// because of its named change (the unchanged delivery vector is valid)
#[test]
fn invalid_document_vectors_are_refused() -> TestResult {
    let entries = json(include_bytes!(
        "../../../../docs/contract/vectors/invalid-documents.json"
    ))?;
    let entries = entries.as_array().ok_or("entries")?;
    assert!(entries.len() >= 7);
    for entry in entries {
        let name = text(entry, "name")?;
        assert!(!text(entry, "rule")?.is_empty(), "{name}");
        let bytes = serde_json::to_vec(entry.get("document").ok_or("document")?)?;
        assert!(decode::<Delivery>(&bytes).is_err(), "{name} was accepted");
    }
    Ok(())
}

// Contract v1 "HTTP and authentication" (Standard Webhooks 1.0.0): 300 seconds either way
#[test]
fn signature_window_vector_bounds_timestamps() -> TestResult {
    let signed = json(include_bytes!(
        "../../../../docs/contract/vectors/signature.json"
    ))?;
    let window = json(include_bytes!(
        "../../../../docs/contract/vectors/signature-window.json"
    ))?;
    let secret = SigningSecret::parse(text(&signed, "secret")?)?;
    let sent: i64 = text(&signed, "timestamp")?.parse()?;
    for case in window.as_array().ok_or("cases")? {
        let offset = case
            .get("offset_seconds")
            .and_then(Value::as_i64)
            .ok_or("offset")?;
        let verified = secret.verify(
            text(&signed, "id")?,
            text(&signed, "timestamp")?,
            text(&signed, "body")?.as_bytes(),
            text(&signed, "signature")?,
            sent.checked_add(offset).ok_or("overflow")?,
        );
        assert_eq!(
            verified.is_ok(),
            text(case, "expected")? == "accept",
            "{offset}"
        );
    }
    Ok(())
}

// Contract v1 "HTTP and authentication": any matching candidate is accepted during rotation
#[test]
fn rotated_signature_vector_is_accepted() -> TestResult {
    let signed = json(include_bytes!(
        "../../../../docs/contract/vectors/signature.json"
    ))?;
    let rotated = json(include_bytes!(
        "../../../../docs/contract/vectors/rotated-signature.json"
    ))?;
    assert_eq!(text(&rotated, "expected")?, "accept");
    let header = text(&rotated, "signature_header")?;
    let (id, timestamp, body) = (
        text(&signed, "id")?,
        text(&signed, "timestamp")?,
        text(&signed, "body")?.as_bytes(),
    );
    let now: i64 = timestamp.parse()?;
    SigningSecret::parse(text(&signed, "secret")?)?.verify(id, timestamp, body, header, now)?;
    // The other candidate alone is from a secret this receiver does not trust.
    let other = header.split_whitespace().next().ok_or("candidate")?;
    let other_secret = SigningSecret::parse(text(&rotated, "other_secret")?)?;
    other_secret.verify(id, timestamp, body, other, now)?;
    assert!(
        SigningSecret::parse(text(&signed, "secret")?)?
            .verify(id, timestamp, body, other, now)
            .is_err()
    );
    Ok(())
}

// Contract v1 "HTTP and authentication" and "GET /orders": limits.json matches the types
#[test]
fn limits_vector_matches_contract_bounds() -> TestResult {
    let limits = json(include_bytes!(
        "../../../../docs/contract/vectors/limits.json"
    ))?;
    let number = |field: &str| -> Result<usize, Box<dyn std::error::Error>> {
        Ok(usize::try_from(
            limits.get(field).and_then(Value::as_u64).ok_or("limit")?,
        )?)
    };
    assert_eq!(number("max_document_bytes")?, MAX_DOCUMENT_BYTES);
    let order = json(include_bytes!(
        "../../../../docs/contract/vectors/order.json"
    ))?;
    let page = |orders: usize, cursor: usize| {
        serde_json::to_vec(&serde_json::json!({
            "contract_version": 1,
            "orders": vec![order.clone(); orders],
            "next_cursor": "c".repeat(cursor),
            "complete": true,
        }))
    };
    let (orders, cursor) = (number("max_orders_per_page")?, number("max_cursor_bytes")?);
    assert!(decode::<OrdersPage>(&page(orders, cursor)?).is_ok());
    assert!(decode::<OrdersPage>(&page(orders.saturating_add(1), cursor)?).is_err());
    assert!(decode::<OrdersPage>(&page(orders, cursor.saturating_add(1))?).is_err());
    let mut tests = order.clone();
    tests["tests"] = Value::Array(vec![
        serde_json::json!({"namespace":"synthetic","value":"T"});
        number("max_tests_per_order")?
    ]);
    assert!(decode::<Order>(&serde_json::to_vec(&tests)?).is_ok());
    tests["tests"] = Value::Array(vec![
        serde_json::json!({"namespace":"synthetic","value":"T"});
        number("max_tests_per_order")?.saturating_add(1)
    ]);
    assert!(decode::<Order>(&serde_json::to_vec(&tests)?).is_err());
    assert_eq!(number("signature_tolerance_seconds")?, 300);
    Ok(())
}

// Contract v1 "GET /orders": the seed file is valid, bounded and covers a cancellation
// and a tombstone (which retains ID and version with no tests)
#[test]
fn seed_vector_holds_cancellation_and_tombstone() -> TestResult {
    let limits = json(include_bytes!(
        "../../../../docs/contract/vectors/limits.json"
    ))?;
    let seed: Vec<Order> = decode(include_bytes!(
        "../../../../docs/contract/vectors/seed-orders.json"
    ))?;
    let max = limits
        .get("max_seed_orders")
        .and_then(Value::as_u64)
        .ok_or("max")?;
    assert!(u64::try_from(seed.len())? <= max);
    assert!(seed.iter().any(|order| {
        !order.fields().tombstone && order.fields().state == document::OrderState::CancelRequested
    }));
    let tombstone = seed
        .iter()
        .find(|order| order.fields().tombstone)
        .ok_or("tombstone")?;
    assert!(tombstone.fields().tests.is_empty());
    let mut ids: Vec<&str> = seed
        .iter()
        .map(|order| order.fields().id.as_str())
        .collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), seed.len(), "one entry per order");
    Ok(())
}

// Contract v1 "HTTP and authentication" and "Delivery responses and retries": the
// request vectors only name documents and statuses the contract defines
#[test]
fn request_vectors_name_known_documents() -> TestResult {
    let auth = json(include_bytes!(
        "../../../../docs/contract/vectors/authentication.json"
    ))?;
    for entry in auth.as_array().ok_or("auth")? {
        let statuses = entry
            .get("expected")
            .and_then(Value::as_array)
            .ok_or("expected")?;
        assert!(
            statuses
                .iter()
                .all(|s| matches!(s.as_u64(), Some(401 | 403)))
        );
    }
    let steps = json(include_bytes!(
        "../../../../docs/contract/vectors/idempotency.json"
    ))?;
    let statuses: Vec<u64> = steps
        .as_array()
        .ok_or("steps")?
        .iter()
        .filter_map(|step| step.get("expected_status")?.as_u64())
        .collect();
    assert_eq!(statuses, [200, 200, 409, 409]);
    for step in steps.as_array().ok_or("steps")? {
        if let Some(body) = step.get("expected_body").filter(|body| !body.is_null()) {
            let status: StatusBody = serde_json::from_value(body.clone())?;
            assert_eq!(status.contract_version, 1);
        }
    }
    Ok(())
}
