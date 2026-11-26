//! Tests the site's repeated-results choice: its default, its spellings and where it applies (ADR 0035).
use analyte_config::{Draft, Error, RepeatedResults, Validated};
const ASTM: &str = include_str!("../examples/astm.toml");
const HL7: &str = include_str!("../examples/hl7.toml");
const LAW: &str = include_str!("../examples/law.toml");
const POCT: &str = include_str!("../examples/poct.toml");
const SITE: &str = include_str!("../examples/site.toml");

/// The example site file with `line` among its top-level keys.
fn site_with(line: &str) -> String {
    SITE.replacen("[destination]", &format!("{line}\n[destination]"), 1)
}

fn load(machine: &str, site: &str) -> Result<Validated, Error> {
    Draft::load(
        ("machine.toml", machine),
        ("site.toml", site),
        ("local.toml", "schema_version = 1"),
    )?
    .validate()
}

// ADR 0035 (2026-11-26): a site file without the key handles repeats
// automatically and keeps its bytes; `automatic` written out is the same snapshot.
#[test]
fn automatic_is_the_default_and_keeps_the_snapshot() -> Result<(), Error> {
    for machine in [ASTM, HL7] {
        let plain = load(machine, SITE)?;
        assert_eq!(
            plain.effective().repeated_results(),
            Some(RepeatedResults::Automatic)
        );
        assert!(!plain.canonical().contains("repeated_results"));
        let written = load(machine, &site_with("repeated_results = \"automatic\""))?;
        assert_eq!(written.digest(), plain.digest());
    }
    Ok(())
}

// ADR 0035 (2026-11-26): "always ask me" loads, is part of the snapshot, and
// so changes the settings version.
#[test]
fn always_ask_loads_into_the_snapshot() -> Result<(), Error> {
    let plain = load(HL7, SITE)?;
    let ask = load(HL7, &site_with("repeated_results = \"always_ask\""))?;
    assert_eq!(
        ask.effective().repeated_results(),
        Some(RepeatedResults::AlwaysAsk)
    );
    assert_eq!(
        ask.effective().site().repeated_results,
        RepeatedResults::AlwaysAsk
    );
    assert!(
        ask.canonical()
            .contains("repeated_results = \"always_ask\"")
    );
    assert_ne!(ask.digest(), plain.digest());
    Ok(())
}

// ADR 0035 (2026-11-26): an unknown value is refused when the site file loads.
#[test]
fn an_unknown_value_is_refused() {
    for value in [
        "\"ask\"",
        "\"Always_Ask\"",
        "\"manual\"",
        "\"\"",
        "true",
        "1",
    ] {
        let site = site_with(&format!("repeated_results = {value}"));
        assert!(
            matches!(load(HL7, &site), Err(Error::Schema { .. })),
            "{value} must be refused"
        );
    }
}

// ADR 0035 (2026-11-26): IHE LAW and POCT keep their protocol-owned rules, so
// "always ask me" is refused for them rather than silently ignored.
#[test]
fn always_ask_is_refused_where_protocol_rules_apply() -> Result<(), Error> {
    for machine in [LAW, POCT] {
        assert_eq!(load(machine, SITE)?.effective().repeated_results(), None);
        let refused = load(machine, &site_with("repeated_results = \"always_ask\""));
        assert!(
            matches!(&refused, Err(Error::Invalid { key, .. }) if key == "repeated_results"),
            "{refused:?}"
        );
    }
    Ok(())
}
