//! Tests VIDAS policy settings and rejects obsolete keys.
use analyte_config::{Draft, Error, Stack};
use analyte_fixtures::ConfigEdit as _;

// ADR 0002: report specific configuration errors at load time.
#[test]
fn vidas_rejects_removed_retry_keys() {
    let definition = include_str!("../examples/vidas_literal.toml");
    let site = include_str!("../examples/site.toml");
    for (key, value) in [("retries", 0), ("backoff_ms", 1000)] {
        let obsolete = definition.rewrite("[vidas]", &format!("[vidas]\n{key} = {value}"));
        let result = Draft::load(
            ("machine", &obsolete),
            ("site", site),
            ("overrides", "schema_version=1"),
        );
        assert!(
            matches!(
                result,
                Err(Error::Schema { source, .. })
                    if source.message().contains(&format!("unknown field `{key}`"))
            ),
            "expected an unknown-field error for `{key}`"
        );
    }
}

// ADR 0002; VIDAS BCI Link rev C appendix B.
#[test]
fn vidas_definitions_select_a_closed_control_label_policy() -> Result<(), Box<dyn std::error::Error>>
{
    let definition = include_str!("../examples/vidas_literal.toml");
    let site = include_str!("../examples/site.toml");
    let load = |definition: &str| {
        Draft::load(
            ("machine", definition),
            ("site", site),
            ("overrides", "schema_version=1"),
        )?
        .validate()
    };
    let chosen = definition.rewrite("[vidas]", "[vidas]\nresult_control_labels = 'appendix_b'");
    let config = load(&chosen)?;
    let Stack::Vidas(stack) = config.effective().stack() else {
        return Err("VIDAS stack expected".into());
    };
    assert_eq!(
        stack.policy().result_control_labels,
        analyte_config::VidasResultControlLabels::AppendixB,
    );
    assert!(load(&chosen.rewrite("'appendix_b'", "'match_prefix'")).is_err());
    Ok(())
}

// ADR 0002; VIDAS BCI Link rev A chapter 4
#[test]
fn vidas_ack_and_checksum_choices_keep_boolean_toml() -> Result<(), Box<dyn std::error::Error>> {
    let original = include_str!("../examples/vidas_literal.toml");
    let site = include_str!("../examples/site.toml");
    let keys = [
        "alternate",
        "include_stx",
        "include_suffix",
        "uppercase",
        "accept_either_case",
    ];
    for selected in 0_u8..32 {
        let mut definition = original.to_owned();
        for (bit, key) in keys.iter().enumerate() {
            let previous = *key == "uppercase";
            let value = selected & (1 << bit) != 0;
            definition =
                definition.rewrite(&format!("{key} = {previous}"), &format!("{key} = {value}"));
        }
        let validated = Draft::load(
            ("machine", &definition),
            ("site", site),
            ("overrides", "schema_version=1"),
        )?
        .validate()?;
        let Stack::Vidas(stack) = validated.effective().stack() else {
            return Err("VIDAS stack expected".into());
        };
        let policy = stack.policy();
        let actual = [
            bool::from(policy.ack_point),
            bool::from(policy.include_stx),
            bool::from(policy.include_suffix),
            bool::from(policy.hex_case),
            bool::from(policy.checksum_acceptance),
        ];
        for (bit, value) in actual.into_iter().enumerate() {
            assert_eq!(value, selected & (1 << bit) != 0);
        }
        let parsed: toml::Value = toml::from_str(&definition)?;
        assert_eq!(parsed.get("vidas"), Some(&toml::Value::try_from(policy)?));
    }
    Ok(())
}
