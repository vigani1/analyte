//! Tests VIDAS result uploads and work downloads.
use analyte_vidas::syntax::{Message, SampleLimit, Subject};

// VIDAS BCI Link rev A §4
#[test]
fn keeps_exact_identity_unknown_tags_and_values() -> Result<(), Box<dyn std::error::Error>> {
    let text = "mtrsl|pi|si|ci000a|rtHCG|tt09:05|td2026/01/02|qlEquivoque|qn<1,234mUI/ml|y3mUI/ml|qd1:1|nccalinc|zzextension|";
    let message = Message::parse(text, SampleLimit::Twelve)?;
    assert_eq!(message.text(), text);
    assert_eq!(message.get("zz"), Some("extension"));
    let result = analyte_mapping::vidas::map_evidence(&message)?;
    assert_eq!(
        result.subject,
        Subject::Sample(analyte_model::SampleId::new("000a")?)
    );
    assert_eq!(result.value.original.as_str(), "<1,234mUI/ml");
    assert_eq!(result.call.as_str(), "Equivoque");
    assert_eq!(result.flag.ok_or("flag")?.as_str(), "calinc");
    assert!(Message::parse("mtmpr|piX|si|ci1234567890123|rtHCG|", SampleLimit::Twelve).is_err());
    assert!(
        Message::parse(
            "mtmpr|piX|si|ci1234567890123|rtHCG|",
            SampleLimit::Seventeen
        )
        .is_ok()
    );
    assert!(Message::parse("mtmpr|piX|si|ci001|", SampleLimit::Twelve).is_err());
    assert!(Message::new(&[("mt", "mpr"), ("pi", "X|ciY")], SampleLimit::Twelve).is_err());
    Ok(())
}

// BCI Link rev C §4 pp. 4-7, 4-10 and appendix B. Preserve both fields
// even when the call explicitly invalidates a quantitative-looking value.
#[test]
fn localized_invalid_call_preserves_original_measurement_and_call()
-> Result<(), Box<dyn std::error::Error>> {
    use analyte_model::ValueInterpretation;
    for (call, expected) in [
        ("INVALID", ValueInterpretation::Invalid),
        ("Invalide", ValueInterpretation::Invalid),
        ("UNGUELTIG", ValueInterpretation::Invalid),
        ("NON VALIDO", ValueInterpretation::Invalid),
        ("INVALID RECAL", ValueInterpretation::Text),
        ("", ValueInterpretation::Text),
    ] {
        let message = Message::parse(
            &format!("mtrsl|pi|si|ci000a|rtSYN|tt09:05|td2026/01/02|ql{call}|qn< 123|"),
            SampleLimit::Twelve,
        )?;
        let result = analyte_mapping::vidas::map_evidence(&message)?;
        assert_eq!(result.call.as_str(), call);
        assert_eq!(result.value.original.as_str(), "< 123");
        assert_eq!(result.value.parsed(), &expected);
    }
    Ok(())
}
