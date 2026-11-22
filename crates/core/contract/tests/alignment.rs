//! Tests model evidence conversion to contract documents.
#![forbid(unsafe_code)]

use analyte_contract::{Delivery, ResultDocument, decode};
use serde_json::{Value, json};

// Safety rule 5 (ADR 0003)
#[test]
fn decoded_text_keeps_original_never_with_unsupported() -> Result<(), Box<dyn std::error::Error>> {
    for bytes in [
        include_bytes!("../../../../docs/contract/vectors/decoded-text.json").as_slice(),
        include_bytes!("../../../../docs/contract/vectors/unsupported-formatting.json").as_slice(),
    ] {
        let document: Value = serde_json::from_slice(bytes)?;
        let expected = document.pointer("/results/0").ok_or("result")?;
        let mut input = expected.clone();
        model_fixture(&mut input);
        let model: analyte_model::ResultContent = serde_json::from_value(input)?;
        assert_eq!(
            serde_json::to_value(ResultDocument::from(&model))?,
            *expected
        );
        let _: Delivery = decode(bytes)?;
    }
    for decoded in [json!(""), json!("A|B"), json!("x".repeat(16384))] {
        let value = json!({"original":r"A\F\B", "parsed":"text", "decoded_text":decoded});
        let contract: analyte_contract::document::ResultValue =
            serde_json::from_value(value.clone())?;
        assert_eq!(serde_json::to_value(contract)?, value);
    }
    for value in [
        json!({"original":"x","parsed":"text","decoded_text":"x".repeat(16385)}),
        json!({"original":"x","parsed":{"unsupported":{"source_type":"FT","reason":"Formatting"}},"decoded_text":"partial"}),
    ] {
        assert!(
            serde_json::from_value::<analyte_contract::document::ResultValue>(value.clone())
                .is_err()
        );
        let mut model_value = value;
        model_fixture(&mut model_value);
        assert!(serde_json::from_value::<analyte_model::ResultValue>(model_value).is_err());
    }
    Ok(())
}

// Model serde spellings are deliberately different from the wire spellings.
// Only fixture enum tokens are translated; production conversion never uses JSON.
fn model_fixture(value: &mut Value) {
    let variants = [
        "resolved",
        "unresolved",
        "numeric",
        "coded",
        "structured_numeric",
        "attachment",
        "unsupported",
        "text",
        "production",
        "training",
        "debug",
        "quality_control",
        "scanned_by_machine",
        "typed_on_machine",
        "work_list",
        "instrument_assigned",
        "unknown",
        "rack",
        "disk",
        "tray",
        "other",
        "patient",
        "control",
        "calibration",
        "pending",
        "preliminary",
        "final",
        "corrected",
        "withdrawn",
        "not_performed",
        "year",
        "month",
        "day",
        "hour",
        "minute",
        "second",
        "fractional_second",
        "less",
        "less_or_equal",
        "greater",
        "greater_or_equal",
        "yes",
        "no",
    ];
    let rename = |text: &str| {
        text.split('_')
            .map(|word| {
                let mut letters = word.chars();
                letters.next().map_or_else(String::new, |first| {
                    first.to_uppercase().to_string() + letters.as_str()
                })
            })
            .collect::<String>()
    };
    match value {
        Value::String(text) if variants.contains(&text.as_str()) => *text = rename(text),
        Value::Object(fields) => {
            let old = std::mem::take(fields);
            for (key, mut value) in old {
                model_fixture(&mut value);
                fields.insert(
                    if [
                        "resolved",
                        "unresolved",
                        "numeric",
                        "coded",
                        "structured_numeric",
                        "attachment",
                        "unsupported",
                        "other",
                    ]
                    .contains(&key.as_str())
                    {
                        rename(&key)
                    } else {
                        key
                    },
                    value,
                );
            }
        }
        Value::Array(items) => {
            for item in items {
                model_fixture(item);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

/// The receipt the published delivery vectors describe.
fn vector_receipt(
    id: analyte_model::ReceiptId,
) -> Result<analyte_model::ReceiptMetadata, Box<dyn std::error::Error>> {
    Ok(analyte_model::ReceiptMetadata {
        id,
        raw_digest: analyte_model::Digest([0; 32]),
        connection: analyte_model::Identifier::new("bench-1")?,
        machine: analyte_model::Identifier::new("synthetic-1")?,
        driver: analyte_model::NamespacedId {
            namespace: analyte_model::Identifier::new("synthetic")?,
            value: analyte_model::Identifier::new("1")?,
        },
        config_version: analyte_model::Identifier::new("1")?,
        config_digest: analyte_model::Digest([1; 32]),
        received_at: "2026-11-15T11:59:59Z".parse()?,
    })
}

// ADR 0008
#[test]
fn published_result_vectors_convert_without_loss() -> Result<(), Box<dyn std::error::Error>> {
    for bytes in [
        include_bytes!("../../../../docs/contract/vectors/delivery.json").as_slice(),
        include_bytes!("../../../../docs/contract/vectors/unresolved-identity.json").as_slice(),
        include_bytes!("../../../../docs/contract/vectors/missing-sample.json").as_slice(),
        include_bytes!("../../../../docs/contract/vectors/multi-part-coded.json").as_slice(),
        include_bytes!("../../../../docs/contract/vectors/withdrawn.json").as_slice(),
    ] {
        let document: Value = serde_json::from_slice(bytes)?;
        let expected = document.pointer("/results/0").ok_or("result")?;
        let mut input = expected.clone();
        model_fixture(&mut input);
        let model: analyte_model::ResultContent = serde_json::from_value(input)?;
        let wire = ResultDocument::from(&model);
        assert_eq!(serde_json::to_value(&wire)?, *expected);
        let _: Delivery = decode(bytes)?;
        let receipt = vector_receipt(model.receipt)?;
        let id = analyte_model::DeliveryId::try_from(uuid::Uuid::parse_str(
            "019ab000-0000-7000-8000-000000000001",
        )?)?;
        // ADR 0032: the vectors name the synthetic example definition's folder.
        let machine = analyte_contract::document::ModelId::new("example", "synthetic-astm")?;
        let delivery = Delivery::from_result(
            &model,
            &receipt,
            &machine,
            id,
            "2026-11-15T12:00:00Z".parse()?,
        )?;
        assert_eq!(serde_json::to_value(delivery)?, document);
        // Contract v1 document ceiling includes all retained evidence (rule 7).
        let mut large = model.clone();
        let mut parts = large.parts.as_slice().to_vec();
        let part = parts.first_mut().ok_or("part")?;
        part.comments = analyte_model::EvidenceList::new(vec![
            analyte_model::SourceText::new(
                &"x".repeat(16384)
            )?;
            65
        ])?;
        large.parts =
            analyte_model::NonEmptyEvidenceList::new(analyte_model::EvidenceList::new(parts)?)?;
        assert!(matches!(
            Delivery::from_result(&large, &receipt, &machine, id, receipt.received_at),
            Err(analyte_contract::DocumentError::TooLarge)
        ));
        let mut parts = model.parts.as_slice().to_vec();
        parts.first_mut().ok_or("part")?.comments = analyte_model::EvidenceList::new(vec![
                analyte_model::SourceText::new(
                    &"x".repeat(16384)
                )?;
                63
            ])?;
        let mut exact = model.clone();
        exact.parts = analyte_model::NonEmptyEvidenceList::new(analyte_model::EvidenceList::new(
            parts.clone(),
        )?)?;
        let initial = Delivery::from_result(&exact, &receipt, &machine, id, receipt.received_at)?;
        let length = serde_json::to_vec(&initial)?.len();
        let padding = analyte_contract::MAX_DOCUMENT_BYTES
            .checked_sub(length)
            .and_then(|n| n.checked_sub(3))
            .ok_or("padding")?;
        let mut comments = parts.first().ok_or("part")?.comments.as_slice().to_vec();
        comments.push(analyte_model::SourceText::new(&"x".repeat(padding))?);
        parts.first_mut().ok_or("part")?.comments =
            analyte_model::EvidenceList::new(comments.clone())?;
        exact.parts = analyte_model::NonEmptyEvidenceList::new(analyte_model::EvidenceList::new(
            parts.clone(),
        )?)?;
        let at_limit = Delivery::from_result(&exact, &receipt, &machine, id, receipt.received_at)?;
        assert_eq!(
            serde_json::to_vec(&at_limit)?.len(),
            analyte_contract::MAX_DOCUMENT_BYTES
        );
        comments.pop();
        comments.push(analyte_model::SourceText::new(
            &"x".repeat(padding.checked_add(1).ok_or("padding overflow")?),
        )?);
        parts.first_mut().ok_or("part")?.comments = analyte_model::EvidenceList::new(comments)?;
        exact.parts =
            analyte_model::NonEmptyEvidenceList::new(analyte_model::EvidenceList::new(parts)?)?;
        assert!(matches!(
            Delivery::from_result(&exact, &receipt, &machine, id, receipt.received_at),
            Err(analyte_contract::DocumentError::TooLarge)
        ));
        let mut mismatch = receipt;
        mismatch.id = analyte_model::ReceiptId::try_from(uuid::Uuid::now_v7())?;
        assert!(
            Delivery::from_result(&model, &mismatch, &machine, id, mismatch.received_at).is_err()
        );
    }
    Ok(())
}

// ADR 0008
#[test]
fn selected_interpretations_and_source_text_survive() -> Result<(), Box<dyn std::error::Error>> {
    let mut document: Value = serde_json::from_slice(include_bytes!(
        "../../../../docs/contract/vectors/delivery.json"
    ))?;
    let values = json!([
        {"original":"", "parsed":"text"},
        {"original":"+001.2300", "parsed":{"numeric":{"number":"+001.2300","comparator":null}}},
        {"original":"1:2", "parsed":{"structured_numeric":{"comparator":"greater_or_equal","first":"1","separator":":","second":"2"}}},
        {"original":"encoded", "parsed":{"attachment":{"media_type":"application/pdf","encoding":"base64","data":"AA=="}}},
        {"original":"error\u{0000}", "parsed":{"unsupported":{"source_type":"ZZ","reason":"Unsupported source type"}}},
        {"original":"x".repeat(16384), "parsed":"text"}
    ]);
    document["results"][0]["parts"][0]["values"] = values;
    let expected = document.pointer("/results/0").ok_or("result")?;
    let mut input = expected.clone();
    model_fixture(&mut input);
    let model: analyte_model::ResultContent = serde_json::from_value(input)?;
    assert_eq!(
        serde_json::to_value(ResultDocument::from(&model))?,
        *expected
    );
    let _: Delivery = decode(&serde_json::to_vec(&document)?)?;
    Ok(())
}

// ADR 0008
#[test]
fn invalid_evidence_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let base: Value = serde_json::from_slice(include_bytes!(
        "../../../../docs/contract/vectors/delivery.json"
    ))?;
    for (path, invalid) in [
        ("/results/0/parts", json!([])),
        ("/results/0/parts/0/flags", json!(vec![""; 257])),
        ("/results/0/parts/0/test/namespace", json!("")),
        (
            "/results/0/parts/0/units/mapping/source",
            json!("different"),
        ),
        ("/results/0/parts/0/times/0/time/offset", json!(93600)),
        (
            "/results/0/receipt",
            json!("019ab000-0000-7000-8000-000000000099"),
        ),
    ] {
        let mut invalid_document = base.clone();
        *invalid_document.pointer_mut(path).ok_or("fixture path")? = invalid;
        assert!(
            decode::<Delivery>(&serde_json::to_vec(&invalid_document)?).is_err(),
            "{path}"
        );
    }
    Ok(())
}

// ADR 0008
#[test]
fn modes_statuses_and_provenance_variants_survive() -> Result<(), Box<dyn std::error::Error>> {
    let base: Value = serde_json::from_slice(include_bytes!(
        "../../../../docs/contract/vectors/delivery.json"
    ))?;
    for (path, variants) in [
        (
            "/results/0/processing",
            vec![
                json!("production"),
                json!("training"),
                json!("debug"),
                json!("quality_control"),
            ],
        ),
        (
            "/results/0/sample/source",
            vec![
                json!("scanned_by_machine"),
                json!("typed_on_machine"),
                json!("work_list"),
                json!("instrument_assigned"),
                json!("unknown"),
            ],
        ),
        (
            "/results/0/sample/position/kind",
            vec![
                json!("rack"),
                json!("disk"),
                json!("tray"),
                json!({"other":"carousel"}),
            ],
        ),
        (
            "/results/0/parts/0/status",
            vec![
                json!("pending"),
                json!("preliminary"),
                json!("final"),
                json!("corrected"),
                json!("withdrawn"),
                json!("not_performed"),
                json!("unknown"),
            ],
        ),
        (
            "/results/0/parts/0/times/0/time/offset",
            vec![json!(-93599), json!(0), json!(93599)],
        ),
    ] {
        for variant in variants {
            let mut document = base.clone();
            *document.pointer_mut(path).ok_or("fixture path")? = variant;
            let expected = document.pointer("/results/0").ok_or("result")?;
            let mut input = expected.clone();
            model_fixture(&mut input);
            let model: analyte_model::ResultContent = serde_json::from_value(input)?;
            assert_eq!(
                serde_json::to_value(ResultDocument::from(&model))?,
                *expected
            );
        }
    }
    for (model, expected) in [
        (analyte_model::OrderState::Requested, "requested"),
        (analyte_model::OrderState::Sent, "sent"),
        (analyte_model::OrderState::Accepted, "accepted"),
        (analyte_model::OrderState::Done, "done"),
        (analyte_model::OrderState::Rejected, "rejected"),
        (
            analyte_model::OrderState::CancelRequested,
            "cancel_requested",
        ),
        (
            analyte_model::OrderState::CancelConfirmed,
            "cancel_confirmed",
        ),
        (analyte_model::OrderState::Uncertain, "uncertain"),
    ] {
        assert_eq!(
            serde_json::to_value(analyte_contract::document::OrderState::try_from(&model)?)?,
            json!(expected)
        );
        let mut event: Value = serde_json::from_slice(include_bytes!(
            "../../../../docs/contract/vectors/event.json"
        ))?;
        event["state"] = json!(expected);
        let _: analyte_contract::OrderEvent = decode(&serde_json::to_vec(&event)?)?;
    }
    Ok(())
}

// Safety rule 8 (ADR 0003)
#[test]
fn internal_sending_state_has_no_wire_form() {
    assert!(matches!(
        analyte_contract::document::OrderState::try_from(&analyte_model::OrderState::Sending),
        Err(analyte_contract::DocumentError::InternalOrderState)
    ));
    assert!(decode::<analyte_contract::document::OrderState>(b"\"sending\"").is_err());
}

// ADR 0008; Safety rule 6 (ADR 0003)
#[test]
fn dilution_original_survives_without_guessed_factor() -> Result<(), Box<dyn std::error::Error>> {
    let model = analyte_model::Dilution {
        original: Some(analyte_model::SourceText::new("1:10")?),
        factor: None,
        applied: analyte_model::DilutionApplied::Unknown,
    };
    let wire = analyte_contract::document::Dilution::from(&model);
    assert_eq!(
        serde_json::to_value(&wire)?,
        json!({ "original": "1:10", "factor": null, "applied": "unknown" })
    );
    let decoded: analyte_contract::document::Dilution =
        serde_json::from_value(serde_json::to_value(&wire)?)?;
    assert_eq!(decoded, wire);
    Ok(())
}
