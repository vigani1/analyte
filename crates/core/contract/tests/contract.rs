//! Tests versioned integration documents.
#![forbid(unsafe_code)]
use analyte_contract::*;
use serde_json::Value;
// Standard Webhooks Rust library test_sign vector
#[test]
fn signature_matches_published_webhook_vector() -> Result<(), Box<dyn std::error::Error>> {
    // standard-webhooks/libraries/rust/src/lib.rs::tests::test_sign (MIT).
    let key = SigningSecret::parse("whsec_C2FVsBQIhrscChlQIMV+b5sSYspob7oD")?;
    let signature = key.sign(
        "msg_27UH4WbU6Z5A5EzD8u03UvzRbpk",
        "1649367553",
        br#"{"email":"test@example.com","username":"test_user"}"#,
    )?;
    assert_eq!(signature, "v1,tZ1I4/hDygAJgO5TYxiSd6Sd0kDW6hPenDe+bTa3Kkw=");
    Ok(())
}
// ADR 0008
#[test]
fn published_vectors_do_not_drift() -> Result<(), Box<dyn std::error::Error>> {
    let vector: Value = serde_json::from_str(include_str!(
        "../../../../docs/contract/vectors/signature.json"
    ))?;
    let field = |name: &str| -> Result<&str, Box<dyn std::error::Error>> {
        vector
            .get(name)
            .and_then(Value::as_str)
            .ok_or_else(|| "missing vector field".into())
    };
    let key = SigningSecret::parse(field("secret")?)?;
    assert_eq!(
        key.sign(field("id")?, field("timestamp")?, field("body")?.as_bytes())?,
        field("signature")?
    );
    let time = field("timestamp")?.parse::<i64>()?;
    for offset in [-300_i64, 0, 300] {
        key.verify(
            field("id")?,
            field("timestamp")?,
            field("body")?.as_bytes(),
            &format!("v1,bad {}", field("signature")?),
            time.checked_add(offset).ok_or("overflow")?,
        )?;
    }
    for offset in [-301_i64, 301] {
        assert!(
            key.verify(
                field("id")?,
                field("timestamp")?,
                field("body")?.as_bytes(),
                field("signature")?,
                time.checked_add(offset).ok_or("overflow")?
            )
            .is_err()
        );
    }
    assert!(
        key.verify(
            field("id")?,
            field("timestamp")?,
            b"modified",
            field("signature")?,
            time
        )
        .is_err()
    );
    let digest = content_digest(field("body")?.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    assert_eq!(digest, field("digest")?);
    let delivery: Delivery = decode(include_bytes!(
        "../../../../docs/contract/vectors/delivery.json"
    ))?;
    assert_eq!(
        delivery
            .fields()
            .results
            .first()
            .ok_or("result")?
            .sample
            .as_ref()
            .ok_or("sample")?
            .id
            .as_str(),
        "000Ab12"
    );
    let body_delivery: Delivery = decode(field("body")?.as_bytes())?;
    assert_eq!(
        serde_json::to_value(delivery)?,
        serde_json::to_value(body_delivery)?
    );
    assert!(
        decode::<Delivery>(include_bytes!(
            "../../../../docs/contract/vectors/invalid-version.json"
        ))
        .is_err()
    );
    let _: OrderEvent = decode(include_bytes!(
        "../../../../docs/contract/vectors/event.json"
    ))?;
    let _: Order = decode(include_bytes!(
        "../../../../docs/contract/vectors/order.json"
    ))?;
    Ok(())
}
// ADR 0008
#[test]
fn response_vectors_map_to_actions() -> Result<(), Box<dyn std::error::Error>> {
    let vectors: Vec<Value> = serde_json::from_str(include_str!(
        "../../../../docs/contract/vectors/responses.json"
    ))?;
    for vector in vectors {
        let status = u16::try_from(
            vector
                .get("status")
                .and_then(Value::as_u64)
                .ok_or("status")?,
        )?;
        let body = serde_json::to_vec(vector.get("body").ok_or("body")?)?;
        let action = classify_response(status, &body, Some("12"));
        let expected = vector
            .get("expected")
            .and_then(Value::as_str)
            .ok_or("expected")?;
        assert!(matches!(
            (expected, action),
            ("done", ResponseAction::Done(_))
                | ("conflict", ResponseAction::Conflict)
                | ("parked", ResponseAction::Parked { .. })
                | ("pause", ResponseAction::PauseForSetup { .. })
                | ("retry", ResponseAction::Retry { after: Some(12) })
        ));
    }
    assert!(matches!(
        classify_response(200, b"{}", None),
        ResponseAction::Parked { .. }
    ));
    Ok(())
}
proptest::proptest! {
    // ADR 0008
    #[test]
    fn sample_text_is_preserved(value in "[A-Za-z0-9]{1,100}") {
        let text=Text::try_from(value.clone()).map_err(|e|proptest::test_runner::TestCaseError::fail(e.to_string()))?;
        let encoded=serde_json::to_vec(&text).map_err(|e|proptest::test_runner::TestCaseError::fail(e.to_string()))?;
        let decoded:Text=decode(&encoded).map_err(|e|proptest::test_runner::TestCaseError::fail(e.to_string()))?;
        proptest::prop_assert_eq!(decoded.as_str(),value);
    }
}
// ADR 0008
#[test]
fn unknown_fields_and_invalid_decimal_are_handled() -> Result<(), Box<dyn std::error::Error>> {
    let mut document: Value = serde_json::from_slice(include_bytes!(
        "../../../../docs/contract/vectors/delivery.json"
    ))?;
    document["future_field"] = serde_json::json!({"future":true});
    let _: Delivery = decode(&serde_json::to_vec(&document)?)?;
    document["results"][0]["parts"][0]["values"][0]["parsed"]["numeric"]["number"] =
        serde_json::json!("NaN");
    assert!(decode::<Delivery>(&serde_json::to_vec(&document)?).is_err());
    assert!(decode::<Delivery>(&vec![b' '; MAX_DOCUMENT_BYTES.saturating_add(1)]).is_err());
    assert!(Text::try_from(String::new()).is_err());
    Ok(())
}

// ADR 0008
#[test]
fn generated_invalid_request_matches_valid_fixture() -> Result<(), Box<dyn std::error::Error>> {
    let mut valid: Value = serde_json::from_slice(include_bytes!(
        "../../../../docs/contract/vectors/delivery.json"
    ))?;
    valid["contract_version"] = serde_json::json!(2);
    let invalid: Value = serde_json::from_slice(include_bytes!(
        "../../../../docs/contract/vectors/invalid-version.json"
    ))?;
    assert_eq!(valid, invalid);
    Ok(())
}
proptest::proptest! {
    // ADR 0008
    #[test]
    fn hostile_bytes_return_errors_without_panics(bytes in proptest::collection::vec(proptest::prelude::any::<u8>(),0..4096)) {
        let _=decode::<Delivery>(&bytes);
        let _=decode::<OrderEvent>(&bytes);
        let _=decode::<OrdersPage>(&bytes);
    }
}

// ADR 0008
#[test]
fn invalid_signature_fails_and_statuses_classify() -> Result<(), Box<dyn std::error::Error>> {
    let valid: Value = serde_json::from_str(include_str!(
        "../../../../docs/contract/vectors/signature.json"
    ))?;
    let invalid: Value = serde_json::from_str(include_str!(
        "../../../../docs/contract/vectors/invalid-signature.json"
    ))?;
    let field = |v: &Value, name: &str| -> Result<String, Box<dyn std::error::Error>> {
        v.get(name)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| "field".into())
    };
    let secret = SigningSecret::parse(&field(&valid, "secret")?)?;
    assert!(
        secret
            .verify(
                &field(&invalid, "id")?,
                &field(&invalid, "timestamp")?,
                field(&invalid, "body")?.as_bytes(),
                &field(&invalid, "signature")?,
                field(&invalid, "timestamp")?.parse()?
            )
            .is_err()
    );
    assert_eq!(
        classify_response(
            200,
            include_bytes!("../../../../docs/contract/vectors/accepted.json"),
            None
        ),
        ResponseAction::Done(DeliveryStatus::Accepted)
    );
    assert_eq!(
        classify_response(
            200,
            include_bytes!("../../../../docs/contract/vectors/duplicate.json"),
            None
        ),
        ResponseAction::Done(DeliveryStatus::Duplicate)
    );
    Ok(())
}

// ADR 0008
#[test]
fn unknown_processing_stays_unknown_and_cannot_deliver() -> Result<(), Box<dyn std::error::Error>> {
    let wire = document::ProcessingMode::from(&analyte_model::ProcessingMode::Unknown);
    assert_eq!(serde_json::to_string(&wire)?, "\"unknown\"");
    let decoded: document::ProcessingMode = serde_json::from_str("\"unknown\"")?;
    assert!(!decoded.may_deliver_to_production());
    assert!(document::ProcessingMode::Production.may_deliver_to_production());
    Ok(())
}

// ADR 0008
#[test]
fn order_context_reuses_position_and_never_defaults() -> Result<(), Box<dyn std::error::Error>> {
    let minimal: Order = decode(include_bytes!(
        "../../../../docs/contract/vectors/order.json"
    ))?;
    assert!(minimal.fields().position.is_none());
    assert!(minimal.fields().patient.is_none());
    assert!(minimal.fields().specimen_type.is_none());
    let bytes = include_bytes!("../../../../docs/contract/vectors/order-context.json");
    let order: Order = decode(bytes)?;
    let position: &document::SamplePosition = order.fields().position.as_ref().ok_or("position")?;
    assert_eq!(position.slot.as_str(), "03");
    assert_eq!(
        order
            .fields()
            .patient
            .as_ref()
            .ok_or("patient")?
            .id
            .value
            .as_str(),
        "000PatA"
    );
    assert_eq!(
        serde_json::to_value(order)?,
        serde_json::from_slice::<Value>(bytes)?
    );
    for field in ["position", "patient", "specimen_type"] {
        let mut bad: Value = serde_json::from_slice(bytes)?;
        bad[field] = serde_json::json!({});
        assert!(decode::<Order>(&serde_json::to_vec(&bad)?).is_err());
    }
    Ok(())
}

// ADR 0008
#[test]
fn order_context_null_is_unknown_nested_fields_required() -> Result<(), Box<dyn std::error::Error>>
{
    let bytes = include_bytes!("../../../../docs/contract/vectors/order-context.json");
    let mut nulls: Value = serde_json::from_slice(bytes)?;
    for field in ["position", "patient", "specimen_type"] {
        nulls[field] = Value::Null;
    }
    let order: Order = decode(&serde_json::to_vec(&nulls)?)?;
    assert!(order.fields().position.is_none());
    assert!(order.fields().specimen_type.is_none());
    assert!(order.fields().patient.is_none());
    for (object, field) in [
        ("position", "kind"),
        ("position", "slot"),
        ("specimen_type", "namespace"),
        ("specimen_type", "value"),
        ("patient", "id"),
    ] {
        let mut bad: Value = serde_json::from_slice(bytes)?;
        bad[object].as_object_mut().ok_or("object")?.remove(field);
        assert!(decode::<Order>(&serde_json::to_vec(&bad)?).is_err());
    }
    for field in ["namespace", "value"] {
        let mut bad: Value = serde_json::from_slice(bytes)?;
        bad["patient"]["id"]
            .as_object_mut()
            .ok_or("id")?
            .remove(field);
        assert!(decode::<Order>(&serde_json::to_vec(&bad)?).is_err());
    }
    Ok(())
}
