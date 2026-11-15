//! ADR 0008 / plan section 6: independent expected wire evidence and conversion.
#![forbid(unsafe_code)]

use analyte_contract::{Delivery, ResultDocument, decode};
use serde_json::{Value, json};

#[test]
fn rule5_decoded_text_preserves_original_and_never_accompanies_unsupported()
-> Result<(), Box<dyn std::error::Error>> {
    for bytes in [
        include_bytes!("../../../docs/contract/vectors/decoded-text.json").as_slice(),
        include_bytes!("../../../docs/contract/vectors/unsupported-formatting.json").as_slice(),
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
        let contract: analyte_contract::evidence::ResultValue =
            serde_json::from_value(value.clone())?;
        assert_eq!(serde_json::to_value(contract)?, value);
    }
    for value in [
        json!({"original":"x","parsed":"text","decoded_text":"x".repeat(16385)}),
        json!({"original":"x","parsed":{"unsupported":{"source_type":"FT","reason":"Formatting"}},"decoded_text":"partial"}),
    ] {
        assert!(
            serde_json::from_value::<analyte_contract::evidence::ResultValue>(value.clone())
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

#[test]
fn adr0008_all_result_evidence_converts_without_loss() -> Result<(), Box<dyn std::error::Error>> {
    for bytes in [
        include_bytes!("../../../docs/contract/vectors/delivery.json").as_slice(),
        include_bytes!("../../../docs/contract/vectors/unresolved-identity.json").as_slice(),
        include_bytes!("../../../docs/contract/vectors/missing-sample.json").as_slice(),
        include_bytes!("../../../docs/contract/vectors/multi-part-coded.json").as_slice(),
        include_bytes!("../../../docs/contract/vectors/withdrawn.json").as_slice(),
    ] {
        let document: Value = serde_json::from_slice(bytes)?;
        let expected = document.pointer("/results/0").ok_or("result")?;
        let mut input = expected.clone();
        model_fixture(&mut input);
        let model: analyte_model::ResultContent = serde_json::from_value(input)?;
        let wire = ResultDocument::from(&model);
        assert_eq!(serde_json::to_value(&wire)?, *expected);
        let _: Delivery = decode(bytes)?;
        let receipt = analyte_model::ReceiptMetadata {
            id: model.receipt,
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
        };
        let id = analyte_model::DeliveryId::try_from(uuid::Uuid::parse_str(
            "019ab000-0000-7000-8000-000000000001",
        )?)?;
        let delivery =
            Delivery::from_result(&model, &receipt, id, "2026-11-15T12:00:00Z".parse()?)?;
        assert_eq!(serde_json::to_value(delivery)?, document);
        let mut mismatch = receipt;
        mismatch.id = analyte_model::ReceiptId::try_from(uuid::Uuid::now_v7())?;
        assert!(Delivery::from_result(&model, &mismatch, id, mismatch.received_at).is_err());
    }
    Ok(())
}

#[test]
fn adr0008_every_interpretation_and_source_text_survives() -> Result<(), Box<dyn std::error::Error>>
{
    let mut document: Value = serde_json::from_slice(include_bytes!(
        "../../../docs/contract/vectors/delivery.json"
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

#[test]
fn adr0008_invalid_evidence_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let base: Value = serde_json::from_slice(include_bytes!(
        "../../../docs/contract/vectors/delivery.json"
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

#[test]
fn adr0008_modes_statuses_and_provenance_variants_survive() -> Result<(), Box<dyn std::error::Error>>
{
    let base: Value = serde_json::from_slice(include_bytes!(
        "../../../docs/contract/vectors/delivery.json"
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
            serde_json::to_value(analyte_contract::evidence::OrderState::try_from(&model)?)?,
            json!(expected)
        );
        let mut event: Value =
            serde_json::from_slice(include_bytes!("../../../docs/contract/vectors/event.json"))?;
        event["state"] = json!(expected);
        let _: analyte_contract::OrderEvent = decode(&serde_json::to_vec(&event)?)?;
    }
    Ok(())
}

#[test]
fn rule8_internal_sending_has_no_contract_representation() {
    assert!(matches!(
        analyte_contract::evidence::OrderState::try_from(&analyte_model::OrderState::Sending),
        Err(analyte_contract::DocumentError::InternalOrderState)
    ));
    assert!(decode::<analyte_contract::evidence::OrderState>(b"\"sending\"").is_err());
}

#[test]
fn adr0008_rule6_dilution_original_survives_without_a_guessed_factor()
-> Result<(), Box<dyn std::error::Error>> {
    let model = analyte_model::Dilution {
        original: Some(analyte_model::SourceText::new("1:10")?),
        factor: None,
        applied: analyte_model::DilutionApplied::Unknown,
    };
    let wire = analyte_contract::evidence::Dilution::from(&model);
    assert_eq!(
        serde_json::to_value(&wire)?,
        json!({ "original": "1:10", "factor": null, "applied": "unknown" })
    );
    let decoded: analyte_contract::evidence::Dilution =
        serde_json::from_value(serde_json::to_value(&wire)?)?;
    assert_eq!(decoded, wire);
    Ok(())
}
