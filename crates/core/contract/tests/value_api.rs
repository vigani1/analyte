//! Checks that public value construction preserves readable evidence.
// Safety rule 5 (ADR 0003)
#[test]
fn public_values_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    use analyte_contract::document::*;
    let original = SourceText::new("001.20")?;
    let values = [
        ResultValue::new(
            original.clone(),
            ValueInterpretation::Numeric(NumericValue {
                number: Decimal::new("001.20")?,
                comparator: None,
            }),
        ),
        ResultValue::new(original.clone(), ValueInterpretation::Text),
        ResultValue::text(original, Some(SourceText::new("decoded")?)),
    ];
    for value in values {
        assert!(
            value.decoded_text().is_none() || matches!(value.parsed(), ValueInterpretation::Text)
        );
        let bytes = serde_json::to_vec(&value)?;
        assert_eq!(serde_json::from_slice::<ResultValue>(&bytes)?, value);
    }
    Ok(())
}

// Safety rule 5 (ADR 0003)
#[test]
fn public_model_values_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    use analyte_model::*;
    let original = SourceText::new("001.20")?;
    let values = [
        ResultValue::new(
            original.clone(),
            ValueInterpretation::Numeric(NumericValue {
                number: Decimal::new("001.20")?,
                comparator: None,
            }),
        ),
        ResultValue::new(original.clone(), ValueInterpretation::Text),
        ResultValue::text(original, Some(SourceText::new("decoded")?)),
    ];
    for value in values {
        assert!(
            value.decoded_text().is_none() || matches!(value.parsed(), ValueInterpretation::Text)
        );
        let bytes = serde_json::to_vec(&value)?;
        assert_eq!(serde_json::from_slice::<ResultValue>(&bytes)?, value);
    }
    Ok(())
}
