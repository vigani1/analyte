//! LAW FT line evidence is retained and blocks delivery without rendering layout.
use super::*;
// HL7 2.5.1 §2.7.6; IHE PaLM TF-2b §C.5
#[test]
fn ft_line_markers_keep_layout_evidence() -> TestResult {
    let cfg = config(include_str!("../../../../core/config/examples/law.toml"))?;
    for note in [
        "first^second",
        r"first^\.sp\second",
        r"\.ti+2\first^second",
        r"\.nf\first^second",
        r"first\.br\second",
        r"before\H\marked\N\after",
    ] {
        let raw = format!("{}NTE|1|Z|{note}\r", std::str::from_utf8(LAW_RESULT)?);
        let message = analyte_hl7::law::parse_message(
            raw.as_bytes(),
            effective_options(cfg.effective()).ok_or("options")?,
        )?;
        let results = analyte_hl7::law::Results::new(&message)?;
        let output = analyte_mapping::law::map(settings::law(&cfg), &results, input())?;
        assert!(output.requires_parking());
        assert!(
            output
                .findings
                .iter()
                .any(|f| f.position.field == 3 && f.problem == Problem::UnsupportedFormatting)
        );
        assert_eq!(output.comments.last().ok_or("note")?.text.as_str(), note);
        assert_eq!(message.raw(), raw.as_bytes());
    }
    Ok(())
}
