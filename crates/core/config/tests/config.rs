//! Tests configuration loading and validation.
use analyte_config::{Draft, Error, EvidenceLevel};
use analyte_fixtures::ConfigEdit as _;
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
        ASTM.rewrite("schema_version = 1", "schema_version = 2"),
        ASTM.rewrite("name =", "unknown ="),
        ASTM.rewrite("baud = 9600", "baud = 9600\npassword = 'secret'"),
    ] {
        {
            let Err(refusal) = draft(&bad, OVERRIDES) else {
                panic!("input must be refused")
            };
            assert!(
                matches!(&refusal, Error::Schema { .. } | Error::Invalid { .. }),
                "unexpected refusal: {refusal:?}"
            );
        }
    }
    {
        let Err(refusal) = draft(ASTM, "schema_version = 1\nevidence = 'proven'") else {
            panic!("input must be refused")
        };
        assert!(
            matches!(&refusal, Error::Schema { .. } | Error::Invalid { .. }),
            "unexpected refusal: {refusal:?}"
        );
    }
}

#[test]
fn plan_5_rejects_incompatible_stacks_and_invalid_serial_settings() -> Result<(), Error> {
    for bad in [
        ASTM.rewrite("astm_lis01", "mllp"),
        ASTM.rewrite("kind = \"serial\"", "kind = \"watched_folder\""),
        ASTM.rewrite("baud = 9600", "baud = 0"),
        ASTM.rewrite("data_bits = 8", "data_bits = 9"),
        ASTM.rewrite("limit = 64000", "limit = 64001"),
        HL7.rewrite("hl7_receiver", "astm_link"),
    ] {
        {
            let Err(refusal) = draft(&bad, OVERRIDES)?.validate() else {
                panic!("input must be refused")
            };
            assert!(
                matches!(
                    &refusal,
                    Error::IncompatibleStack { .. }
                        | Error::Invalid { .. }
                        | Error::Departure { .. }
                ),
                "unexpected refusal: {refusal:?}"
            );
        }
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
            .stack()
            .field_mappings()
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
            .stack()
            .field_mappings()
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
        ASTM.rewrite("fields = 128", "fields = 0"),
        ASTM.rewrite("field = 3", "field = 0"),
        ASTM.rewrite(
            "units = [{original=\"mg/L\", ucum=\"mg/L\"}]",
            "units = [{original='mg/L',ucum='mg/L'},{original='mg/L',ucum='g/L'}]",
        ),
    ] {
        {
            let Err(refusal) = draft(&bad, OVERRIDES)?.validate() else {
                panic!("input must be refused")
            };
            assert!(
                matches!(&refusal, Error::Invalid { .. }),
                "unexpected refusal: {refusal:?}"
            );
        }
    }
    Ok(())
}

#[test]
fn adr_0005_claims_require_capability_evidence() -> Result<(), Error> {
    {
        let Err(refusal) = draft(
            &ASTM.rewrite("level = \"documented\"", "level = \"proven\""),
            OVERRIDES,
        )?
        .validate() else {
            panic!("input must be refused")
        };
        assert!(
            matches!(&refusal, Error::Invalid { .. }),
            "unexpected refusal: {refusal:?}"
        );
    }
    Ok(())
}

#[test]
fn adr_0007_vendor_departures_need_named_reasons() -> Result<(), Error> {
    let smaller = ASTM.rewrite("limit = 64000", "limit = 247");
    {
        let Err(refusal) = draft(&smaller, OVERRIDES)?.validate() else {
            panic!("input must be refused")
        };
        assert!(
            matches!(&refusal, Error::Departure { .. }),
            "unexpected refusal: {refusal:?}"
        );
    }
    let declared = smaller.rewrite("deviations = []", include_str!("fixtures/departure.toml"));
    assert_eq!(
        astm_frame_limit(&draft(&declared, OVERRIDES)?.validate()?)?,
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
    let unknown = ASTM.rewrite("run = []", "rerun = []");
    assert!(
        matches!(draft(&unknown, OVERRIDES), Err(Error::Schema { source, .. }) if source.message().contains("unknown field `rerun`"))
    );
    let ambiguous = ASTM.rewrite(
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
    let bad = ASTM.rewrite(
        "order_id = {group=\"order\",record=\"O\",field=3",
        "order_id = {group=\"order\",record=\"O\",field=0",
    );
    assert!(
        matches!(draft(&bad, OVERRIDES)?.validate(), Err(Error::Invalid { key, .. }) if key == "mappings.identity.order_id")
    );
    let bad = ASTM.rewrite(
        "run = []",
        "run = [{group='observation',record='R',field=0,component=1,repetition=1}]",
    );
    assert!(
        matches!(draft(&bad, OVERRIDES)?.validate(), Err(Error::Invalid { key, .. }) if key == "mappings.identity.run")
    );
    let bad = ASTM.rewrite(
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
        assert!(config.effective().stack().field_mappings().is_none());
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
                match config.effective().stack() {
                    analyte_config::Stack::Poct(_) => "poct",
                    analyte_config::Stack::Vidas(_) => "vidas_literal",
                    analyte_config::Stack::Autolyser(_) => "autolyser",
                    analyte_config::Stack::Astm(analyte_config::RecordTransport::Stream(_))
                    | analyte_config::Stack::Astm(analyte_config::RecordTransport::Folder(_)) =>
                        "astm_lis02",
                    analyte_config::Stack::Hl7(analyte_config::RecordTransport::Stream(_))
                    | analyte_config::Stack::Law(_)
                    | analyte_config::Stack::Hl7(analyte_config::RecordTransport::Folder(_)) =>
                        "hl7_v2",
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
        let invalid = example.rewrite(
            key,
            &format!(
                "{} = 0",
                key.split_once(" = ").map_or(key, |(name, _)| name)
            ),
        );
        assert!(
            matches!(draft(&invalid, OVERRIDES)?.validate(), Err(Error::ProtocolSettings { protocol: found, .. }) if found == protocol)
        );
        let missing = example.rewrite(key, "");
        assert!(matches!(
            draft(&missing, OVERRIDES),
            Err(Error::Schema { .. })
        ));
        let folder = example.rewrite("kind = \"tcp_server\"", "kind = \"watched_folder\"");
        assert!(matches!(
            draft(&folder, OVERRIDES)?.validate(),
            Err(Error::IncompatibleStack { .. })
        ));
    }
    Ok(())
}

#[test]
fn adr0002_component_parts_invalid_declarations_have_specific_errors() -> Result<(), Error> {
    for (declaration, expected) in [
        ("separator='^'\ncount=2\ncode=1\ndilution=2", 0),
        ("separator='/'\ncount=2\ncode=3\ndilution=2", 1),
        ("separator='/'\ncount=2\ncode=1\ndilution=1", 2),
    ] {
        let definition =
            format!("{ASTM}\n[mappings.component_parts]\ncomponent=4\n{declaration}\n");
        let error = draft(&definition, OVERRIDES)?
            .validate()
            .expect_err("invalid parts");
        assert!(matches!(
            (expected, error),
            (0, Error::PartSeparator { .. })
                | (1, Error::PartIndex { .. })
                | (2, Error::PartOverlap { .. })
        ));
    }
    Ok(())
}

#[test]
fn adr_0002_vidas_installed_sample_limit_preserves_numeric_toml() -> Result<(), Error> {
    use analyte_config::VidasSampleLimit;
    let definition = include_str!("../examples/vidas_literal.toml");
    for (number, expected) in [
        (12, VidasSampleLimit::Twelve),
        (17, VidasSampleLimit::Seventeen),
    ] {
        let definition =
            definition.rewrite("sample_limit = 12", &format!("sample_limit = {number}"));
        let valid = draft(&definition, OVERRIDES)?.validate()?;
        assert_eq!(vidas(&valid)?.sample_limit, expected);
        assert!(
            valid
                .canonical()
                .contains(&format!("sample_limit = {number}"))
        );
    }
    for number in [0, 11, 13, 16, 18, 255] {
        let definition =
            definition.rewrite("sample_limit = 12", &format!("sample_limit = {number}"));
        {
            let Err(refusal) = draft(&definition, OVERRIDES) else {
                panic!("input must be refused")
            };
            assert!(
                matches!(&refusal, Error::Schema { .. }),
                "unexpected refusal: {refusal:?}"
            );
        }
    }
    Ok(())
}

fn vidas(config: &analyte_config::Validated) -> Result<&analyte_config::Vidas, Error> {
    let analyte_config::Stack::Vidas(stack) = config.effective().stack() else {
        return Err(Error::ProtocolSettings {
            path: "fixture".into(),
            protocol: "vidas",
        });
    };
    Ok(stack.policy())
}
fn astm_frame_limit(config: &analyte_config::Validated) -> Result<usize, Error> {
    let analyte_config::Stack::Astm(analyte_config::RecordTransport::Stream(stack)) =
        config.effective().stack()
    else {
        return Err(Error::ProtocolSettings {
            path: "fixture".into(),
            protocol: "astm",
        });
    };
    Ok(stack.settings().frame_limit())
}

#[test]
fn hl7_231_2_24_1_18_utf8_requires_named_vendor_departure() -> Result<(), Error> {
    let base = HL7
        .rewrite("edition = \"v251\"", "edition = \"v231\"")
        .rewrite("encoding = \"ascii\"", "encoding = \"utf8\"");
    let selected = base.rewrite(
        "escapes = \"reject\"",
        "escapes = \"reject\"\nallow_utf8_in_v231 = true",
    );
    {
        let Err(refusal) = draft(&selected, OVERRIDES)?.validate() else {
            panic!("input must be refused")
        };
        assert!(
            matches!(&refusal, Error::Departure { .. }),
            "unexpected refusal: {refusal:?}"
        );
    }
    let declared = selected.rewrite(
        "deviations = []",
        &include_str!("fixtures/departure.toml")
            .rewrite("astm_frame_limit", "hl7_utf8_in_v231")
            .rewrite("protocol=\"astm\"", "protocol=\"hl7\",edition=\"v231\"")
            .rewrite("astm_results", "oru_r01_patient"),
    );
    let config = draft(&declared, OVERRIDES)?.validate()?;
    let analyte_config::Stack::Hl7(analyte_config::RecordTransport::Stream(stack)) =
        config.effective().stack()
    else {
        return Err(Error::ProtocolSettings {
            path: "fixture".into(),
            protocol: "hl7",
        });
    };
    assert!(stack.settings().syntax().options().allow_utf8_in_v231);
    for invalid in [
        declared.rewrite("edition = \"v231\"", "edition = \"v251\""),
        declared.rewrite("encoding = \"utf8\"", "encoding = \"ascii\""),
    ] {
        {
            let Err(refusal) = draft(&invalid, OVERRIDES)?.validate() else {
                panic!("input must be refused")
            };
            assert!(
                matches!(&refusal, Error::Departure { .. }),
                "unexpected refusal: {refusal:?}"
            );
        }
    }
    let strict = draft(&base, OVERRIDES)?.validate()?;
    let analyte_config::Stack::Hl7(analyte_config::RecordTransport::Stream(stack)) =
        strict.effective().stack()
    else {
        return Err(Error::ProtocolSettings {
            path: "fixture".into(),
            protocol: "hl7",
        });
    };
    assert!(!stack.settings().syntax().options().allow_utf8_in_v231);
    Ok(())
}
