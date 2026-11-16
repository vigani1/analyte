//! VIDAS BCI Appendix A chapter 4 mapping assertions, moved without changed expectations.
use analyte_vidas::syntax::{Message, SampleLimit};

#[test]
fn vidas_bci_a_4_exact_identity_unknown_tags_and_values() -> Result<(), Box<dyn std::error::Error>>
{
    let text = "mtrsl|pi|si|ci000a|rtHCG|tt09:05|td2026/01/02|qlEquivoque|qn<1,234mUI/ml|y3mUI/ml|qd1:1|nccalinc|zzextension|";
    let message = Message::parse(text, SampleLimit::Twelve)?;
    assert_eq!(message.text(), text);
    assert_eq!(message.get("zz"), Some("extension"));
    let result = analyte_mapping::vidas_evidence(&message)?;
    assert_eq!(result.sample.as_str(), "000a");
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
    assert!(Message::build(&[("mt", "mpr"), ("pi", "X|ciY")], SampleLimit::Twelve).is_err());
    Ok(())
}
