//! Tests agreement between frozen contract validators and current documents.
use analyte_contract::document as wire;
use analyte_model as model;
use proptest::prelude::*;

// ADR 0008 and contract v1, result value interpretations.
#[test]
fn qualitative_absent_and_invalid_values_keep_distinct_wire_meanings()
-> Result<(), Box<dyn std::error::Error>> {
    use model::{QualitativeReaction, ValueInterpretation};
    use serde_json::json;

    for (original, interpretation, parsed) in [
        (
            "-1",
            ValueInterpretation::Qualitative(QualitativeReaction::Negative),
            json!({"qualitative": "negative"}),
        ),
        (
            "0",
            ValueInterpretation::Qualitative(QualitativeReaction::Borderline),
            json!({"qualitative": "borderline"}),
        ),
        (
            "1",
            ValueInterpretation::Qualitative(QualitativeReaction::Positive),
            json!({"qualitative": "positive"}),
        ),
        ("", ValueInterpretation::Absent, json!("absent")),
        ("ERR", ValueInterpretation::Invalid, json!("invalid")),
    ] {
        let value = model::ResultValue::new(model::SourceText::new(original)?, interpretation);
        assert_eq!(
            serde_json::to_value(wire::ResultValue::from(&value))?,
            json!({"original": original, "parsed": parsed})
        );
    }
    Ok(())
}

fn text() -> impl Strategy<Value = String> {
    prop_oneof![
        any::<String>(),
        (
            prop_oneof![Just('a'), Just('?'), Just('\0'), Just('\n')],
            prop::sample::select(vec![
                0, 1, 255, 256, 257, 1023, 1024, 1025, 16383, 16384, 16385
            ])
        )
            .prop_map(|(character, count)| std::iter::repeat_n(character, count).collect()),
    ]
}

proptest! {
    // ADR 0008
    #[test]
    fn sample_id_validation_agrees(value in text()) {
        prop_assert_eq!(model::SampleId::new(&value).is_ok(), wire::SampleId::new(&value).is_ok());
    }
    // ADR 0008
    #[test]
    fn identifier_validation_agrees(value in text()) {
        prop_assert_eq!(model::Identifier::new(&value).is_ok(), wire::Identifier::new(&value).is_ok());
    }
    // ADR 0008
    #[test]
    fn source_text_validation_agrees(value in text()) {
        prop_assert_eq!(model::SourceText::new(&value).is_ok(), wire::SourceText::new(&value).is_ok());
    }
    // ADR 0008
    #[test]
    fn reason_validation_agrees(value in text()) {
        prop_assert_eq!(model::Reason::new(&value).is_ok(), wire::Reason::new(&value).is_ok());
    }
    // ADR 0008
    #[test]
    fn decimal_validation_agrees(value in text()) {
        prop_assert_eq!(model::Decimal::new(&value).is_ok(), wire::Decimal::new(&value).is_ok());
    }
    // ADR 0008
    #[test]
    fn uuid_validation_agrees(bytes in any::<[u8; 16]>(), version in 0u8..16, variant in any::<u8>()) {
        let mut bytes = bytes;
        bytes[6] = (bytes[6] & 15) | (version << 4);
        bytes[8] = variant;
        let value = uuid::Uuid::from_bytes(bytes);
        prop_assert_eq!(model::ReceiptId::try_from(value).is_ok(), wire::ReceiptId::try_from(value).is_ok());
        prop_assert_eq!(model::DeliveryId::try_from(value).is_ok(), wire::DeliveryId::try_from(value).is_ok());
    }
    // ADR 0008
    #[test]
    fn offset_validation_agrees(value in prop_oneof![any::<i32>(), -94000i32..94000]) {
        prop_assert_eq!(model::UtcOffset::try_from(value).is_ok(), wire::UtcOffset::try_from(value).is_ok());
    }
    // ADR 0008
    #[test]
    fn evidence_bounds_agree(count in 0usize..4100) {
        let model = model::EvidenceList::new(vec![(); count]);
        let wire = wire::EvidenceList::new(vec![(); count]);
        prop_assert_eq!(model.is_ok(), wire.is_ok());
        if let (Ok(model), Ok(wire)) = (model, wire) {
            prop_assert_eq!(model::NonEmptyEvidenceList::new(model).is_ok(), wire::NonEmptyEvidenceList::new(wire).is_ok());
        }
    }
    // ADR 0008
    #[test]
    fn unit_source_validation_agrees(original in text(), source in text(), target in text(), matching in any::<bool>()) {
        let source = if matching { original.clone() } else { source };
        let value = serde_json::json!({"original": original, "mapping": {"source": source, "target": target}});
        prop_assert_eq!(serde_json::from_value::<model::Units>(value.clone()).is_ok(), serde_json::from_value::<wire::Units>(value).is_ok());
    }
    // ADR 0008
    #[test]
    fn decoded_text_validation_agrees(original in text(), decoded in prop::option::of(text()), kind in 0u8..3) {
        let parsed = match kind {
            0 => serde_json::json!("text"),
            1 => serde_json::json!({"numeric":{"number":"12.30", "comparator":null}}),
            _ => serde_json::json!({"unsupported":{"source_type":"SYN", "reason":"synthetic"}}),
        };
        let model_parsed = match kind {
            0 => serde_json::json!("Text"),
            1 => serde_json::json!({"Numeric":{"number":"12.30", "comparator":null}}),
            _ => serde_json::json!({"Unsupported":{"source_type":"SYN", "reason":"synthetic"}}),
        };
        let model_value = serde_json::json!({"original":original, "decoded_text": decoded, "parsed": model_parsed});
        let value = serde_json::json!({"original":original, "decoded_text": decoded, "parsed": parsed});
        prop_assert_eq!(serde_json::from_value::<model::ResultValue>(model_value).is_ok(), serde_json::from_value::<wire::ResultValue>(value).is_ok());
    }
}

proptest! {
    // ADR 0008: source evidence survives the frozen wire projection, including absence.
    #[test]
    fn result_details_survive_wire_round_trip(
        suffix in 0u32..,
        present in prop_oneof![Just(0u32), Just(u32::MAX), any::<u32>()],
        (model_method, wire_method) in prop::sample::select(vec![
            ("Measured", "measured"), ("Calculated", "calculated"),
        ]),
        (model_completion, wire_completion) in prop::sample::select(vec![
            ("Normal", "normal"), ("SenderAborted", "sender_aborted"), ("Unknown", "unknown"),
        ]),
    ) {
        use serde_json::json;

        let source = |label| format!("  {label}-{suffix:08}  ");
        let mut evidence = serde_json::Map::new();
        for (index, field) in [
            "specimen_descriptor", "message_code", "predilution", "specimen_label",
            "control_name", "report_list", "control_knowledge", "control_level", "control_lot",
            "strip_lot", "instrument_id", "instrument_serial", "technologist", "termination",
        ].into_iter().enumerate() {
            if present & (1 << index) != 0 {
                evidence.insert(field.into(), json!(source(field)));
            }
        }
        evidence.insert("alarms".into(), if present & (1 << 14) == 0 {
            json!([])
        } else {
            json!([
                {"number": source("second"), "text": null},
                {"number": source("first"), "text": source("alarm-text")},
            ])
        });
        let mut model_details = evidence.clone();
        let mut wire_details = evidence;
        for (bit, field, model_value, wire_value) in [
            (15, "method", json!(model_method), json!(wire_method)),
            (16, "completion", json!(model_completion), json!(wire_completion)),
            (17, "report_status", json!("Corrected"), json!("corrected")),
            (18, "cutoff_index",
                json!({"original": "<0.100 ", "reference": source("cutoff-reference"),
                    "parsed": {"Numeric": {"number": "0.100", "comparator": "Less"}}}),
                json!({"original": "<0.100 ", "reference": source("cutoff-reference"),
                    "parsed": {"numeric": {"number": "0.100", "comparator": "less"}}})),
        ] {
            if present & (1 << bit) != 0 {
                model_details.insert(field.into(), model_value);
                wire_details.insert(field.into(), wire_value);
            }
        }

        let model: model::ResultDetails = serde_json::from_value(model_details.into()).map_err(|error| TestCaseError::fail(error.to_string()))?;
        let expected: wire::ResultDetails = serde_json::from_value(wire_details.into()).map_err(|error| TestCaseError::fail(error.to_string()))?;
        let serialized = serde_json::to_vec(&wire::ResultDetails::from(&model)).map_err(|error| TestCaseError::fail(error.to_string()))?;
        let decoded: wire::ResultDetails = analyte_contract::decode(&serialized).map_err(|error| TestCaseError::fail(error.to_string()))?;
        prop_assert_eq!(decoded, expected);
    }
}
