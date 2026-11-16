//! Tests src behavior with synthetic fixtures.
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
