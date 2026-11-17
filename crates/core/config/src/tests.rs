//! Checks configuration rejection and immutable validated settings using synthetic inputs.
use super::*;
const SITE: &str = include_str!("../examples/site.toml");
const ASTM: &str = include_str!("../examples/astm.toml");
const HL7: &str = include_str!("../examples/hl7.toml");

#[test]
fn adr0002_every_example_keeps_legacy_canonical_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
    let cases = [
        ("astm.toml", "", "astm"),
        ("hl7.toml", "", "hl7"),
        ("law.toml", "", "law"),
        ("poct.toml", "", "poct"),
        ("vidas_literal.toml", "", "vidas"),
        ("autolyser.toml", "", "autolyser"),
        ("orders.toml", ASTM, "astm"),
        ("hl7_orders.toml", HL7, "hl7"),
        ("hl7_send.toml", HL7, "hl7"),
        (
            "law_orders.toml",
            include_str!("../examples/law.toml"),
            "law",
        ),
        ("site.toml", HL7, "hl7"),
    ];

    for (name, base, _) in cases {
        let text = std::fs::read_to_string(directory.join(name))?;
        let (machine, site) = if name == "site.toml" {
            (base.to_owned(), text)
        } else {
            (format!("{base}\n{text}"), SITE.to_owned())
        };
        let parsed_machine: MachineDocument = load("machine", &machine)?;
        let parsed_site: Site = load("site", &site)?;
        let legacy = toml::Value::try_from(Canonical {
            schema_version: 1,
            machine: &parsed_machine,
            site: &parsed_site,
        })?;
        let validated = Draft::load(
            ("machine", &machine),
            ("site", &site),
            ("overrides", "schema_version=1"),
        )?
        .validate()?;
        assert_eq!(toml::to_string(&legacy)?, validated.canonical(), "{name}");
    }
    Ok(())
}

#[test]
fn adr0002_stack_conversion_returns_missing_setting_without_validation() -> Result<(), Error> {
    let mut machine: MachineDocument = load("machine", ASTM)?;
    machine.framing.checksum_hex = None;
    assert!(
        matches!(Stack::parse(&machine, "machine"), Err(Error::Invalid { key, .. }) if key == "framing.checksum_hex")
    );
    let mut machine: MachineDocument = load("machine", HL7)?;
    machine.syntax.edition = None;
    assert!(
        matches!(Stack::parse(&machine, "machine"), Err(Error::Invalid { key, .. }) if key == "syntax.edition")
    );
    Ok(())
}

#[test]
fn hl7_2_5_1_section_2_15_9_12_unsupported_edition_lists_qualified_result_editions() {
    let error = Error::UnsupportedHl7Edition {
        path: "machine.toml".into(),
        edition: "2.7",
    };
    assert!(
        error
            .to_string()
            .ends_with("supported: 2.3, 2.3.1, 2.4, 2.5, 2.5.1, 2.6, 2.8.2, 2.9.1")
    );
}

#[test]
fn adr0002_schema1_migrates_to_identical_schema2_canonical_bytes()
-> Result<(), Box<dyn std::error::Error>> {
    for (current, names) in [
        (ASTM, ["astm_lis01", "astm_link", "astm_lis02"]),
        (HL7, ["mllp", "hl7_receiver", "hl7_v2"]),
    ] {
        let mut old = current.replace("schema_version = 2", "schema_version = 1");
        for (section, name) in ["framing", "dialog", "syntax"].into_iter().zip(names) {
            old = old.replace(
                &format!("[{section}]"),
                &format!("[{section}]\nkind = '{name}'"),
            );
        }
        let canonical = |text: &str| -> Result<String, Error> {
            Ok(Draft::load(
                ("machine", text),
                ("site", SITE),
                ("overrides", "schema_version=1"),
            )?
            .validate()?
            .canonical()
            .to_owned())
        };
        assert_eq!(canonical(&old)?, canonical(current)?);
        for (section, name) in ["framing", "dialog", "syntax"].into_iter().zip(names) {
            let bad = old.replace(&format!("kind = '{name}'"), "kind = 'wrong'");
            assert!(
                matches!(canonical(&bad), Err(Error::Invalid { key, .. }) if key == format!("{section}.kind"))
            );
        }
    }
    Ok(())
}
