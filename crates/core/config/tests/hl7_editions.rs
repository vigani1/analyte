//! Tests exact configured HL7 editions and actionable unsupported-edition diagnostics.
use analyte_config::{Draft, Edition, Error, EvidenceLevel};
use analyte_fixtures::ConfigEdit as _;
use analyte_hl7::Version;

const MACHINE: &str = include_str!("../examples/hl7.toml");
const SITE: &str = include_str!("../examples/site.toml");

fn query_config(edition: &str) -> Result<analyte_config::Validated, Error> {
    let base = MACHINE.rewrite("edition = \"v251\"", &format!("edition = \"{edition}\""));
    Draft::load(
        (
            "synthetic.toml",
            &format!("{base}\n{}", include_str!("../examples/hl7_orders.toml")),
        ),
        ("site.toml", SITE),
        ("local.toml", "schema_version=1"),
    )?
    .validate()
}

// HL7 2.8.2 §5.10.2
#[test]
fn v282_withdrawn_original_query_is_refused() {
    let error = query_config("v282").unwrap_err();
    assert!(matches!(
        &error,
        Error::WithdrawnHl7Query {
            edition: "2.8.2",
            section: "5.10.2",
            ..
        }
    ));
    assert_eq!(
        error.to_string(),
        "synthetic.toml: orders.hl7: HL7 2.8.2 withdraws the original query messages (§5.10.2); legacy query is not available for this edition"
    );
}

// HL7 2.9.1 §5.9.2
#[test]
fn v291_withdrawn_original_query_is_refused() {
    let error = query_config("v291").unwrap_err();
    assert!(matches!(
        &error,
        Error::WithdrawnHl7Query {
            edition: "2.9.1",
            section: "5.9.2",
            ..
        }
    ));
    assert_eq!(
        error.to_string(),
        "synthetic.toml: orders.hl7: HL7 2.9.1 withdraws the original query messages (§5.9.2); legacy query is not available for this edition"
    );
}

// HL7 2.6 §5.10.2
#[test]
fn v26_keeps_original_queries() -> Result<(), Error> {
    assert_eq!(Version::V26.legacy_query_withdrawal(), None);
    query_config("v26")?;
    Ok(())
}

fn draft(edition: &str) -> Result<Draft, Error> {
    let machine = MACHINE.rewrite("edition = \"v251\"", &format!("edition = \"{edition}\""));
    Draft::load(
        ("synthetic.toml", &machine),
        ("site.toml", SITE),
        ("local.toml", "schema_version=1"),
    )
}

// ADR 0012 §3
#[test]
fn supported_editions_keep_exact_wire_identity() -> Result<(), Error> {
    for (config_id, edition, wire) in [
        ("v23", Edition::from(Version::V23), "2.3"),
        ("v231", Edition::from(Version::V231), "2.3.1"),
        ("v24", Edition::from(Version::V24), "2.4"),
        ("v25", Edition::from(Version::V25), "2.5"),
        ("v251", Edition::from(Version::V251), "2.5.1"),
        ("v26", Edition::from(Version::V26), "2.6"),
        ("v282", Edition::from(Version::V282), "2.8.2"),
        ("v291", Edition::from(Version::V291), "2.9.1"),
    ] {
        let config = draft(config_id)?.validate()?;
        let analyte_config::Stack::Hl7(analyte_config::RecordTransport::Stream(stack)) =
            config.effective().stack()
        else {
            panic!("synthetic fixture must remain an HL7 stream stack");
        };
        assert_eq!(stack.settings().syntax().edition(), edition);
        assert_eq!(Version::from(edition).as_str(), wire);
        assert!(
            config
                .canonical()
                .contains(&format!("edition = \"{config_id}\""))
        );
        assert_eq!(
            config.effective().machine().evidence.level,
            EvidenceLevel::Documented
        );
    }
    Ok(())
}

// ADR 0012 §3
#[test]
fn unlisted_edition_names_value_and_allowed_ones() {
    for edition in [
        "", "v22", "v230", "v2510", "v27", "v28", "v29", "v292", "v291 ", "2.9.1",
    ] {
        let Error::Schema { path, source } = draft(edition).unwrap_err() else {
            panic!("an unlisted edition must fail at the schema boundary");
        };
        assert_eq!(path, "synthetic.toml");
        let diagnostic = source.to_string();
        assert!(diagnostic.contains("unknown variant"), "{diagnostic}");
        assert!(diagnostic.contains(edition), "{diagnostic}");
        for allowed in ["v23", "v231", "v24", "v25", "v251", "v26", "v282", "v291"] {
            assert!(diagnostic.contains(allowed), "{diagnostic}");
        }
        assert!(source.span().is_some());
    }
}

// ADR 0012 §3
#[test]
fn missing_edition_is_not_defaulted() {
    let machine = MACHINE
        .lines()
        .filter(|line| !line.starts_with("edition ="))
        .collect::<Vec<_>>()
        .join("\n");
    let draft = Draft::load(
        ("synthetic.toml", &machine),
        ("site.toml", SITE),
        ("local.toml", "schema_version=1"),
    )
    .unwrap();
    {
        let Err(refusal) = draft.validate() else {
            panic!("input must be refused")
        };
        assert!(
            matches!(&refusal, Error::Invalid { .. }),
            "unexpected refusal: {refusal:?}"
        );
    }
}

// HL7 2.9.1 §4.5.1.4
#[test]
fn unresolved_group_encoding_refuses_sending() -> Result<(), Error> {
    {
        let edition = "v291";
        let base = MACHINE.rewrite("edition = \"v251\"", &format!("edition = \"{edition}\""));
        let machine = format!("{base}\n{}", include_str!("../examples/hl7_send.toml"));
        let result = Draft::load(
            ("synthetic.toml", &machine),
            ("site.toml", SITE),
            ("local.toml", "schema_version=1"),
        )?
        .validate();
        assert!(
            matches!(result, Err(Error::Invalid { key, reason, .. }) if key == "orders.hl7_send" && reason.contains("4.5.1.4"))
        );
    }
    Ok(())
}

// HL7 2.5.1 §2.8
#[test]
fn legacy_queries_validate_but_unbuilt_writers_fail() {
    for edition in ["v23", "v24", "v25"] {
        let base = MACHINE.rewrite("edition = \"v251\"", &format!("edition = \"{edition}\""));
        for (section, sending) in [
            (include_str!("../examples/hl7_orders.toml"), false),
            (include_str!("../examples/hl7_send.toml"), true),
        ] {
            let machine = format!("{base}\n{section}");
            let result = Draft::load(
                ("synthetic.toml", &machine),
                ("site.toml", SITE),
                ("local.toml", "schema_version=1"),
            )
            .unwrap()
            .validate();
            if sending {
                assert!(matches!(result, Err(Error::Invalid { key, reason, .. })
                    if key == "orders.hl7_send" && reason.contains("not yet implemented")));
            } else {
                result.unwrap();
            }
        }
    }
}

// ADR 0012 §3
#[test]
fn config_reports_registry_writer_blocker() {
    for (edition, version) in [
        ("v23", Version::V23),
        ("v24", Version::V24),
        ("v25", Version::V25),
        ("v291", Version::V291),
    ] {
        let base = MACHINE.rewrite("edition = \"v251\"", &format!("edition = \"{edition}\""));
        let machine = format!("{base}\n{}", include_str!("../examples/hl7_send.toml"));
        let result = Draft::load(
            ("synthetic.toml", &machine),
            ("site.toml", SITE),
            ("local.toml", "schema_version=1"),
        )
        .unwrap()
        .validate();
        let Err(Error::Invalid { key, reason, .. }) = result else {
            panic!("unqualified writer must fail configuration");
        };
        assert_eq!(key, "orders.hl7_send");
        assert_eq!(Some(reason), version.order_sending_blocker());
    }
}

// HL7 2.6; HL7 2.8.2 §4.4.6, §2.A.13.7
#[test]
fn send_requires_explicit_table_revisions() {
    for edition in ["v26", "v282"] {
        let base = MACHINE.rewrite("edition = \"v251\"", &format!("edition = \"{edition}\""));
        let send = include_str!("../examples/hl7_send.toml");
        let send = format!("{send}\npriority_version = \"prio-r1\"\ntest_version = \"test-r1\"\n");
        for (settings, accepted) in [
            (send.clone(), true),
            (send.rewrite("priority_version = \"prio-r1\"", ""), false),
            (send.rewrite("specimen_version = \"1\"", ""), false),
            // ADR 0029: the machine's code system revision is one setting.
            (send.rewrite("test_version = \"test-r1\"", ""), false),
        ] {
            let machine = format!("{base}\n{settings}");
            let result = Draft::load(
                ("synthetic.toml", &machine),
                ("site.toml", SITE),
                ("local.toml", "schema_version=1"),
            )
            .unwrap()
            .validate();
            assert_eq!(result.is_ok(), accepted, "{result:?}");
        }
    }
}

// HL7 2.5.1 §2.A.7: CE has no coding-system version component, so an edition
// that writes OBR-4 as CE refuses a test_version instead of dropping it.
#[test]
fn ce_editions_refuse_a_test_version() {
    let send = include_str!("../examples/hl7_send.toml");
    for (settings, accepted) in [
        (send.to_owned(), true),
        (format!("{send}\ntest_version = \"test-r1\"\n"), false),
    ] {
        let machine = format!("{MACHINE}\n{settings}");
        let result = Draft::load(
            ("synthetic.toml", &machine),
            ("site.toml", SITE),
            ("local.toml", "schema_version=1"),
        )
        .unwrap()
        .validate();
        assert_eq!(result.is_ok(), accepted, "{result:?}");
    }
}

// HL7 2.9.1 section 2.8.3.2 requires a separate application exchange channel.
// HL7 2.9.1 §2.8.3.2
#[test]
fn separate_application_channel_cannot_be_enabled() -> Result<(), Error> {
    for enabled in [false, true] {
        let definition=MACHINE.rewrite("edition = \"v251\"", "edition = \"v291\"").rewrite("acknowledgment = \"store_and_forward\"", &format!("acknowledgment = \"store_and_forward\"\nenhanced_application_acknowledgments = {enabled}"));
        let result = Draft::load(
            ("synthetic", &definition),
            ("site", SITE),
            ("local", "schema_version=1"),
        )?
        .validate();
        if enabled {
            assert!(
                matches!(&result, Err(Error::Invalid { key, .. }) if key == "dialog.enhanced_application_acknowledgments")
            );
        } else {
            result
                .as_ref()
                .expect("disabled separate channel is supported");
        }
        if let Err(error) = result {
            assert!(error.to_string().contains("unsupported separate channel"));
        }
    }
    Ok(())
}
