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

// ADR 0002, ADR 0005
#[test]
fn examples_validate_without_promoting_evidence() -> Result<(), Error> {
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
    assert!(matches!(
        draft(ASTM, OVERRIDES)?.validate()?.effective().stack(),
        analyte_config::Stack::Astm(_)
    ));
    assert!(matches!(
        draft(HL7, OVERRIDES)?.validate()?.effective().stack(),
        analyte_config::Stack::Hl7(_)
    ));
    Ok(())
}

// ADR 0002
#[test]
fn schema_rejects_versions_unknown_keys_and_secrets() {
    for bad in [
        ASTM.rewrite("schema_version = 2", "schema_version = 3"),
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

// Plan §5
#[test]
fn rejects_incompatible_stacks_and_bad_serial_settings() -> Result<(), Error> {
    for bad in [
        ASTM.rewrite("[framing]", "[framing]\nkind = 'mllp'"),
        ASTM.rewrite("kind = \"serial\"", "kind = \"watched_folder\""),
        ASTM.rewrite("baud = 9600", "baud = 0"),
        ASTM.rewrite("data_bits = 8", "data_bits = 9"),
        ASTM.rewrite("limit = 64000", "limit = 64001"),
        HL7.rewrite("[dialog]", "[dialog]\nkind = 'astm_link'"),
    ] {
        {
            let Err(refusal) = draft(&bad, OVERRIDES).and_then(Draft::validate) else {
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

// ADR 0002
#[test]
fn override_precedence_and_canonical_identity() -> Result<(), Error> {
    let a = draft(ASTM, "schema_version = 1\nendpoint = 'COM9'")?.validate()?;
    let b = draft(
        &format!("# ignored comment\n{ASTM}"),
        "endpoint = 'COM9'\nschema_version = 1",
    )?
    .validate()?;
    assert_eq!(a.canonical(), b.canonical());
    assert_eq!(a.effective().site().endpoint.fixed(), Ok("COM9"));
    assert_ne!(a.digest(), draft(ASTM, OVERRIDES)?.validate()?.digest());
    Ok(())
}

fn serial_site(location: &str) -> Result<analyte_config::Validated, Error> {
    Draft::load(
        ("machine.toml", ASTM),
        (
            "site.toml",
            &SITE.rewrite("endpoint = \"127.0.0.1:5600\"", location),
        ),
        ("local.toml", OVERRIDES),
    )?
    .validate()
}

// Plan §11
#[test]
fn site_names_a_com_port() -> Result<(), Error> {
    let config = serial_site("endpoint = \"COM9\"")?;
    assert_eq!(config.effective().site().endpoint.fixed(), Ok("COM9"));
    assert!(config.canonical().contains("endpoint = \"COM9\""));
    Ok(())
}

// Plan §11
#[test]
fn site_names_a_usb_serial_number() -> Result<(), Error> {
    let config = serial_site("usb_serial = \"A10K3XYZ\"")?;
    let analyte_config::Endpoint::UsbSerial(serial) = &config.effective().site().endpoint else {
        panic!("expected a USB serial number");
    };
    assert_eq!(serial.as_str(), "A10K3XYZ");
    assert!(config.canonical().contains("usb_serial = \"A10K3XYZ\""));
    assert!(!config.canonical().contains("endpoint ="));
    Ok(())
}

// Plan §11
#[test]
fn site_refuses_both_or_neither_port_forms() {
    let both = serial_site(
        "endpoint = \"COM9\"
usb_serial = \"A10K3XYZ\"",
    );
    assert!(
        matches!(&both, Err(Error::Invalid { key, reason, .. })
            if key == "usb_serial" && reason.contains("not both")),
        "{both:?}"
    );
    let neither = serial_site("");
    assert!(
        matches!(&neither, Err(Error::Invalid { key, reason, .. })
            if key == "endpoint" && reason.contains("usb_serial")),
        "{neither:?}"
    );
}

// Plan §11
#[test]
fn usb_serial_number_is_checked_once() {
    for bad in [
        "\"\"",
        "\" A10K3XYZ\"",
        "\"A10\tK3\"",
        &format!("{:?}", "9".repeat(129)),
    ] {
        let refused = serial_site(&format!("usb_serial = {bad}"));
        assert!(
            matches!(&refused, Err(Error::Schema { .. })),
            "{bad}: {refused:?}"
        );
    }
}

// Plan §11
#[test]
fn usb_serial_number_needs_a_serial_link() -> Result<(), Error> {
    let tcp = ASTM
        .rewrite("kind = \"serial\"", "kind = \"tcp_client\"")
        .rewrite(
            "[transport.serial]
baud = 9600
data_bits = 8
parity = \"none\"
stop_bits = 1
flow_control = \"none\"
",
            "",
        );
    let refused = Draft::load(
        ("machine.toml", &tcp),
        (
            "site.toml",
            &SITE.rewrite("endpoint = \"127.0.0.1:5600\"", "usb_serial = \"A10K3XYZ\""),
        ),
        ("local.toml", OVERRIDES),
    )?
    .validate();
    assert!(
        matches!(&refused, Err(Error::Invalid { key, .. }) if key == "usb_serial"),
        "{refused:?}"
    );
    Ok(())
}

// Plan §11
#[test]
fn override_switches_between_port_forms() -> Result<(), Error> {
    let usb = draft(
        ASTM,
        "schema_version = 1
usb_serial = 'A10K3XYZ'",
    )?
    .validate()?;
    assert!(matches!(
        &usb.effective().site().endpoint,
        analyte_config::Endpoint::UsbSerial(serial) if serial.as_str() == "A10K3XYZ"
    ));
    let both = draft(
        ASTM,
        "schema_version = 1
endpoint = 'COM9'
usb_serial = 'A10K3XYZ'",
    )?
    .validate();
    assert!(
        matches!(&both, Err(Error::Invalid { path, key, .. }) if path == "local.toml" && key == "usb_serial"),
        "{both:?}"
    );
    Ok(())
}

fn listening_site(peers: &str) -> Result<analyte_config::Validated, Error> {
    Draft::load(
        ("machine.toml", HL7),
        (
            "site.toml",
            &SITE.rewrite(
                "endpoint = \"127.0.0.1:5600\"",
                &format!("endpoint = \"0.0.0.0:5600\"\nallowed_peers = {peers}"),
            ),
        ),
        ("local.toml", OVERRIDES),
    )?
    .validate()
}

// ADR 0026 decision 1: a listening machine names the devices it accepts, as
// plain IP addresses; an IPv4 address written in its IPv6-mapped form is the
// same device, and the canonical settings keep the written list.
#[test]
fn listening_machine_names_the_devices_it_accepts() -> Result<(), Box<dyn std::error::Error>> {
    let config = listening_site("[\"192.168.1.50\", \"fd00::7\"]")?;
    let peers = config
        .effective()
        .site()
        .allowed_peers
        .as_ref()
        .ok_or("allowed_peers missing")?;
    for allowed in ["192.168.1.50", "::ffff:192.168.1.50", "fd00::7"] {
        assert!(peers.allows(allowed.parse()?), "{allowed}");
    }
    for refused in ["192.168.1.51", "127.0.0.1", "fd00::8"] {
        assert!(!peers.allows(refused.parse()?), "{refused}");
    }
    assert!(
        config
            .canonical()
            .contains("allowed_peers = [\"192.168.1.50\", \"fd00::7\"]"),
        "{}",
        config.canonical()
    );
    // Without the key the canonical bytes stay as before: no empty list appears.
    let open = draft(HL7, OVERRIDES)?.validate()?;
    assert!(open.effective().site().allowed_peers.is_none());
    assert!(!open.canonical().contains("allowed_peers"));
    Ok(())
}

// ADR 0026 decision 1: each address is checked once at the boundary; a
// hostname, a port, a range, a wildcard, a multicast address, a duplicate, an
// empty list or an overlong list is refused rather than guessed at.
#[test]
fn allowed_peers_are_checked_once() {
    let many = format!(
        "[{}]",
        (1..=17)
            .map(|n| format!("\"192.168.1.{n}\""))
            .collect::<Vec<_>>()
            .join(", ")
    );
    for bad in [
        "[]",
        "[\"analyzer.lab\"]",
        "[\"192.168.1.50:5600\"]",
        "[\"192.168.1.0/24\"]",
        "[\"0.0.0.0\"]",
        "[\"::\"]",
        "[\"224.0.0.1\"]",
        "[\"255.255.255.255\"]",
        "[\"192.168.1.50\", \"::ffff:192.168.1.50\"]",
        "[\" 192.168.1.50\"]",
        "\"192.168.1.50\"",
        many.as_str(),
    ] {
        let refused = listening_site(bad);
        assert!(
            matches!(&refused, Err(Error::Schema { .. })),
            "{bad}: {refused:?}"
        );
    }
}

// ADR 0026 decision 1: only a machine that connects to Analyte can limit who
// connects; a serial, folder or outgoing (tcp_client) machine refuses the key.
#[test]
fn allowed_peers_need_a_listening_machine() -> Result<(), Error> {
    let client = ASTM
        .rewrite("kind = \"serial\"", "kind = \"tcp_client\"")
        .rewrite(
            "[transport.serial]
baud = 9600
data_bits = 8
parity = \"none\"
stop_bits = 1
flow_control = \"none\"
",
            "",
        );
    let site = SITE.rewrite(
        "endpoint = \"127.0.0.1:5600\"",
        "endpoint = \"127.0.0.1:5600\"\nallowed_peers = [\"127.0.0.1\"]",
    );
    for (machine, site) in [
        (client.clone(), site.clone()),
        (
            ASTM.to_owned(),
            site.rewrite("endpoint = \"127.0.0.1:5600\"", "endpoint = \"COM9\""),
        ),
    ] {
        let refused = Draft::load(
            ("machine.toml", &machine),
            ("site.toml", &site),
            ("local.toml", OVERRIDES),
        )?
        .validate();
        assert!(
            matches!(&refused, Err(Error::Invalid { key, reason, .. })
                if key == "allowed_peers" && reason.contains("tcp_server")),
            "{refused:?}"
        );
    }
    Ok(())
}

// ADR 0003
#[test]
fn identity_stays_unresolved_without_evidence() -> Result<(), Error> {
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

// ADR 0002
#[test]
fn bounds_and_duplicate_tables_fail_visibly() -> Result<(), Error> {
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
            let Err(refusal) = draft(&bad, OVERRIDES).and_then(Draft::validate) else {
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

// ADR 0005
#[test]
fn claims_require_capability_evidence() -> Result<(), Error> {
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

// ADR 0007
#[test]
fn vendor_departures_need_named_reasons() -> Result<(), Error> {
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

// ADR 0003
#[test]
fn identity_settings_required_and_unknown_keys_rejected() {
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

// ADR 0003
#[test]
fn identity_selectors_are_validated() -> Result<(), Error> {
    let bad = ASTM.rewrite(
        "order_id = {group=\"order\",record=\"O\",field=3",
        "order_id = {group=\"order\",record=\"O\",field=0",
    );
    assert!(
        matches!(draft(&bad, OVERRIDES).and_then(Draft::validate), Err(Error::Invalid { key, .. }) if key == "mappings.identity.order_id")
    );
    let bad = ASTM.rewrite(
        "run = []",
        "run = [{group='observation',record='R',field=0,component=1,repetition=1}]",
    );
    assert!(
        matches!(draft(&bad, OVERRIDES).and_then(Draft::validate), Err(Error::Invalid { key, .. }) if key == "mappings.identity.run")
    );
    let bad = ASTM.rewrite(
        "correction_status_codes = [\"C\"]",
        "correction_status_codes = ['C','C']",
    );
    assert!(
        matches!(draft(&bad, OVERRIDES).and_then(Draft::validate), Err(Error::Invalid { key, .. }) if key == "mappings.identity.correction_status_codes")
    );
    Ok(())
}

// ADR 0002, ADR 0005
#[test]
fn vendor_layers_require_policy_and_evidence() -> Result<(), Error> {
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
            draft(&bad, OVERRIDES).and_then(Draft::validate),
            Err(Error::VendorFieldMappings { .. })
        ));
        let bad = example.rewrite("[framing]", "[framing]\nkind = 'mllp'");
        assert!(
            matches!(draft(&bad, OVERRIDES), Err(Error::Invalid { key, .. }) if key == "framing.kind")
        );
    }
    Ok(())
}

// ADR 0002
#[test]
fn vendor_policy_bounds_and_transports_are_specific() -> Result<(), Error> {
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

// ADR 0002
#[test]
fn invalid_component_parts_have_specific_errors() -> Result<(), Error> {
    for (declaration, expected) in [
        ("separator='^'\ncount=2\ncode=1\ndilution=2", 0),
        ("separator='/'\ncount=2\ncode=3\ndilution=2", 1),
        ("separator='/'\ncount=2\ncode=1\ndilution=1", 2),
    ] {
        let definition = format!("{ASTM}\n[mappings.component_parts]\n{declaration}\n");
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

// ADR 0029; LIS02-A2 §5.6.1: only ASTM names which R.3 component holds the
// machine's code, and it must; HL7 OBX-3 has one fixed place for it.
#[test]
fn test_component_is_required_for_astm_and_refused_elsewhere() -> Result<(), Error> {
    let without = ASTM.rewrite("test_component = \"manufacturer\"\n", "");
    let with_hl7 = HL7.rewrite(
        "test_system = \"synthetic\"",
        "test_system = \"synthetic\"\ntest_component = \"manufacturer\"",
    );
    for definition in [without, with_hl7] {
        let error = draft(&definition, OVERRIDES)?
            .validate()
            .expect_err("misplaced test_component");
        assert!(
            error.to_string().contains("mappings.test_component"),
            "{error}"
        );
    }
    let universal = ASTM.rewrite("\"manufacturer\"", "\"universal\"");
    draft(&universal, OVERRIDES)?.validate()?;
    let missing = ASTM.rewrite("test_system = \"synthetic\"\n", "");
    assert!(matches!(
        draft(&missing, OVERRIDES).and_then(Draft::validate),
        Err(Error::Schema { .. })
    ));
    Ok(())
}

// ADR 0029: a value expectation names a numeric or coded kind once per code.
#[test]
fn value_kinds_refuse_either_and_duplicates() -> Result<(), Error> {
    let with = |entries: &str| {
        ASTM.rewrite(
            "test_component = \"manufacturer\"",
            &format!("test_component = \"manufacturer\"\nvalue_kinds = [{entries}]"),
        )
    };
    draft(
        &with(concat!(
            "{code=\"SYN\", kind=\"numeric\"}, ",
            "{code=\"SYN\", system=\"LN\", kind=\"coded\"}"
        )),
        OVERRIDES,
    )?
    .validate()?;
    for entries in [
        "{code=\"SYN\", kind=\"either\"}",
        "{code=\"SYN\", kind=\"numeric\"}, {code=\"SYN\", kind=\"coded\"}",
    ] {
        let error = draft(&with(entries), OVERRIDES)?
            .validate()
            .expect_err("invalid value_kinds");
        assert!(
            error.to_string().contains("mappings.value_kinds"),
            "{error}"
        );
    }
    Ok(())
}

// ADR 0002
#[test]
fn vidas_sample_limit_preserves_numeric_toml() -> Result<(), Error> {
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

// HL7 2.3.1 §2.24.1.18
#[test]
fn hl7_utf8_requires_named_vendor_departure() -> Result<(), Error> {
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
    assert!(stack.settings().syntax().allow_utf8_in_v231());
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
    assert!(!stack.settings().syntax().allow_utf8_in_v231());
    Ok(())
}

// ADR 0004: the lab-app request deadline is a validated site setting, not a host constant.
// ADR 0004
#[test]
fn site_request_timeout_defaults_and_bounds() -> Result<(), Error> {
    let validate = |extra: &str| {
        Draft::load(
            ("machine.toml", HL7),
            ("site.toml", &format!("{SITE}{extra}")),
            ("local.toml", OVERRIDES),
        )
        .and_then(Draft::validate)
    };
    let configured = |config: &analyte_config::Validated| {
        config
            .effective()
            .site()
            .destination
            .request_timeout_seconds
    };
    assert_eq!(configured(&validate("")?), 10);
    for seconds in [1, 120] {
        let config = validate(&format!("request_timeout_seconds = {seconds}\n"))?;
        assert_eq!(configured(&config), seconds);
    }
    for seconds in [0, 121] {
        let refusal = validate(&format!("request_timeout_seconds = {seconds}\n"));
        let Err(Error::Invalid { key, .. }) = refusal else {
            panic!("{seconds} seconds must be refused with its key, got {refusal:?}")
        };
        assert_eq!(key, "site.destination.request_timeout_seconds");
    }
    Ok(())
}
