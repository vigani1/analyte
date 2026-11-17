//! Tests ADR 0015 load-time refusals and exact declaration provenance.
use analyte_config::{DepartureRefusal as R, Draft, Error};
use analyte_fixtures::ConfigEdit as _;
use sha2::{Digest, Sha256};
use toml::Value;

const BASE: &str = include_str!("../examples/astm.toml");
const SITE: &str = include_str!("../examples/site.toml");
const DECLARATION: &str = include_str!("fixtures/departure.toml");

#[expect(
    clippy::expect_used,
    reason = "static synthetic fixture construction must succeed"
)]
fn machine() -> Value {
    toml::from_str(
        &BASE
            .rewrite("deviations = []", DECLARATION)
            .rewrite("limit = 64000", "limit = 247"),
    )
    .expect("valid synthetic TOML")
}
#[expect(
    clippy::expect_used,
    reason = "static synthetic fixture construction must succeed"
)]
fn validate(value: &Value) -> Result<analyte_config::Validated, Error> {
    Draft::load(
        (
            "machine",
            &toml::to_string(value).expect("serialize fixture"),
        ),
        ("site", SITE),
        ("overrides", "schema_version=1"),
    )?
    .validate()
}
#[expect(
    clippy::indexing_slicing,
    reason = "test fixture has known scope and declaration fields"
)]
fn declaration(value: &mut Value) -> &mut Value {
    &mut value["deviations"][0]["declaration"]
}
fn assert_refused(value: &Value, expected: R) {
    assert!(
        matches!(validate(value), Err(Error::Departure { reason, .. }) if reason == expected),
        "{expected}: {:?}",
        validate(value)
    );
}
#[expect(
    clippy::indexing_slicing,
    reason = "test fixture has known scope and declaration fields"
)]
fn change_scope(value: &mut Value, key: &str, replacement: Value) {
    value["departure_scope"][key] = replacement.clone();
    declaration(value)["scope"][key] = replacement;
}

#[test]
fn adr0015_validate_once_rejects_legacy_unknown_tags_and_fields() {
    let legacy = BASE.rewrite(
        "deviations = []",
        "deviations=[{name='old',key='framing.limit',reason='not evidence'}]",
    );
    assert!(matches!(
        Draft::load(
            ("machine", &legacy),
            ("site", SITE),
            ("overrides", "schema_version=1")
        ),
        Err(Error::Departure {
            reason: R::LegacyDeclaration,
            ..
        })
    ));
    for (old, new) in [
        ("astm_frame_limit", "ignore_validation"),
        ("revision=", "typo="),
    ] {
        let invalid = BASE.rewrite("deviations = []", &DECLARATION.rewrite(old, new));
        assert!(matches!(
            Draft::load(
                ("machine", &invalid),
                ("site", SITE),
                ("overrides", "schema_version=1")
            ),
            Err(Error::Schema { .. })
        ));
    }
}

#[test]
fn adr0015_validate_once_duplicate_conflict_and_resource_refusals() {
    let mut value = machine();
    let item = value["deviations"][0].clone();
    value["deviations"]
        .as_array_mut()
        .expect("array")
        .push(item.clone());
    assert_refused(&value, R::DuplicateRule);
    value["deviations"] = Value::Array(vec![item; 13]);
    assert_refused(&value, R::TooManyDeclarations);
    let mut value = machine();
    declaration(&mut value)["evidence"] = Value::Array(vec![]);
    assert_refused(&value, R::EmptyEvidence);
    let mut value = machine();
    let reference = declaration(&mut value)["evidence"][0].clone();
    declaration(&mut value)["evidence"] = Value::Array(vec![reference; 17]);
    assert_refused(&value, R::EvidenceLimit);
}

#[test]
fn adr0015_scope_model_firmware_mode_and_selected_protocol_are_exact() {
    for key in ["model", "firmware", "mode", "message", "direction"] {
        let mut value = machine();
        let replacement = match key {
            "message" => "astm_link",
            "direction" => "host_to_machine",
            _ => "other",
        };
        declaration(&mut value)["scope"][key] = replacement.into();
        assert_refused(&value, R::ScopeMismatch);
    }
    let mut value = machine();
    value
        .as_table_mut()
        .expect("table")
        .remove("departure_scope");
    assert_refused(&value, R::MissingInstalledScope);
    let mut value = machine();
    change_scope(&mut value, "model", "*".into());
    assert_refused(&value, R::UnsupportedApplicability);
    let mut value = machine();
    change_scope(&mut value, "firmware", "unknown".into());
    assert_refused(&value, R::UnknownFirmware);
    declaration(&mut value)["evidence"][0]["covers_unknown_firmware"] = true.into();
    assert!(validate(&value).is_ok());
    value["deviations"][0]["rule"] = "hl7_absent_obr25_means_final".into();
    assert_refused(&value, R::ProtocolMismatch);
}

#[test]
fn adr0015_existing_settings_require_active_evidenced_declarations() {
    let mut value = machine();
    value["framing"]["limit"] = 64000.into();
    assert_refused(&value, R::InactiveSetting);
    value["framing"]["limit"] = 247.into();
    value["deviations"] = Value::Array(vec![]);
    assert_refused(&value, R::MissingDeclaration);
    let mut value = machine();
    declaration(&mut value)["evidence"][0]["contradicts"] = true.into();
    assert_refused(&value, R::ContradictoryEvidence);
    let mut value = machine();
    declaration(&mut value)["evidence"][0]["reference"] = "C:/protected/manual.pdf".into();
    assert_refused(&value, R::InvalidManualReference);
    for level in ["simulated", "proven"] {
        let mut value = machine();
        declaration(&mut value)["level"] = level.into();
        assert_refused(&value, R::LevelEvidence);
    }
}

#[expect(
    clippy::expect_used,
    reason = "static synthetic fixture construction must succeed"
)]
fn hl7() -> Value {
    let fixture = DECLARATION
        .rewrite("astm_frame_limit", "hl7_absent_obr25_means_final")
        .rewrite("protocol=\"astm\"", "protocol=\"hl7\",edition=\"v251\"")
        .rewrite("astm_results", "oru_r01_patient")
        .rewrite("compatibility", "absent_obr25_final");
    toml::from_str(&include_str!("../examples/hl7.toml").rewrite("deviations = []", &fixture))
        .expect("fixture")
}

#[test]
fn adr0015_status_rules_require_separate_documented_omission_guarantees() {
    let mut value = hl7();
    assert!(validate(&value).is_ok());
    value["deviations"][0]["rule"] = "hl7_absent_obx11_means_final".into();
    assert_refused(&value, R::MissingRuleEvidence);
    declaration(&mut value)["evidence"][0]["guarantee"] = "absent_obx11_final".into();
    let checked = validate(&value).expect("explicit OBX omission guarantee");
    assert!(checked.canonical().contains("absent_obx11_final"));
    assert!(checked.canonical().contains("synthetic-test-policy"));
    change_scope(&mut value, "direction", "host_to_machine".into());
    assert_refused(&value, R::DirectionMismatch);
    change_scope(&mut value, "direction", "machine_to_host".into());
    change_scope(&mut value, "message", "astm_results".into());
    assert_refused(&value, R::MessageMismatch);
    change_scope(&mut value, "message", "oru_r01_patient".into());
    let edition: Value = toml::from_str("protocol='hl7'\nedition='v231'").expect("edition");
    change_scope(&mut value, "edition", edition);
    assert_refused(&value, R::EditionMismatch);
}

#[test]
fn adr0015_recording_references_are_local_resolved_and_digest_checked() {
    let mut value = machine();
    let reference: Value = toml::from_str(
        "kind='recording'\nfile='../bad.rec.jsonl'\nsha256='bad'\nfirst_event=1\nlast_event=1",
    )
    .expect("reference");
    declaration(&mut value)["evidence"] = Value::Array(vec![reference]);
    assert_refused(&value, R::InvalidLocalReference);
    declaration(&mut value)["evidence"][0]["file"] = "does-not-exist.rec.jsonl".into();
    assert_refused(&value, R::UnresolvedRecording);
    // Each process has a unique directory; no analyzer or protected store is contacted.
    let directory =
        std::env::temp_dir().join(format!("analyte-departure-config-{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("temporary directory");
    let recording = directory.join("test.rec.jsonl");
    let bytes = concat!(
        "{\"version\":1,\"stack\":[\"serial\",\"astm_lis01\",\"astm_link\",\"astm_lis02\"],\"machine\":\"synthetic\",\"software\":\"test-1\",\"evidence\":\"synthetic\",\"de_identification\":\"Synthetic\",\"source\":\"ADR 0015 test\"}\n",
        "{\"direction\":\"machine_to_host\",\"time_ms\":0,\"bytes\":\"BQ==\"}\n"
    );
    std::fs::write(&recording, bytes).expect("synthetic recording");
    declaration(&mut value)["evidence"][0]["file"] = "test.rec.jsonl".into();
    let path = directory
        .join("machine.toml")
        .to_string_lossy()
        .into_owned();
    let check = |value: &Value| {
        Draft::load(
            (&path, &toml::to_string(value).expect("serialize fixture")),
            ("site", SITE),
            ("overrides", "schema_version=1"),
        )
        .expect("load")
        .validate()
    };
    assert!(matches!(
        check(&value),
        Err(Error::Departure {
            reason: R::DigestMismatch,
            ..
        })
    ));
    let digest = Sha256::digest(bytes.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    declaration(&mut value)["evidence"][0]["sha256"] = digest.into();
    // A synthetic recording cannot supply even a compatibility guarantee alone.
    assert!(matches!(
        check(&value),
        Err(Error::Departure {
            reason: R::MissingRuleEvidence,
            ..
        })
    ));
    let manual: Value = toml::from_str(DECLARATION).expect("fixture");
    declaration(&mut value)["evidence"]
        .as_array_mut()
        .expect("evidence")
        .push(manual["deviations"][0]["declaration"]["evidence"][0].clone());
    // The same bounded local references resolve after composition, even though
    // the override filename is elsewhere. Synthetic replay does not raise level.
    assert!(check(&value).is_ok(), "{:?}", check(&value));
    declaration(&mut value)["evidence"][0]["last_event"] = 2.into();
    assert!(matches!(
        check(&value),
        Err(Error::Departure {
            reason: R::ExchangeMismatch,
            ..
        })
    ));
    declaration(&mut value)["evidence"][0]["last_event"] = 1.into();
    change_scope(&mut value, "firmware", "other".into());
    assert!(matches!(
        check(&value),
        Err(Error::Departure {
            reason: R::RecordingScopeMismatch,
            ..
        })
    ));
    std::fs::remove_file(recording).expect("remove synthetic recording");
    std::fs::remove_dir(directory).expect("remove empty temporary directory");
}

#[test]
fn adr0015_validate_once_terminator_conflicts_and_missing_manual_fields() {
    let mut value = hl7();
    value["deviations"][0]["rule"] = "hl7_cr_lf_terminators".into();
    let mut other = value["deviations"][0].clone();
    other["rule"] = "hl7_lf_terminators".into();
    value["deviations"]
        .as_array_mut()
        .expect("array")
        .push(other);
    assert_refused(&value, R::ConflictingVariants);
    for field in ["title", "revision", "section", "reference"] {
        let mut value = machine();
        declaration(&mut value)["evidence"][0]
            .as_table_mut()
            .expect("manual")
            .remove(field);
        assert!(matches!(validate(&value), Err(Error::Schema { .. })));
    }
}

#[test]
fn adr0015_existing_compatibility_inventory_validates_each_exact_selected_setting() {
    for (tag, section, key, setting) in [
        ("astm_frame_limit", "framing", "limit", Value::Integer(247)),
        (
            "astm_receive_timeout",
            "dialog",
            "receive_timeout_ms",
            Value::Integer(12000),
        ),
        (
            "astm_send_timeout",
            "dialog",
            "send_timeout_ms",
            Value::Integer(10000),
        ),
        (
            "astm_host_contention_priority",
            "dialog",
            "contention_priority",
            Value::String("host".into()),
        ),
        (
            "astm_lowercase_checksum_hex",
            "framing",
            "checksum_hex",
            Value::String("accept_lowercase".into()),
        ),
    ] {
        let mut value = machine();
        value["framing"]["limit"] = 64000.into();
        value["deviations"][0]["rule"] = tag.into();
        value[section][key] = setting;
        assert!(validate(&value).is_ok(), "{tag}: {:?}", validate(&value));
    }
    for (tag, key, setting) in [
        ("hl7_cr_lf_terminators", "terminator", "cr_lf"),
        ("hl7_lf_terminators", "terminator", "lf"),
        ("hl7_preserve_unsupported_escapes", "escapes", "preserve"),
    ] {
        let mut value = hl7();
        value["deviations"][0]["rule"] = tag.into();
        declaration(&mut value)["evidence"][0]["guarantee"] = "compatibility".into();
        value["syntax"][key] = setting.into();
        assert!(validate(&value).is_ok(), "{tag}: {:?}", validate(&value));
    }
}
