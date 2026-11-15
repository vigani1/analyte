//! Synthetic configuration cases traceable to the project decisions.
use analyte_config::{Draft, Error, EvidenceLevel};
use sha2::{Digest, Sha256};
const ASTM: &str = include_str!("../examples/astm.toml");
const HL7: &str = include_str!("../examples/hl7.toml");
const SITE: &str = include_str!("../examples/site.toml");
const OVERRIDES: &str = "schema_version = 1";

fn draft(machine: &str, overrides: &str) -> Result<Draft, Error> {
    Draft::load(
        ("machine.toml", machine),
        ("site.toml", SITE),
        ("local.toml", overrides),
    )
}

#[test]
fn adr_0002_and_0005_examples_validate_without_promoting_evidence() -> Result<(), Error> {
    for example in [ASTM, HL7] {
        let config = draft(example, OVERRIDES)?.validate()?;
        assert_eq!(
            config.effective().machine().evidence.level,
            EvidenceLevel::Documented
        );
        let digest: [u8; 32] = Sha256::digest(config.canonical().as_bytes()).into();
        assert_eq!(config.digest(), digest);
        assert_eq!(config.activate().snapshot().digest(), digest);
    }
    assert!(
        draft(ASTM, OVERRIDES)?
            .validate()?
            .effective()
            .astm_limits()
            .is_some()
    );
    assert!(
        draft(HL7, OVERRIDES)?
            .validate()?
            .effective()
            .hl7_options()
            .is_some()
    );
    Ok(())
}

#[test]
fn adr_0002_schema_rejects_versions_unknown_keys_and_secrets() {
    for bad in [
        ASTM.replace("schema_version = 1", "schema_version = 2"),
        ASTM.replace("name =", "unknown ="),
        ASTM.replace("baud = 9600", "baud = 9600\npassword = 'secret'"),
    ] {
        assert!(draft(&bad, OVERRIDES).is_err());
    }
    assert!(draft(ASTM, "schema_version = 1\nevidence = 'proven'").is_err());
}

#[test]
fn plan_5_rejects_incompatible_stacks_and_invalid_serial_settings() -> Result<(), Error> {
    for bad in [
        ASTM.replace("astm_lis01", "mllp"),
        ASTM.replace("kind = \"serial\"", "kind = \"watched_folder\""),
        ASTM.replace("baud = 9600", "baud = 0"),
        ASTM.replace("data_bits = 8", "data_bits = 9"),
        ASTM.replace("limit = 64000", "limit = 64001"),
        HL7.replace("hl7_receiver", "astm_link"),
    ] {
        assert!(draft(&bad, OVERRIDES)?.validate().is_err());
    }
    Ok(())
}

#[test]
fn adr_0002_override_precedence_and_canonical_identity() -> Result<(), Error> {
    let a = draft(ASTM, "schema_version = 1\nendpoint = 'COM9'")?.validate()?;
    let b = draft(
        &format!("# ignored comment\n{ASTM}"),
        "endpoint = 'COM9'\nschema_version = 1",
    )?
    .validate()?;
    assert_eq!(a.canonical(), b.canonical());
    assert_eq!(a.effective().site().endpoint, "COM9");
    assert_ne!(a.digest(), draft(ASTM, OVERRIDES)?.validate()?.digest());
    Ok(())
}

#[test]
fn adr_0003_identity_remains_unresolved_without_evidence() -> Result<(), Error> {
    let config = draft(ASTM, OVERRIDES)?.validate()?;
    assert!(
        config
            .effective()
            .machine()
            .mappings
            .as_ref()
            .ok_or_else(|| Error::FileSize {
                path: "fixture".into()
            })?
            .identity
            .run
            .is_empty()
    );
    assert!(
        config
            .effective()
            .machine()
            .mappings
            .as_ref()
            .ok_or_else(|| Error::FileSize {
                path: "fixture".into()
            })?
            .identity
            .correction_status_codes
            .iter()
            .all(|code| code.as_str() == "C")
    );
    Ok(())
}

#[test]
fn adr_0002_bounds_and_duplicate_tables_fail_visibly() -> Result<(), Error> {
    let oversized = " ".repeat(analyte_config::MAX_FILE_BYTES.saturating_add(1));
    assert!(matches!(
        draft(&oversized, OVERRIDES),
        Err(Error::FileSize { .. })
    ));
    for bad in [
        ASTM.replace("fields = 128", "fields = 0"),
        ASTM.replace("field = 3", "field = 0"),
        ASTM.replace(
            "units = [{original=\"mg/L\", ucum=\"mg/L\"}]",
            "units = [{original='mg/L',ucum='mg/L'},{original='mg/L',ucum='g/L'}]",
        ),
    ] {
        assert!(draft(&bad, OVERRIDES)?.validate().is_err());
    }
    Ok(())
}

#[test]
fn adr_0005_claims_require_capability_evidence() -> Result<(), Error> {
    assert!(
        draft(
            &ASTM.replace("level = \"documented\"", "level = \"proven\""),
            OVERRIDES
        )?
        .validate()
        .is_err()
    );
    Ok(())
}

#[test]
fn adr_0007_vendor_departures_need_named_reasons() -> Result<(), Error> {
    let smaller = ASTM.replace("limit = 64000", "limit = 247");
    assert!(draft(&smaller, OVERRIDES)?.validate().is_err());
    let declared = smaller.replace("deviations = []", "deviations = [{name='small-frame',key='framing.limit',reason='Synthetic vendor requires 247 bytes'}]");
    assert_eq!(
        draft(&declared, OVERRIDES)?
            .validate()?
            .effective()
            .machine()
            .framing
            .limit,
        247
    );
    Ok(())
}

#[test]
fn adr0003_identity_settings_are_required_and_unknown_keys_are_rejected() {
    for (setting, line) in [
        (
            "order_id",
            "order_id = {group=\"order\",record=\"O\",field=3,component=1,repetition=1}\n",
        ),
        (
            "order_namespace",
            "order_namespace = {fixed=\"synthetic-orders\"}\n",
        ),
    ] {
        let missing = ASTM
            .lines()
            .filter(|candidate| !candidate.starts_with(line.trim_end()))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            matches!(draft(&missing, OVERRIDES), Err(Error::MissingIdentitySetting { setting: actual, .. }) if actual == setting)
        );
    }
    let unknown = ASTM.replace("run = []", "rerun = []");
    assert!(
        matches!(draft(&unknown, OVERRIDES), Err(Error::Schema { source, .. }) if source.message().contains("unknown field `rerun`"))
    );
    let ambiguous = ASTM.replace(
        "{fixed=\"synthetic-orders\"}",
        "{fixed='orders', field={group='order',record='O',field=3,component=1,repetition=1}}",
    );
    assert!(matches!(
        draft(&ambiguous, OVERRIDES),
        Err(Error::Schema { .. })
    ));
}

#[test]
fn adr0003_identity_selectors_are_validated() -> Result<(), Error> {
    let bad = ASTM.replace(
        "order_id = {group=\"order\",record=\"O\",field=3",
        "order_id = {group=\"order\",record=\"O\",field=0",
    );
    assert!(
        matches!(draft(&bad, OVERRIDES)?.validate(), Err(Error::Invalid { key, .. }) if key == "mappings.identity.order_id")
    );
    let bad = ASTM.replace(
        "run = []",
        "run = [{group='observation',record='R',field=0,component=1,repetition=1}]",
    );
    assert!(
        matches!(draft(&bad, OVERRIDES)?.validate(), Err(Error::Invalid { key, .. }) if key == "mappings.identity.run")
    );
    let bad = ASTM.replace(
        "correction_status_codes = [\"C\"]",
        "correction_status_codes = ['C','C']",
    );
    assert!(
        matches!(draft(&bad, OVERRIDES)?.validate(), Err(Error::Invalid { key, .. }) if key == "mappings.identity.correction_status_codes")
    );
    Ok(())
}

#[test]
fn adr0002_0005_vendor_layers_require_explicit_policy_and_documented_evidence() -> Result<(), Error>
{
    for example in [
        include_str!("../examples/poct.toml"),
        include_str!("../examples/vidas_literal.toml"),
        include_str!("../examples/autolyser.toml"),
    ] {
        let config = draft(example, OVERRIDES)?.validate()?;
        assert!(config.effective().machine().mappings.is_none());
        assert_eq!(
            config.effective().machine().evidence.level,
            EvidenceLevel::Documented
        );
        // A complete field-position section is rejected by the semantic boundary.
        let fields = &HL7[HL7.find("[mappings]").ok_or_else(|| Error::FileSize {
            path: "fixture".into(),
        })?..HL7.find("[evidence]").ok_or_else(|| Error::FileSize {
            path: "fixture".into(),
        })?];
        let bad = format!("{example}\n{fields}");
        assert!(matches!(
            draft(&bad, OVERRIDES)?.validate(),
            Err(Error::VendorFieldMappings { .. })
        ));
        let bad = example.replacen(
            &format!(
                "kind = \"{}\"",
                match config.effective().machine().syntax.kind {
                    analyte_config::SyntaxKind::Poct => "poct",
                    analyte_config::SyntaxKind::VidasLiteral => "vidas_literal",
                    analyte_config::SyntaxKind::Autolyser => "autolyser",
                    analyte_config::SyntaxKind::AstmLis02 => "astm_lis02",
                    analyte_config::SyntaxKind::Hl7V2 => "hl7_v2",
                }
            ),
            "kind = \"mllp\"",
            1,
        );
        assert!(matches!(
            draft(&bad, OVERRIDES)?.validate(),
            Err(Error::IncompatibleStack { .. })
        ));
    }
    Ok(())
}

#[test]
fn adr0002_vendor_policy_bounds_and_transports_are_specific() -> Result<(), Error> {
    for (example, key, protocol) in [
        (
            include_str!("../examples/poct.toml"),
            "response_ms = 30000",
            "poct",
        ),
        (
            include_str!("../examples/vidas_literal.toml"),
            "chunk = 80",
            "vidas",
        ),
        (
            include_str!("../examples/autolyser.toml"),
            "poll_ms = 1000",
            "autolyser",
        ),
    ] {
        let invalid = example.replace(
            key,
            &format!(
                "{} = 0",
                key.split_once(" = ").map_or(key, |(name, _)| name)
            ),
        );
        assert!(
            matches!(draft(&invalid, OVERRIDES)?.validate(), Err(Error::ProtocolSettings { protocol: found, .. }) if found == protocol)
        );
        let missing = example.replace(key, "");
        assert!(matches!(
            draft(&missing, OVERRIDES),
            Err(Error::Schema { .. })
        ));
        let folder = example.replace("kind = \"tcp_server\"", "kind = \"watched_folder\"");
        assert!(matches!(
            draft(&folder, OVERRIDES)?.validate(),
            Err(Error::IncompatibleStack { .. })
        ));
    }
    Ok(())
}
