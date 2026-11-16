//! Tests agreement between frozen contract validators and current documents.
use analyte_contract::document as wire;
use analyte_model as model;
use proptest::prelude::*;

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
    #[test]
    fn adr0008_sampleid_validation_agrees(value in text()) {
        prop_assert_eq!(model::SampleId::new(&value).is_ok(), wire::SampleId::new(&value).is_ok());
    }
    #[test]
    fn adr0008_identifier_validation_agrees(value in text()) {
        prop_assert_eq!(model::Identifier::new(&value).is_ok(), wire::Identifier::new(&value).is_ok());
    }
    #[test]
    fn adr0008_sourcetext_validation_agrees(value in text()) {
        prop_assert_eq!(model::SourceText::new(&value).is_ok(), wire::SourceText::new(&value).is_ok());
    }
    #[test]
    fn adr0008_reason_validation_agrees(value in text()) {
        prop_assert_eq!(model::Reason::new(&value).is_ok(), wire::Reason::new(&value).is_ok());
    }
    #[test]
    fn adr0008_decimal_validation_agrees(value in text()) {
        prop_assert_eq!(model::Decimal::new(&value).is_ok(), wire::Decimal::new(&value).is_ok());
    }
    #[test]
    fn adr0008_uuid_validation_agrees(bytes in any::<[u8; 16]>(), version in 0u8..16, variant in any::<u8>()) {
        let mut bytes = bytes;
        bytes[6] = (bytes[6] & 15) | (version << 4);
        bytes[8] = variant;
        let value = uuid::Uuid::from_bytes(bytes);
        prop_assert_eq!(model::ReceiptId::try_from(value).is_ok(), wire::ReceiptId::try_from(value).is_ok());
        prop_assert_eq!(model::DeliveryId::try_from(value).is_ok(), wire::DeliveryId::try_from(value).is_ok());
    }
    #[test]
    fn adr0008_offset_validation_agrees(value in prop_oneof![any::<i32>(), -94000i32..94000]) {
        prop_assert_eq!(model::UtcOffset::try_from(value).is_ok(), wire::UtcOffset::try_from(value).is_ok());
    }
    #[test]
    fn adr0008_evidence_bounds_agree(count in 0usize..4100) {
        let model = model::EvidenceList::new(vec![(); count]);
        let wire = wire::EvidenceList::new(vec![(); count]);
        prop_assert_eq!(model.is_ok(), wire.is_ok());
        if let (Ok(model), Ok(wire)) = (model, wire) {
            prop_assert_eq!(model::NonEmptyEvidenceList::new(model).is_ok(), wire::NonEmptyEvidenceList::new(wire).is_ok());
        }
    }
    #[test]
    fn adr0008_unit_source_validation_agrees(original in text(), source in text(), target in text(), matching in any::<bool>()) {
        let source = if matching { original.clone() } else { source };
        let value = serde_json::json!({"original": original, "mapping": {"source": source, "target": target}});
        prop_assert_eq!(serde_json::from_value::<model::Units>(value.clone()).is_ok(), serde_json::from_value::<wire::Units>(value).is_ok());
    }
    #[test]
    fn adr0008_decoded_text_validation_agrees(original in text(), decoded in prop::option::of(text()), kind in 0u8..3) {
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
