//! Tests input limits and clinical data retention.
#![forbid(unsafe_code)]
use analyte_contract::*;
use serde_json::{Value, json};
type TestResult = Result<(), Box<dyn std::error::Error>>;
fn fixture(name: &str) -> Result<Value, serde_json::Error> {
    serde_json::from_str(match name {
        "delivery" => include_str!("../../../../docs/contract/vectors/delivery.json"),
        "event" => include_str!("../../../../docs/contract/vectors/event.json"),
        _ => include_str!("../../../../docs/contract/vectors/order.json"),
    })
}
// App contract v1 (ADR 0008); Safety rule 7 (ADR 0003)
#[test]
fn text_and_document_byte_ceilings_are_inclusive() -> TestResult {
    for n in [1, 4095, 4096] {
        let s = "x".repeat(n);
        assert_eq!(Text::try_from(s.clone())?.as_str(), s);
    }
    for s in [String::new(), "x".repeat(4097), "a\0b".into()] {
        assert!(Text::try_from(s).is_err());
    }
    let mut bytes = b"0".to_vec();
    bytes.resize(MAX_DOCUMENT_BYTES, b' ');
    assert_eq!(decode::<u8>(&bytes)?, 0);
    bytes.push(b' ');
    assert!(matches!(decode::<u8>(&bytes), Err(DocumentError::TooLarge)));
    Ok(())
}
// App contract v1 "HTTP and authentication": the bearer key is 1 to 4096
// printable ASCII characters; a TAB or other control character is refused.
#[test]
fn api_key_is_bounded_printable_ascii() {
    for accepted in ["k", "key with space", "~!", &"x".repeat(4096)] {
        assert!(api_key_valid(accepted), "{accepted:?}");
    }
    for refused in ["", "a\tb", "a\nb", "a\u{7f}", "\u{e9}", &"x".repeat(4097)] {
        assert!(!api_key_valid(refused), "{refused:?}");
    }
}
// App contract v1 (ADR 0008); Safety rule 4 (ADR 0003)
#[test]
fn envelope_ids_times_and_digests_validate_independently() -> TestResult {
    for (path, bad) in [
        (
            "/delivery_id",
            json!("019ab000-0000-4000-8000-000000000001"),
        ),
        (
            "/delivery_id",
            json!("019ab000-0000-7000-0000-000000000001"),
        ),
        ("/created_at", json!("yesterday")),
        ("/receipt/received_at", json!("yesterday")),
        ("/receipt/raw_sha256", json!("a".repeat(63))),
        ("/receipt/raw_sha256", json!("g".repeat(64))),
        ("/receipt/config_sha256", json!("a".repeat(65))),
        ("/receipt/config_sha256", json!("g".repeat(64))),
    ] {
        let mut v = fixture("delivery")?;
        *v.pointer_mut(path).ok_or("path")? = bad;
        assert!(
            decode::<Delivery>(&serde_json::to_vec(&v)?).is_err(),
            "{path}"
        );
    }
    let mut e = fixture("event")?;
    e["occurred_at"] = json!("yesterday");
    assert!(decode::<OrderEvent>(&serde_json::to_vec(&e)?).is_err());
    Ok(())
}
// App contract v1 (ADR 0008); Safety rule 7 (ADR 0003)
#[test]
fn lists_orders_and_cursor_accept_exact_limits() -> TestResult {
    let base = fixture("delivery")?;
    for n in [0, 1, 1000, 1001] {
        let mut v = base.clone();
        v["results"] = json!(vec![base["results"][0].clone(); n]);
        assert_eq!(
            decode::<Delivery>(&serde_json::to_vec(&v)?).is_ok(),
            (1..=1000).contains(&n)
        );
    }
    for n in [0, 1, 100, 101] {
        for tombstone in [false, true] {
            let mut o = fixture("order")?;
            o["tests"] = json!(vec![o["tests"][0].clone(); n]);
            o["tombstone"] = json!(tombstone);
            assert_eq!(
                decode::<Order>(&serde_json::to_vec(&o)?).is_ok(),
                n <= 100 && (n > 0 || tombstone)
            );
        }
        let mut o = fixture("order")?;
        o["target_analyzers"] = json!(vec!["machine"; n]);
        assert_eq!(
            decode::<Order>(&serde_json::to_vec(&o)?).is_ok(),
            (1..=100).contains(&n)
        );
        let page = json!({"contract_version":1,"orders":vec![fixture("order")?;n],"next_cursor":"cursor","complete":true});
        assert_eq!(
            decode::<OrdersPage>(&serde_json::to_vec(&page)?).is_ok(),
            n <= 100
        );
    }
    for n in [256, 257] {
        let page =
            json!({"contract_version":1,"orders":[],"next_cursor":"x".repeat(n),"complete":true});
        assert_eq!(
            decode::<OrdersPage>(&serde_json::to_vec(&page)?).is_ok(),
            n == 256
        );
    }
    let mut o = fixture("order")?;
    o["expires_at"] = json!("yesterday");
    assert!(decode::<Order>(&serde_json::to_vec(&o)?).is_err());
    assert!(
        decode::<OrdersPage>(
            br#"{"contract_version":2,"orders":[],"next_cursor":"x","complete":true}"#
        )
        .is_err()
    );
    Ok(())
}
// App contract v1 (ADR 0008)
#[test]
fn credentials_compare_exact_bytes() {
    assert!(credential_matches(b"secret", b"secret"));
    assert!(!credential_matches(b"secret", b"secreu"));
    assert!(!credential_matches(b"secret", b"secret "));
}
// App contract v1 (ADR 0008); Safety rule 5 (ADR 0003)
#[test]
fn decimal_accessors_preserve_precision() -> TestResult {
    for (s, scale) in [("+001.2300", 4), ("-2", 0), ("1.00", 2)] {
        let d = document::Decimal::new(s)?;
        assert_eq!(d.as_str(), s);
        assert_eq!(d.scale(), scale);
    }
    Ok(())
}
// App contract v1 (ADR 0008); Safety rules 6 and 2 (ADR 0003)
#[test]
fn unit_and_unsupported_accessors_keep_evidence() -> TestResult {
    let units: document::Units = serde_json::from_value(
        json!({"original":" mg/dL ","mapping":{"source":" mg/dL ","target":"mg/dL"}}),
    )?;
    assert_eq!(units.original_text().as_str(), " mg/dL ");
    let mapping = units.mapping().ok_or("mapping lost")?;
    assert_eq!(mapping.source.as_str(), " mg/dL ");
    assert_eq!(mapping.target.as_str(), "mg/dL");
    let plain = document::Units::original(document::SourceText::new("x")?);
    assert!(plain.mapping().is_none());
    for (v, expected) in [
        (json!("text"), false),
        (
            json!({"unsupported":{"source_type":"ZZ","reason":"uninterpreted"}}),
            true,
        ),
        (json!({"numeric":{"number":"1","comparator":null}}), false),
    ] {
        let interpretation: document::ValueInterpretation = serde_json::from_value(v)?;
        assert_eq!(interpretation.is_unsupported(), expected);
    }
    Ok(())
}
// App contract v1 (ADR 0008)
#[test]
fn evidence_list_reports_type_and_keeps_parts() -> TestResult {
    let error =
        decode::<document::EvidenceList<u8>>(b"false").expect_err("boolean is not evidence list");
    assert!(error.to_string() == "invalid document");
    let DocumentError::Json(cause) = error else {
        return Err("wrong error boundary".into());
    };
    assert!(cause.to_string().contains("at most 256 evidence entries"));
    let parts = document::NonEmptyEvidenceList::new(document::EvidenceList::new(vec![1, 2])?)?;
    assert_eq!(parts.as_slice(), [1, 2]);
    Ok(())
}
// App contract v1 (ADR 0008)
#[test]
fn delivery_responses_keep_reason_version_and_retry() {
    for status in [400, 422] {
        assert_eq!(
            classify_response(
                status,
                br#"{"contract_version":1,"error":"bad","detail":"specific reason"}"#,
                None
            ),
            ResponseAction::Parked {
                reason: "specific reason".into()
            }
        );
        assert_eq!(
            classify_response(
                status,
                br#"{"contract_version":2,"error":"bad","detail":"specific reason"}"#,
                None
            ),
            ResponseAction::Parked {
                reason: "invalid error response".into()
            }
        );
    }
    for status in [408, 429, 500, 599] {
        for (s, after) in [("", None), ("-1", None), ("+1", None), ("00012", Some(12))] {
            assert_eq!(
                classify_response(status, b"", Some(s)),
                ResponseAction::Retry { after }
            );
        }
    }
    for status in [199, 201, 499, 600] {
        assert_eq!(
            classify_response(status, b"", None),
            ResponseAction::Parked {
                reason: format!("unexpected HTTP status {status}")
            }
        );
    }
}
// ADR 0008; docs/contract/v1.md delivery responses: setup errors pause the destination.
#[test]
fn setup_errors_pause_destination() {
    for status in [301, 302, 307, 308, 404, 413] {
        assert!(matches!(
            classify_response(status, b"", None),
            ResponseAction::PauseForSetup { .. }
        ));
    }
}
// App contract v1 (ADR 0008)
#[test]
fn success_requires_version_one() {
    assert_eq!(
        classify_response(200, br#"{"contract_version":2,"status":"accepted"}"#, None),
        ResponseAction::Parked {
            reason: "invalid success response".into()
        }
    );
}
// App contract v1 (ADR 0008); Safety rule 7 (ADR 0003)
#[test]
fn retry_header_byte_limit_is_inclusive() {
    for (n, after) in [(64, Some(0)), (65, None)] {
        assert_eq!(
            classify_response(503, b"", Some(&"0".repeat(n))),
            ResponseAction::Retry { after }
        );
    }
}
// App contract v1 (ADR 0008)
#[test]
fn signing_secret_and_metadata_bounds_are_independent() -> TestResult {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    for n in [23, 24, 64, 65] {
        assert_eq!(
            SigningSecret::parse(&format!("whsec_{}", STANDARD.encode(vec![0; n]))).is_ok(),
            (24..=64).contains(&n)
        );
    }
    let key = SigningSecret::parse(&format!("whsec_{}", STANDARD.encode([0; 24])))?;
    for (id, time, body) in [
        ("".into(), "1".into(), vec![]),
        ("x".repeat(257), "1".into(), vec![]),
        ("a.b".into(), "1".into(), vec![]),
        ("id".into(), "0".repeat(21), vec![]),
        ("id".into(), "".into(), vec![]),
        ("id".into(), "+1".into(), vec![]),
        ("id".into(), "1".into(), vec![0; MAX_DOCUMENT_BYTES + 1]),
    ] {
        assert!(matches!(
            key.sign(&id, &time, &body),
            Err(SignatureError::Metadata)
        ));
    }
    assert!(
        key.sign(
            &"x".repeat(256),
            &"0".repeat(20),
            &vec![0; MAX_DOCUMENT_BYTES]
        )
        .is_ok()
    );
    let sig = key.sign("id", "1", b"body")?;
    assert!(matches!(
        key.verify("id", "1", b"body", &format!("{sig}{}", " ".repeat(4097)), 1),
        Err(SignatureError::Metadata)
    ));
    Ok(())
}
// App contract v1 (ADR 0008); Safety rule 7 (ADR 0003)
#[test]
fn signature_header_accepts_exactly_4096_bytes() -> TestResult {
    let key = SigningSecret::parse(analyte_fixtures::SECRET)?;
    let sig = key.sign("id", "1", b"body")?;
    for n in [4096, 4097] {
        let header = format!("{sig}{}", " ".repeat(n - sig.len()));
        assert_eq!(
            key.verify("id", "1", b"body", &header, 1).is_ok(),
            n == 4096
        );
    }
    Ok(())
}

// ADR 0032: source.model is a vendor/model folder key, each part 1 to 64
// lower-case letters, digits or hyphens; nothing is tidied or guessed, and a
// delivery or event without one is refused.
#[test]
fn model_ids_are_vendor_model_keys() -> TestResult {
    use analyte_contract::document::ModelId;
    assert_eq!(
        ModelId::new("roche", "cobas-e411")?.as_str(),
        "roche/cobas-e411"
    );
    assert_eq!(ModelId::parse("yhlo/iflash-1200")?.vendor(), "yhlo");
    let longest = format!("{}/{}", "a".repeat(64), "9".repeat(64));
    assert_eq!(ModelId::parse(&longest)?.as_str(), longest);
    let too_long = format!("{}/x", "a".repeat(65));
    for bad in [
        "",
        "roche",
        "roche/",
        "/cobas",
        "Roche/cobas",
        "roche/cobas e411",
        "roche/cobas/e411",
        "roche/cobas_e411",
        too_long.as_str(),
    ] {
        assert!(ModelId::parse(bad).is_err(), "{bad}");
        assert!(
            serde_json::from_value::<ModelId>(json!(bad)).is_err(),
            "{bad}"
        );
    }
    for name in ["delivery", "event"] {
        let mut document = fixture(name)?;
        document["source"]
            .as_object_mut()
            .ok_or("source")?
            .remove("model");
        let bytes = serde_json::to_vec(&document)?;
        if name == "delivery" {
            assert!(decode::<Delivery>(&bytes).is_err());
        } else {
            assert!(decode::<OrderEvent>(&bytes).is_err());
        }
    }
    Ok(())
}
