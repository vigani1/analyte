//! Tests vidas field preservation and malformed inputs against vendor source sections.
use super::*;
use proptest::prelude::*;

const RESULT_FIELDS: &[(&str, &str)] = &[
    ("mt", "rsl"),
    ("pi", "PATIENT"),
    ("pn", "Synthetic name"),
    ("pb", "20000101"),
    ("ps", "F"),
    ("so", "SERUM"),
    ("si", ""),
    ("ci", "000042a"),
    ("rt", "TEST"),
    ("rn", "Synthetic assay"),
    ("tt", "20260101"),
    ("td", "120000"),
    ("ql", "positive"),
    ("qn", "<0.1"),
    ("y3", "1,234"),
    ("qd", "10"),
    ("nc", "CONTROL"),
    ("id", "LOT"),
    ("sn", "SERIAL"),
    ("m4", "component"),
    ("zz", "extension"),
];

#[test]
fn vidas_bci_appendix_a_ch4_every_result_field_and_unknown_tag_roundtrip() {
    let message = Message::new(RESULT_FIELDS, SampleLimit::Seventeen).unwrap();
    let parsed = Message::parse(message.text(), SampleLimit::Seventeen).unwrap();
    assert_eq!(parsed.kind(), Kind::Result);
    assert_eq!(parsed.sample().as_str(), "000042a");
    for (field, (tag, value)) in parsed.fields().iter().zip(RESULT_FIELDS) {
        assert_eq!(field.tag(), *tag);
        assert_eq!(field.value(), *value);
        assert_eq!(parsed.get(tag), Some(*value));
    }
    assert_eq!(parsed.fields().len(), RESULT_FIELDS.len());
    assert_eq!(parsed.text(), message.text());
    let request: Vec<_> = RESULT_FIELDS
        .iter()
        .copied()
        .filter(|(tag, _)| REQUEST.contains(tag))
        .map(|(tag, value)| (tag, if tag == "mt" { "mpr" } else { value }))
        .collect();
    let request = Message::new(&request, SampleLimit::Twelve).unwrap();
    assert_eq!(request.kind(), Kind::Request);
    assert_eq!(request.fields().len(), REQUEST.len());
    assert_eq!(
        Message::parse(request.text(), SampleLimit::Twelve)
            .unwrap()
            .text(),
        request.text()
    );
}

#[test]
fn vidas_bci_appendix_a_ch4_field_widths_and_installed_identity_limits() {
    for (tag, width) in [
        ("pi", 17),
        ("ci", 17),
        ("pn", 30),
        ("y3", 30),
        ("so", 5),
        ("si", 0),
        ("rt", 4),
        ("rn", 40),
        ("ql", 29),
        ("qn", 29),
        ("nc", 15),
        ("id", 15),
        ("sn", 20),
        ("m4", 20),
        ("ps", 1),
    ] {
        let exact = "X".repeat(width);
        let mut fields = RESULT_FIELDS.to_vec();
        let slot = fields.iter().position(|(key, _)| *key == tag).unwrap();
        fields[slot].1 = &exact;
        assert!(
            Message::new(&fields, SampleLimit::Seventeen).is_ok(),
            "{tag}"
        );
        let excessive = "X".repeat(width + 1);
        fields[slot].1 = &excessive;
        assert!(
            matches!(Message::new(&fields, SampleLimit::Seventeen), Err(Error::Field(key)) if key == tag)
        );
    }
    let mut fields = RESULT_FIELDS.to_vec();
    let slot = fields.iter().position(|(key, _)| *key == "ci").unwrap();
    fields[slot].1 = "123456789012";
    assert!(Message::new(&fields, SampleLimit::Twelve).is_ok());
    fields[slot].1 = "1234567890123";
    assert!(Message::new(&fields, SampleLimit::Twelve).is_err());
}

#[test]
fn vidas_bci_appendix_a_ch4_rejects_missing_duplicate_reordered_and_malformed_fields() {
    for tag in ["mt", "pi", "si", "ci", "rt", "tt", "td", "ql", "qn"] {
        let fields: Vec<_> = RESULT_FIELDS
            .iter()
            .copied()
            .filter(|(key, _)| *key != tag)
            .collect();
        assert!(
            Message::new(&fields, SampleLimit::Seventeen).is_err(),
            "{tag}"
        );
    }
    let message = Message::new(RESULT_FIELDS, SampleLimit::Seventeen).unwrap();
    let text = message.text();
    for malformed in [
        text.trim_end_matches('|').to_owned(),
        format!("{text}ciOTHER|"),
        text.replace("pir", "piq").replace("rsl", "bad"),
        text.replace("qn<0.1", "qn\n"),
        text.replace("piPATIENT|pnSynthetic name", "pnSynthetic name|piPATIENT"),
        text.replace("mt", "MT"),
        text.replace("ci000042a", "ci"),
    ] {
        assert!(Message::parse(&malformed, SampleLimit::Seventeen).is_err());
    }
    assert!(Message::new(&[("mt", "rsl|injected")], SampleLimit::Seventeen).is_err());
    assert!(Message::parse(&"X".repeat(1921), SampleLimit::Seventeen).is_err());
}

#[test]
fn vidas_bci_appendix_a_ch4_total_message_byte_ceiling_is_inclusive() {
    let base = "mtrsl|pi|si|ci0001|rtTEST|tt|td|ql|qn|zz";
    let raw = format!("{base}{}|", "X".repeat(1920 - base.len() - 1));
    assert_eq!(raw.len(), 1920);
    assert_eq!(
        Message::parse(&raw, SampleLimit::Twelve).unwrap().text(),
        raw
    );
    assert!(Message::parse(&raw.replace("zz", "zzX"), SampleLimit::Twelve).is_err());
}

proptest! {
    #[test]
    fn vidas_bci_appendix_a_ch4_tagged_text_roundtrip(sample in "[0-9A-Za-z]{1,12}", value in "[0-9A-Za-z <>,.]{0,29}") {
        let fields = [("mt","rsl"),("pi",""),("si",""),("ci",sample.as_str()),
            ("rt","TEST"),("tt","20260101"),("td","120000"),("ql",""),("qn",value.as_str())];
        let message = Message::new(&fields, SampleLimit::Twelve).unwrap();
        let parsed = Message::parse(message.text(), SampleLimit::Twelve).unwrap();
        prop_assert_eq!(parsed.sample().as_str(), sample);
        prop_assert_eq!(parsed.get("qn"), Some(value.as_str()));
        prop_assert_eq!(parsed.text(), message.text());
    }
}
