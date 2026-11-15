//! Synthetic configuration evidence; IHE `PaLM` TF-2b Rev 11 sections 2.2, 3.27, 3.29.
use analyte_config::{DialogKind, Draft, Error, EvidenceLevel, LawQueryForm};

const LAW: &str = include_str!("../examples/law.toml");
const SITE: &str = include_str!("../examples/site.toml");

fn load(text: &str) -> Result<analyte_config::Validated, Error> {
    Draft::load(
        ("law.toml", text),
        ("site.toml", SITE),
        ("overrides", "schema_version = 1"),
    )?
    .validate()
}

#[test]
fn law_tf2b_2_2_config_selects_mllp_251_manager_and_explicit_peer_options() -> Result<(), Error> {
    let config = load(LAW)?;
    let machine = config.effective().machine();
    assert_eq!(machine.dialog.kind, DialogKind::LawManager);
    assert_eq!(machine.evidence.level, EvidenceLevel::Documented);
    let law = machine.law.as_ref().expect("LAW declaration");
    assert_eq!(
        law.query_forms,
        [LawQueryForm::Container, LawQueryForm::AllPending]
    );
    assert_eq!(law.query_deadline_ms, 1500);
    assert_eq!(law.response_ms, 1500);
    assert!(config.effective().hl7_options().is_some());
    // Each peer flag is independently declared; omission is never a default.
    for flag in [
        "patient",
        "container",
        "specimen",
        "dilution",
        "manager_repeat",
        "repeat_control",
        "priority",
    ] {
        assert!(
            load(&LAW.replace(&format!("{flag} = false"), "")).is_err(),
            "{flag}"
        );
        let changed = load(&LAW.replace(&format!("{flag} = false"), &format!("{flag} = true")))?;
        let peer = changed
            .effective()
            .machine()
            .law
            .as_ref()
            .expect("LAW")
            .peer
            .capabilities();
        assert_eq!(
            [
                peer.patient,
                peer.container,
                peer.specimen,
                peer.dilution,
                peer.manager_repeat,
                peer.repeat_control,
                peer.priority
            ]
            .into_iter()
            .filter(|flag| *flag)
            .count(),
            1
        );
    }
    Ok(())
}

#[test]
fn law_tf2b_2_2_config_rejects_impossible_stack_combinations() {
    for bad in [
        LAW.replace("law_manager", "hl7_receiver"),
        LAW.split("[law]").next().expect("prefix").to_owned(),
        LAW.replace("v251", "v231"),
        LAW.replace("encoding = \"utf8\"", "encoding = \"ascii\""),
        LAW.replace("terminator = \"cr\"", "terminator = \"lf\""),
        LAW.replace("escapes = \"reject\"", "escapes = \"preserve\""),
        LAW.replace("kind = \"mllp\"", "kind = \"none\""),
        LAW.replace("kind = \"tcp_server\"", "kind = \"watched_folder\""),
        LAW.replace("acknowledgment = \"store_and_forward\"", ""),
        LAW.replace("kind = \"hl7_v2\"", "kind = \"astm_lis02\""),
    ] {
        assert!(load(&bad).is_err(), "invalid LAW stack accepted");
    }
}

#[test]
fn law_tf2b_3_27_config_bounds_deadlines_and_declares_only_supported_query_forms()
-> Result<(), Error> {
    for forms in [
        "[]",
        "[\"container\", \"container\"]",
        "[\"rack\"]",
        "[\"tray\"]",
        "[\"isolate\"]",
        "[\"container\", \"all_pending\", \"container\"]",
    ] {
        assert!(load(&LAW.replace("[\"container\", \"all_pending\"]", forms)).is_err());
    }
    for forms in ["[\"container\"]", "[\"all_pending\"]"] {
        load(&LAW.replace("[\"container\", \"all_pending\"]", forms))?;
    }
    for key in ["query_deadline_ms", "response_ms"] {
        for value in [0, 60_001] {
            assert!(
                load(&LAW.replace(&format!("{key} = 1500"), &format!("{key} = {value}"))).is_err()
            );
        }
        for value in [1, 60_000] {
            load(&LAW.replace(&format!("{key} = 1500"), &format!("{key} = {value}")))?;
        }
    }
    Ok(())
}

#[test]
fn law_tf2b_3_29_4_config_allows_corrections_only_by_explicit_c() {
    for codes in ["[]", "[\"F\"]", "[\"C\", \"F\"]"] {
        let bad = LAW.replace(
            "correction_status_codes = [\"C\"]",
            &format!("correction_status_codes = {codes}"),
        );
        assert!(
            matches!(load(&bad), Err(Error::Invalid { key, .. }) if key == "mappings.identity.correction_status_codes")
        );
    }
}

#[test]
fn law_tf2b_3_29_4_b3_config_cannot_replace_awos_or_truncate_og_identity() {
    for bad in [
        LAW.replace(
            "record=\"OBR\",field=2,component=1",
            "record=\"OBR\",field=3,component=1",
        ),
        LAW.replace(
            "record=\"OBR\",field=2,component=2",
            "record=\"OBR\",field=3,component=2",
        ),
        LAW.replace(
            "record=\"OBX\",field=4,component=1",
            "record=\"OBX\",field=4,component=2",
        ),
        LAW.replace(
            "sub_id = {group=\"observation\",record=\"OBX\",field=4,component=1,repetition=1}",
            "sub_id = \"none\"",
        ),
        LAW.replace(
            "run = []",
            "run = [{group=\"observation\",record=\"OBX\",field=4,component=2,repetition=1}]",
        ),
    ] {
        assert!(
            matches!(load(&bad), Err(Error::Invalid { key, .. }) if key == "mappings.identity")
        );
    }
}

#[test]
fn law_tf2b_3_27_3_28_config_rejects_non_law_order_layouts() {
    for orders in [
        include_str!("../examples/hl7_send.toml"),
        include_str!("../examples/hl7_orders.toml"),
    ] {
        let bad = format!("{LAW}\n{orders}");
        assert!(matches!(load(&bad), Err(Error::Invalid { key, .. }) if key == "orders"));
    }
}

#[test]
fn adr0002_law_overrides_replace_policy_and_change_protected_provenance() -> Result<(), Error> {
    let replacement = LAW.split_once("[law]").expect("LAW policy").1;
    let replacement = format!("schema_version = 1\n[law]{replacement}")
        .replace("query_deadline_ms = 1500", "query_deadline_ms = 2500");
    let config = Draft::load(
        ("law.toml", LAW),
        ("site.toml", SITE),
        ("overrides", &replacement),
    )?
    .validate()?;
    assert_eq!(
        config
            .effective()
            .machine()
            .law
            .as_ref()
            .expect("LAW")
            .query_deadline_ms,
        2500
    );
    assert_ne!(config.digest(), load(LAW)?.digest());
    let invalid = replacement.replace("query_deadline_ms = 2500", "query_deadline_ms = 0");
    assert!(
        Draft::load(
            ("law.toml", LAW),
            ("site.toml", SITE),
            ("overrides", &invalid)
        )?
        .validate()
        .is_err()
    );
    Ok(())
}
