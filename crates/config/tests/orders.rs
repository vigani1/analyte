//! ADR 0002: outbound capabilities must be explicitly configured.
use analyte_config::{Draft, Validated};
type TestResult = Result<(), Box<dyn std::error::Error>>;
fn config(section: &str) -> Result<Validated, analyte_config::Error> {
    let definition = format!("{}\n{section}", include_str!("../examples/astm.toml"));
    Draft::load(
        ("machine", &definition),
        ("site", include_str!("../examples/site.toml")),
        ("overrides", "schema_version=1"),
    )?
    .validate()
}
#[test]
fn adr0002_outbound_duplicate_missing_policy_and_unbounded_deadline_are_rejected() -> TestResult {
    let section = include_str!("../examples/orders.toml");
    for invalid in [section.replace("deadline_ms = 1000", "deadline_ms = 0"), section.replace("when_untrusted = \"error\"", ""), section.replace("send_ahead = true", ""), section.replace("machine_components=[\"\", \"\", \"\", \"SEND\"]}", "machine_components=[\"SEND\"]},{app={namespace=\"demo\",value=\"DEMO\"},machine_components=[\"OTHER\"]}")] {
        assert!(config(&invalid).is_err());
    }
    let valid = config(section)?;
    assert!(valid.canonical().contains("[machine.orders]"));
    Ok(())
}
