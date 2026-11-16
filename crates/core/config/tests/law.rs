//! Tests law tf2b 2 2 config selects mllp 251 manager and explicit peer options; law tf2b 2 2 config rejects impossible stack combinations.
use analyte_config::{Draft, Error, EvidenceLevel, LawQueryForm};

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
    assert!(matches!(
        config.effective().stack(),
        analyte_config::Stack::Law(_)
    ));
    assert_eq!(machine.evidence.level, EvidenceLevel::Documented);
    let policy = &law(&config)?.policy();
    assert_eq!(
        policy.query_forms(),
        [LawQueryForm::Container, LawQueryForm::AllPending]
    );
    assert_eq!(policy.query_deadline_ms(), 1500);
    assert_eq!(policy.response_ms(), 1500);
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
        let peer = law(&changed)?.policy().peer().capabilities();
        assert_eq!(
            [
                peer.content.patient,
                peer.content.container,
                peer.content.specimen,
                peer.instructions.dilution,
                peer.instructions.repeat.manager_repeat,
                peer.instructions.repeat.repeat_control,
                peer.instructions.priority
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
fn law_tf2b_3_27_3_28_work_requires_explicit_bindings_and_allows_send_ahead() -> Result<(), Error> {
    let layout = include_str!("../examples/law_orders.toml");
    let text = format!("{LAW}\n{layout}");
    load(&text.replace("send_ahead = false", "send_ahead = true"))?;
    let valid = load(&text)?;
    assert!(
        law(&valid)?
            .orders()
            .as_ref()
            .map(analyte_config::LawOrders::work)
            .is_some()
    );
    for bad in [
        text.replace("placer_namespace = \"LAB\"", ""),
        text.replace("specimen_role = \"P\"", "specimen_role = \"Q\""),
        text.replace(
            "container_namespace = \"LAB\"",
            "container_namespace = \"L^AB\"",
        ),
        text.replace(
            "[\"SYN\",\"Synthetic\",\"99LAB\"]",
            "[\"SYN|bad\",\"Synthetic\",\"99LAB\"]",
        ),
        text.replace(
            "specimen_types = [{app={namespace=\"types\",value=\"SERUM\"},machine=\"SER\"}]",
            "specimen_types = []",
        ),
    ] {
        assert!(load(&bad).is_err());
    }
    assert!(
        load(&format!(
            "{LAW}\n{}",
            layout
                .split("[orders]")
                .next()
                .ok_or(Error::IncompatibleStack {
                    reason: "LAW test requires LAW stack",
                    path: "fixture".into()
                })?
        ))
        .is_err()
    );
    Ok(())
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
    assert_eq!(law(&config)?.policy().query_deadline_ms(), 2500);
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

#[test]
fn law_tf2b_b6_declared_topologies_require_explicit_direction_and_outbound_endpoint()
-> Result<(), Error> {
    for role in ["tcp_server", "tcp_client"] {
        load(&LAW.replace("kind = \"tcp_server\"", &format!("kind = \"{role}\"")))?;
    }
    let unsupported = LAW.replace(
        "connection_mode = \"persistent\"",
        "connection_mode = \"independent_per_transaction\"",
    );
    assert!(
        matches!(load(&unsupported), Err(Error::Invalid { key, .. }) if key=="law.outbound_endpoint")
    );
    let independent =
        unsupported.replace("[law]", "[law]\noutbound_endpoint = \"127.0.0.1:15100\"");
    load(&independent)?;
    assert!(load(&independent.replace("kind = \"tcp_server\"", "kind = \"tcp_client\"")).is_err());
    assert!(load(&independent.replace("127.0.0.1:15100", "0.0.0.0:0")).is_err());
    assert!(load(&independent.replace("independent_per_transaction", "persistent")).is_err());
    assert!(load(&LAW.replace("connection_mode = \"persistent\"", "")).is_err());
    Ok(())
}

fn law(config: &analyte_config::Validated) -> Result<&analyte_config::LawStack, Error> {
    let analyte_config::Stack::Law(stack) = config.effective().stack() else {
        return Err(Error::ProtocolSettings {
            path: "fixture".into(),
            protocol: "law",
        });
    };
    Ok(stack)
}
