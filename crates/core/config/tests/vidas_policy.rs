//! Tests adr0002 vidas bci a ch4 ack and checksum choices keep boolean toml.
use analyte_config::{Draft, Stack};

#[test]
fn adr0002_vidas_bci_a_ch4_ack_and_checksum_choices_keep_boolean_toml()
-> Result<(), Box<dyn std::error::Error>> {
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
                definition.replace(&format!("{key} = {previous}"), &format!("{key} = {value}"));
        }
        let validated = Draft::load(
            ("machine", &definition),
            ("site", site),
            ("overrides", "schema_version=1"),
        )?
        .validate()?;
        let Stack::VidasLiteral(stack) = validated.effective().stack() else {
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
