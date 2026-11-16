//! Tests exact configured HL7 editions and actionable unsupported-edition diagnostics.
use analyte_config::{Draft, Edition, Error, EvidenceLevel};
use analyte_hl7::Version;

const MACHINE: &str = include_str!("../examples/hl7.toml");
const SITE: &str = include_str!("../examples/site.toml");

fn draft(edition: &str) -> Result<Draft, Error> {
    let machine = MACHINE.replace("edition = \"v251\"", &format!("edition = \"{edition}\""));
    Draft::load(
        ("synthetic.toml", &machine),
        ("site.toml", SITE),
        ("local.toml", "schema_version=1"),
    )
}

#[test]
fn adr0012_s3_typed_supported_config_editions_preserve_exact_wire_identity() -> Result<(), Error> {
    for (config_id, edition, wire) in [
        ("v23", Edition::V23, "2.3"),
        ("v231", Edition::V231, "2.3.1"),
        ("v24", Edition::V24, "2.4"),
        ("v25", Edition::V25, "2.5"),
        ("v251", Edition::V251, "2.5.1"),
    ] {
        let config = draft(config_id)?.validate()?;
        let analyte_config::Stack::Hl7(analyte_config::RecordTransport::Stream(stack)) =
            config.effective().stack()
        else {
            panic!("synthetic fixture must remain an HL7 stream stack");
        };
        assert_eq!(stack.syntax().edition(), edition);
        assert_eq!(Version::from(edition).as_str(), wire);
        assert!(
            config
                .canonical()
                .contains(&format!("edition = \"{config_id}\""))
        );
        assert_eq!(
            config.effective().machine().evidence.level,
            EvidenceLevel::Documented
        );
    }
    Ok(())
}

#[test]
fn adr0012_s3_recognized_editions_without_typed_support_fail_config_validation() {
    for (config_id, wire) in [("v26", "2.6"), ("v282", "2.8.2"), ("v291", "2.9.1")] {
        assert_eq!(Version::parse(wire.as_bytes()).unwrap().as_str(), wire);
        let error = draft(config_id).unwrap().validate().unwrap_err();
        assert!(
            matches!(&error, Error::UnsupportedHl7Edition { path, edition }
            if path == "synthetic.toml" && *edition == wire)
        );
        assert_eq!(
            error.to_string(),
            format!(
                "synthetic.toml: syntax.edition: HL7 {wire} is recognized but not yet supported for result processing; supported: 2.3, 2.3.1, 2.4, 2.5, 2.5.1"
            )
        );
    }
}

#[test]
fn adr0012_s3_unlisted_config_editions_name_bad_value_and_allowed_values() {
    for edition in [
        "", "v22", "v230", "v2510", "v27", "v28", "v29", "v292", "v291 ", "2.9.1",
    ] {
        let Error::Schema { path, source } = draft(edition).unwrap_err() else {
            panic!("an unlisted edition must fail at the schema boundary");
        };
        assert_eq!(path, "synthetic.toml");
        let diagnostic = source.to_string();
        assert!(diagnostic.contains("unknown variant"), "{diagnostic}");
        assert!(diagnostic.contains(edition), "{diagnostic}");
        for allowed in ["v23", "v231", "v24", "v25", "v251", "v26", "v282", "v291"] {
            assert!(diagnostic.contains(allowed), "{diagnostic}");
        }
        assert!(source.span().is_some());
    }
}

#[test]
fn adr0012_s3_missing_config_edition_is_not_defaulted() {
    let machine = MACHINE
        .lines()
        .filter(|line| !line.starts_with("edition ="))
        .collect::<Vec<_>>()
        .join("\n");
    let draft = Draft::load(
        ("synthetic.toml", &machine),
        ("site.toml", SITE),
        ("local.toml", "schema_version=1"),
    )
    .unwrap();
    assert!(draft.validate().is_err());
}

#[test]
fn hl7_2_5_1_s2_8_legacy_query_configs_validate_but_unimplemented_order_writers_do_not() {
    for edition in ["v23", "v24", "v25"] {
        let base = MACHINE.replace("edition = \"v251\"", &format!("edition = \"{edition}\""));
        for (section, sending) in [
            (include_str!("../examples/hl7_orders.toml"), false),
            (include_str!("../examples/hl7_send.toml"), true),
        ] {
            let machine = format!("{base}\n{section}");
            let result = Draft::load(
                ("synthetic.toml", &machine),
                ("site.toml", SITE),
                ("local.toml", "schema_version=1"),
            )
            .unwrap()
            .validate();
            if sending {
                assert!(matches!(result, Err(Error::Invalid { key, reason, .. })
                    if key == "orders.hl7_send" && reason.contains("not yet implemented")));
            } else {
                result.unwrap();
            }
        }
    }
}
