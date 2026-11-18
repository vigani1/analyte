//! Tests LAW message validation and host exchanges.
use analyte_config::{Draft, Error, EvidenceLevel, LawQueryForm};
use analyte_fixtures::ConfigEdit as _;

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

// IHE PaLM TF-2b §2.2
#[test]
fn selects_mllp_manager_and_explicit_peer_options() -> Result<(), Error> {
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
    assert!(matches!(
        config.effective().stack(),
        analyte_config::Stack::Law(_) | analyte_config::Stack::Hl7(_)
    ));
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
            matches!(
                load(&LAW.rewrite(&format!("{flag} = false"), "")),
                Err(Error::Schema { .. })
            ),
            "{flag}"
        );
        let changed = load(&LAW.rewrite(&format!("{flag} = false"), &format!("{flag} = true")))?;
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

// IHE PaLM TF-2b §2.2
#[test]
fn rejects_impossible_stack_combinations() {
    for bad in [
        LAW.rewrite("[dialog]", "[dialog]\nkind = 'hl7_receiver'"),
        LAW.split("[law]").next().expect("prefix").to_owned(),
        LAW.rewrite("v251", "v231"),
        LAW.rewrite("encoding = \"utf8\"", "encoding = \"ascii\""),
        LAW.rewrite("terminator = \"cr\"", "terminator = \"lf\""),
        LAW.rewrite("escapes = \"reject\"", "escapes = \"preserve\""),
        LAW.rewrite("[framing]", "[framing]\nkind = 'none'"),
        LAW.rewrite("kind = \"tcp_server\"", "kind = \"watched_folder\""),
        LAW.rewrite("acknowledgment = \"store_and_forward\"", ""),
        LAW.rewrite("[syntax]", "[syntax]\nkind = 'astm_lis02'"),
    ] {
        assert!(
            matches!(
                load(&bad),
                Err(Error::IncompatibleStack { .. }
                    | Error::ProtocolSettings { .. }
                    | Error::Schema { .. }
                    | Error::Invalid { .. })
            ),
            "invalid LAW stack accepted"
        );
    }
}

// IHE PaLM TF-2b §3.27
#[test]
fn bounds_deadlines_and_declares_supported_query_forms() -> Result<(), Error> {
    for forms in [
        "[]",
        "[\"container\", \"container\"]",
        "[\"rack\"]",
        "[\"tray\"]",
        "[\"isolate\"]",
        "[\"container\", \"all_pending\", \"container\"]",
    ] {
        {
            let Err(refusal) = load(&LAW.rewrite("[\"container\", \"all_pending\"]", forms)) else {
                panic!("input must be refused")
            };
            assert!(
                matches!(&refusal, Error::Schema { .. } | Error::Invalid { .. }),
                "unexpected refusal: {refusal:?}"
            );
        }
    }
    for forms in ["[\"container\"]", "[\"all_pending\"]"] {
        load(&LAW.rewrite("[\"container\", \"all_pending\"]", forms))?;
    }
    for key in ["query_deadline_ms", "response_ms"] {
        for value in [0, 60_001] {
            {
                let refusal =
                    load(&LAW.rewrite(&format!("{key} = 1500"), &format!("{key} = {value}")))
                        .map(|_| ())
                        .expect_err("input must be refused");
                assert!(
                    matches!(&refusal, Error::Schema { .. } | Error::Invalid { .. }),
                    "unexpected refusal: {refusal:?}"
                );
            }
        }
        for value in [1, 60_000] {
            load(&LAW.rewrite(&format!("{key} = 1500"), &format!("{key} = {value}")))?;
        }
    }
    Ok(())
}

// IHE PaLM TF-2b §3.29.4
#[test]
fn allows_corrections_only_by_explicit_c() {
    for codes in ["[]", "[\"F\"]", "[\"C\", \"F\"]"] {
        let bad = LAW.rewrite(
            "correction_status_codes = [\"C\"]",
            &format!("correction_status_codes = {codes}"),
        );
        assert!(
            matches!(load(&bad), Err(Error::Invalid { key, .. }) if key == "mappings.identity.correction_status_codes")
        );
    }
}

// IHE PaLM TF-2b §3.27, §3.28
#[test]
fn work_requires_bindings_and_allows_send_ahead() -> Result<(), Error> {
    let layout = include_str!("../examples/law_orders.toml");
    let text = format!("{LAW}\n{layout}");
    load(&text.rewrite("send_ahead = false", "send_ahead = true"))?;
    let valid = load(&text)?;
    assert!(
        law(&valid)?
            .orders()
            .map(analyte_config::LawOrders::work)
            .is_some()
    );
    for bad in [
        text.rewrite("placer_namespace = \"LAB\"", ""),
        text.rewrite("specimen_role = \"P\"", "specimen_role = \"Q\""),
        text.rewrite(
            "container_namespace = \"LAB\"",
            "container_namespace = \"L^AB\"",
        ),
        text.rewrite(
            "[\"SYN\",\"Synthetic\",\"99LAB\"]",
            "[\"SYN|bad\",\"Synthetic\",\"99LAB\"]",
        ),
        text.rewrite(
            "specimen_types = [{app={namespace=\"types\",value=\"SERUM\"},machine=\"SER\"}]",
            "specimen_types = []",
        ),
    ] {
        {
            let Err(refusal) = load(&bad) else {
                panic!("input must be refused")
            };
            assert!(
                matches!(&refusal, Error::Invalid { .. }),
                "unexpected refusal: {refusal:?}"
            );
        }
    }
    {
        let Err(refusal) = load(&format!(
            "{LAW}\n{}",
            layout
                .split("[orders]")
                .next()
                .ok_or(Error::IncompatibleStack {
                    reason: "LAW test requires LAW stack",
                    path: "fixture".into()
                })?
        )) else {
            panic!("input must be refused")
        };
        assert!(
            matches!(&refusal, Error::Invalid { .. }),
            "unexpected refusal: {refusal:?}"
        );
    }
    Ok(())
}

// IHE PaLM TF-2b §3.29.4, §B.3
#[test]
fn cannot_replace_awos_or_truncate_og_identity() {
    for bad in [
        LAW.rewrite(
            "record=\"OBR\",field=2,component=1",
            "record=\"OBR\",field=3,component=1",
        ),
        LAW.rewrite(
            "order_namespace = {fixed=\"LAB\"}",
            "order_namespace = {field={group=\"order\",record=\"OBR\",field=2,component=2,repetition=1}}",
        ),
        LAW.rewrite(
            "record=\"OBX\",field=4,component=1",
            "record=\"OBX\",field=4,component=2",
        ),
        LAW.rewrite(
            "sub_id = {group=\"observation\",record=\"OBX\",field=4,component=1,repetition=1}",
            "sub_id = \"none\"",
        ),
        LAW.rewrite(
            "run = []",
            "run = [{group=\"observation\",record=\"OBX\",field=4,component=2,repetition=1}]",
        ),
    ] {
        assert!(
            matches!(load(&bad), Err(Error::Invalid { key, .. }) if key == "mappings.identity")
        );
    }
}

// IHE PaLM TF-2b §3.27, §3.28
#[test]
fn rejects_non_law_order_layouts() {
    for orders in [
        include_str!("../examples/hl7_send.toml"),
        include_str!("../examples/hl7_orders.toml"),
    ] {
        let bad = format!("{LAW}\n{orders}");
        assert!(matches!(load(&bad), Err(Error::Invalid { key, .. }) if key == "orders"));
    }
}

// ADR 0002
#[test]
fn overrides_replace_policy_and_change_provenance() -> Result<(), Error> {
    let replacement = LAW.split_once("[law]").expect("LAW policy").1;
    let replacement = format!("schema_version = 1\n[law]{replacement}")
        .rewrite("query_deadline_ms = 1500", "query_deadline_ms = 2500");
    let config = Draft::load(
        ("law.toml", LAW),
        ("site.toml", SITE),
        ("overrides", &replacement),
    )?
    .validate()?;
    assert_eq!(law(&config)?.policy().query_deadline_ms(), 2500);
    assert_ne!(config.digest(), load(LAW)?.digest());
    let invalid = replacement.rewrite("query_deadline_ms = 2500", "query_deadline_ms = 0");
    {
        let Err(refusal) = Draft::load(
            ("law.toml", LAW),
            ("site.toml", SITE),
            ("overrides", &invalid),
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

// IHE PaLM TF-2b §B.6
#[test]
fn topologies_require_direction_and_outbound_endpoint() -> Result<(), Error> {
    for role in ["tcp_server", "tcp_client"] {
        load(&LAW.rewrite("kind = \"tcp_server\"", &format!("kind = \"{role}\"")))?;
    }
    let unsupported = LAW.rewrite(
        "connection_mode = \"persistent\"",
        "connection_mode = \"independent_per_transaction\"",
    );
    assert!(
        matches!(load(&unsupported), Err(Error::Invalid { key, .. }) if key=="law.outbound_endpoint")
    );
    let independent =
        unsupported.rewrite("[law]", "[law]\noutbound_endpoint = \"127.0.0.1:15100\"");
    load(&independent)?;
    {
        let Err(refusal) =
            load(&independent.rewrite("kind = \"tcp_server\"", "kind = \"tcp_client\""))
        else {
            panic!("input must be refused")
        };
        assert!(
            matches!(&refusal, Error::Invalid { .. }),
            "unexpected refusal: {refusal:?}"
        );
    }
    {
        let Err(refusal) = load(&independent.rewrite("127.0.0.1:15100", "0.0.0.0:0")) else {
            panic!("input must be refused")
        };
        assert!(
            matches!(&refusal, Error::Invalid { .. }),
            "unexpected refusal: {refusal:?}"
        );
    }
    {
        let Err(refusal) = load(&independent.rewrite("independent_per_transaction", "persistent"))
        else {
            panic!("input must be refused")
        };
        assert!(
            matches!(&refusal, Error::Invalid { .. }),
            "unexpected refusal: {refusal:?}"
        );
    }
    {
        let Err(refusal) = load(&LAW.rewrite("connection_mode = \"persistent\"", "")) else {
            panic!("input must be refused")
        };
        assert!(
            matches!(&refusal, Error::Schema { .. }),
            "unexpected refusal: {refusal:?}"
        );
    }
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
