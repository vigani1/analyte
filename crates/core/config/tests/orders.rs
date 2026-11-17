//! Tests configured outgoing orders and code mappings.
use analyte_config::{Draft, Validated};
use analyte_fixtures::ConfigEdit as _;
type TestResult = Result<(), Box<dyn std::error::Error>>;
fn config(section: &str) -> Result<Validated, analyte_config::Error> {
    let definition = format!("{}\n{section}", include_str!("../examples/astm.toml"));
    Draft::load(
        ("machine", &definition),
        ("site", include_str!("../examples/site.toml")),
        ("overrides", "schema_version=1"),
    )?
    .validate()
}
#[test]
fn adr0002_outbound_duplicate_missing_policy_and_unbounded_deadline_are_rejected() -> TestResult {
    let section = include_str!("../examples/orders.toml");
    for invalid in [section.rewrite("deadline_ms = 1000", "deadline_ms = 0"), section.rewrite("when_untrusted = \"error\"", ""), section.rewrite("send_ahead = true", ""), section.rewrite("machine_components=[\"\", \"\", \"\", \"SEND\"]}", "machine_components=[\"SEND\"]},{app={namespace=\"demo\",value=\"DEMO\"},machine_components=[\"OTHER\"]}")] {
        { let Err(refusal) = config(&invalid) else { panic!("input must be refused") }; assert!(matches!(&refusal, analyte_config::Error::Schema { .. } | analyte_config::Error::Invalid { .. }), "unexpected refusal: {refusal:?}"); }
    }
    let valid = config(section)?;
    assert!(valid.canonical().contains("[machine.orders]"));
    Ok(())
}

fn vendor_config(definition: &str, section: &str) -> Result<Validated, analyte_config::Error> {
    Draft::load(
        ("machine", &format!("{definition}\n{section}")),
        ("site", include_str!("../examples/site.toml")),
        ("overrides", "schema_version=1"),
    )?
    .validate()
}

const VENDOR_ORDERS: &str = r#"
[orders]
send_ahead = true
patient_namespace = "synthetic-patients"
tests = [{app={namespace="demo",value="DEMO"},machine_components=["SYN"]}]
priorities = [{app="routine",machine="R"}]
"#;

fn invalid_key(result: Result<Validated, analyte_config::Error>, expected: &str) {
    assert!(matches!(result, Err(analyte_config::Error::Invalid { key, .. }) if key == expected));
}

#[test]
fn adr0002_vendor_patient_namespace_is_required_and_retained_exactly() -> TestResult {
    for definition in [
        include_str!("../examples/vidas_literal.toml"),
        include_str!("../examples/autolyser.toml"),
    ] {
        let definition = definition.rewrite("download = false", "download = true");
        let section = if definition.contains("[autolyser]") {
            format!("{}clone = false\n", autolyser_orders())
        } else {
            VENDOR_ORDERS.to_owned()
        };
        let valid = vendor_config(&definition, &section)?;
        assert_eq!(vendor_namespace(&valid)?.as_str(), "synthetic-patients");
        assert!(matches!(
            valid.effective().stack(),
            analyte_config::Stack::Vidas(_) | analyte_config::Stack::Autolyser(_)
        ));
        invalid_key(
            vendor_config(
                &definition,
                &section.rewrite("patient_namespace = \"synthetic-patients\"", ""),
            ),
            "orders.patient_namespace",
        );
        for namespace in ["", "bad\nnamespace"] {
            let invalid = section.rewrite("synthetic-patients", namespace);
            {
                let Err(refusal) = vendor_config(&definition, &invalid) else {
                    panic!("input must be refused")
                };
                assert!(
                    matches!(
                        &refusal,
                        analyte_config::Error::Schema { .. }
                            | analyte_config::Error::Invalid { .. }
                    ),
                    "unexpected refusal: {refusal:?}"
                );
            }
        }
        let exact = section.rewrite("synthetic-patients", "000LabA");
        let valid = vendor_config(&definition, &exact)?;
        assert_eq!(vendor_namespace(&valid)?.as_str(), "000LabA");
    }
    Ok(())
}

#[test]
fn autolyser_rev18_ch23_clone_requires_explicit_true_or_false() -> TestResult {
    let definition =
        include_str!("../examples/autolyser.toml").rewrite("download = false", "download = true");
    invalid_key(
        vendor_config(&definition, &autolyser_orders()),
        "orders.clone",
    );
    for value in [true, false] {
        let section = format!("{}clone = {value}\n", autolyser_orders());
        let valid = vendor_config(&definition, &section)?;
        assert_eq!(
            autolyser(&valid)?.orders().ok_or("orders")?.clone_enabled(),
            value
        );
        assert!(valid.canonical().contains(&format!("clone = {value}")));
    }
    {
        let Err(refusal) =
            vendor_config(&definition, &format!("{VENDOR_ORDERS}clone = \"false\"\n"))
        else {
            panic!("input must be refused")
        };
        assert!(
            matches!(&refusal, analyte_config::Error::Schema { .. }),
            "unexpected refusal: {refusal:?}"
        );
    }
    Ok(())
}

#[test]
fn adr0002_outbound_settings_cannot_silently_apply_to_another_stack() -> TestResult {
    let vidas = include_str!("../examples/vidas_literal.toml")
        .rewrite("download = false", "download = true");
    invalid_key(
        vendor_config(&vidas, &format!("{}clone = false\n", autolyser_orders())),
        "orders.clone",
    );
    invalid_key(
        vendor_config(
            &vidas,
            &format!(
                "{VENDOR_ORDERS}\n[orders.query]\ndeadline_ms=1000\nwhen_untrusted=\"error\"\n"
            ),
        ),
        "orders.query",
    );
    let astm_section = include_str!("../examples/orders.toml");
    let astm_table = astm_section
        .split_once("[orders.astm]")
        .ok_or("ASTM table")?
        .1;
    invalid_key(
        vendor_config(
            &vidas,
            &format!("{VENDOR_ORDERS}\n[orders.astm]{astm_table}"),
        ),
        "orders.astm",
    );
    invalid_key(
        config(&astm_section.rewrite(
            "send_ahead = true",
            "send_ahead = true\npatient_namespace=\"patients\"",
        )),
        "orders.patient_namespace",
    );
    invalid_key(
        config(&astm_section.rewrite("send_ahead = true", "send_ahead = true\nclone=false")),
        "orders.clone",
    );
    invalid_key(
        config(
            astm_section
                .split_once("[orders.astm]")
                .ok_or("ASTM table")?
                .0,
        ),
        "orders.astm",
    );
    for definition in [
        include_str!("../examples/vidas_literal.toml"),
        include_str!("../examples/autolyser.toml"),
    ] {
        let section = if definition.contains("[autolyser]") {
            format!("{}clone=false\n", autolyser_orders())
        } else {
            VENDOR_ORDERS.to_owned()
        };
        invalid_key(vendor_config(definition, &section), "orders.send_ahead");
        // Receive-only machines need neither outbound setting.
        vendor_config(definition, "")?;
    }
    Ok(())
}

#[test]
fn vidas_bci_a_section4_outbound_assay_codes_require_single_bounded_tag_value() -> TestResult {
    let definition = include_str!("../examples/vidas_literal.toml")
        .rewrite("download = false", "download = true");
    for components in [r#"["ABCDE"]"#, r#"["A", "B"]"#, r#"["A|B"]"#] {
        invalid_key(
            vendor_config(
                &definition,
                &VENDOR_ORDERS.rewrite(r#"["SYN"]"#, components),
            ),
            "orders.tests",
        );
    }
    Ok(())
}

fn autolyser_orders() -> String {
    VENDOR_ORDERS
        .rewrite(r#"["SYN"]"#, r#"["SYN "]"#)
        .rewrite(r#"machine="R""#, r#"machine="T""#)
}

#[test]
fn autolyser_rev18_ch23_outbound_medium_list_and_analysis_tables_are_explicit() -> TestResult {
    let definition =
        include_str!("../examples/autolyser.toml").rewrite("download = false", "download = true");
    let section = format!("{}clone=false\n", autolyser_orders());
    let types = r#"[{app={namespace="synthetic",value="SERUM"},machine="S"}]"#;
    vendor_config(&definition, &format!("{section}specimen_types={types}\n"))?;
    for table in [
        "[]".to_owned(),
        types.rewrite(r#"machine="S""#, r#"machine="X""#),
        types.rewrite(
            "]",
            &format!(",{}]", types.trim_start_matches('[').trim_end_matches(']')),
        ),
    ] {
        invalid_key(
            vendor_config(&definition, &format!("{section}specimen_types={table}\n")),
            "orders.specimen_types",
        );
    }
    invalid_key(
        vendor_config(&definition, &section.rewrite(r#"["SYN "]"#, r#"["SYN"]"#)),
        "orders.tests",
    );
    invalid_key(
        vendor_config(
            &definition,
            &section.rewrite(r#"machine="T""#, r#"machine="routine""#),
        ),
        "orders.priorities",
    );
    Ok(())
}

#[test]
fn lis02_section8_4_16_1_specimen_destination_and_code_table_preserve_field_ownership() -> TestResult
{
    let section=include_str!("../examples/orders.toml").rewrite("[orders.query]", "specimen_types=[{app={namespace='specimens',value='SERUM'},machine='SER'}]\n[orders.query]");
    let destination = "specimen_type={group='order',record='O',field=16,component=1,repetition=1}";
    config(&format!("{section}\n{destination}"))?;
    for invalid in [
        destination.rewrite("field=16", "field=20"),
        destination.rewrite("component=1", "component=2"),
        destination.rewrite("record='O'", "record='R'"),
        destination.rewrite("group='order'", "group='observation'"),
        destination.rewrite("repetition=1", "repetition=2"),
    ] {
        invalid_key(
            config(&format!("{section}\n{invalid}")),
            "orders.astm.specimen_type",
        );
    }
    invalid_key(
        config(&section.rewrite("machine='SER'", "machine='S^R'")),
        "orders.specimen_types",
    );
    let duplicated = section.rewrite(
        "machine='SER'}]",
        "machine='SER'},{app={namespace='specimens',value='SERUM'},machine='OTHER'}]",
    );
    invalid_key(config(&duplicated), "orders.specimen_types");
    Ok(())
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "One retained-evidence scenario checks refusals and rollback alongside its successful lifecycle"
)]
fn adr0002_hl7_legacy_query_requires_explicit_bounded_layout_and_selectors() -> TestResult {
    let definition = include_str!("../examples/hl7.toml");
    let section = include_str!("../examples/hl7_orders.toml");
    let valid = vendor_config(definition, section)?;
    assert!(hl7(&valid)?.orders().and_then(|o| o.query()).is_some());
    for (invalid, key) in [
        (
            section.rewrite("acknowledgment_ms = 500", "acknowledgment_ms = 0"),
            "orders.hl7.acknowledgment_ms",
        ),
        (
            section.rewrite("sample_field = 8", "sample_field = 9"),
            "orders.hl7.sample_field",
        ),
        (
            section.rewrite("{field=9,value=\"ORD\"},", ""),
            "orders.hl7.filters",
        ),
        (
            section.rewrite("{source=\"sample_id\"},", ""),
            "orders.hl7.lines",
        ),
        (
            section.rewrite("{source=\"test_code\"},", ""),
            "orders.hl7.lines",
        ),
        (
            section.rewrite("send_ahead = false", "send_ahead = true"),
            "orders",
        ),
        (
            section.rewrite(
                "value=\"TEST\"}]",
                "value=\"TEST\"},{field=9,value=\"OTHER\"}]",
            ),
            "orders.hl7.filters",
        ),
    ] {
        invalid_key(vendor_config(definition, &invalid), key);
    }
    {
        let Err(refusal) = vendor_config(
            definition,
            &section.rewrite("deadline_ms = 1000", "deadline_ms = 60001"),
        ) else {
            panic!("input must be refused")
        };
        assert!(
            matches!(&refusal, analyte_config::Error::Invalid { .. }),
            "unexpected refusal: {refusal:?}"
        );
    }
    {
        let Err(refusal) = vendor_config(
            definition,
            &section.rewrite("source=\"test_code\"", "source=\"guessed\""),
        ) else {
            panic!("input must be refused")
        };
        assert!(
            matches!(&refusal, analyte_config::Error::Schema { .. }),
            "unexpected refusal: {refusal:?}"
        );
    }
    {
        let Err(refusal) = vendor_config(
            definition,
            &section.rewrite("value=\"\"", "value=\"injected|field\""),
        ) else {
            panic!("input must be refused")
        };
        assert!(
            matches!(&refusal, analyte_config::Error::Invalid { .. }),
            "unexpected refusal: {refusal:?}"
        );
    }
    invalid_key(
        vendor_config(
            definition,
            &section.rewrite(
                r#"when_untrusted = "error""#,
                r#"when_untrusted = "no_answer""#,
            ),
        ),
        "orders.query.when_untrusted",
    );
    {
        let Err(refusal) = vendor_config(
            definition,
            &section.rewrite(r#"sample_suffix = "^^^^^^^LCL""#, r#"sample_suffix = """#),
        ) else {
            panic!("input must be refused")
        };
        assert!(
            matches!(&refusal, analyte_config::Error::Invalid { .. }),
            "unexpected refusal: {refusal:?}"
        );
    }
    {
        let Err(refusal) = config(section) else {
            panic!("input must be refused")
        };
        assert!(
            matches!(&refusal, analyte_config::Error::Invalid { .. }),
            "unexpected refusal: {refusal:?}"
        );
    }
    Ok(())
}

#[test]
fn adr0002_hl7_send_requires_namespace_valid_headers_item_codes_and_deadline() -> TestResult {
    let definition = include_str!("../examples/hl7.toml");
    let section = include_str!("../examples/hl7_send.toml");
    vendor_config(definition, section)?;
    for (invalid, key) in [
        (
            section.rewrite(r#"placer_namespace = "LAB""#, ""),
            "orders.placer_namespace",
        ),
        (
            section.rewrite("response_ms = 1500", "response_ms = 0"),
            "orders.hl7_send",
        ),
        (
            section.rewrite("sample_field = 18", "sample_field = 17"),
            "orders.hl7_send.position_field",
        ),
        (
            section.rewrite(
                "sample_field = 18",
                "sample_field = 18\nposition_field = 18",
            ),
            "orders.hl7_send.position_field",
        ),
        (
            section.rewrite(r#"["SYN","","L"]"#, r#"["SYN"]"#),
            "orders.tests",
        ),
    ] {
        invalid_key(vendor_config(definition, &invalid), key);
    }
    let invalid_processing = section.rewrite(r#"processing = "T""#, r#"processing = "unknown""#);
    assert!(matches!(
        vendor_config(definition, &invalid_processing),
        Err(analyte_config::Error::Schema { .. })
    ));
    {
        let Err(refusal) = config(section) else {
            panic!("input must be refused")
        };
        assert!(
            matches!(&refusal, analyte_config::Error::Invalid { .. }),
            "unexpected refusal: {refusal:?}"
        );
    }
    Ok(())
}

#[test]
fn hl7_ch2_msh11_and_ch4_priority_codes_are_typed_and_preserve_wire_spelling() -> TestResult {
    use analyte_config::{Hl7Processing, MachinePriority, PriorityCode};
    let definition = include_str!("../examples/hl7.toml");
    let section = include_str!("../examples/hl7_send.toml");
    for (wire, expected) in [
        ("P", Hl7Processing::Production),
        ("T", Hl7Processing::Training),
        ("D", Hl7Processing::Debugging),
    ] {
        let section = section.rewrite(r#"processing = "T""#, &format!("processing = \"{wire}\""));
        let valid = vendor_config(definition, &section)?;
        assert_eq!(
            hl7(&valid)?
                .orders()
                .expect("orders")
                .send()
                .expect("HL7 send")
                .settings()
                .processing,
            expected
        );
        assert!(
            valid
                .canonical()
                .contains(&format!("processing = \"{wire}\""))
        );
    }
    for (wire, expected) in [
        ("R", PriorityCode::R),
        ("S", PriorityCode::S),
        ("A", PriorityCode::A),
        ("P", PriorityCode::P),
        ("C", PriorityCode::C),
        ("T", PriorityCode::T),
        ("PRN", PriorityCode::Prn),
    ] {
        let section = section.rewrite(r#"machine="R""#, &format!("machine=\"{wire}\""));
        let valid = vendor_config(definition, &section)?;
        assert!(
            matches!(hl7(&valid)?.orders().expect("orders").tables().priorities().first().expect("priority").machine, MachinePriority::Code(code) if code == expected)
        );
    }
    invalid_key(
        vendor_config(
            definition,
            &section.rewrite(r#"machine="R""#, r#"machine="custom""#),
        ),
        "orders.priorities",
    );
    // Configurable ASTM priority text must not be restricted to HL7 table 0485.
    let astm_text =
        include_str!("../examples/orders.toml").rewrite(r#"machine="R""#, r#"machine="custom""#);
    let valid = config(&astm_text)?;
    assert!(
        matches!(&astm(&valid)?.orders().expect("orders").tables().priorities().first().expect("priority").machine, MachinePriority::Custom(value) if value.as_str() == "custom")
    );
    Ok(())
}

fn hl7(config: &Validated) -> Result<&analyte_config::Hl7Stack, Box<dyn std::error::Error>> {
    let analyte_config::Stack::Hl7(analyte_config::RecordTransport::Stream(stack)) =
        config.effective().stack()
    else {
        return Err("HL7 fixture".into());
    };
    Ok(stack)
}
fn astm(config: &Validated) -> Result<&analyte_config::AstmStack, Box<dyn std::error::Error>> {
    let analyte_config::Stack::Astm(analyte_config::RecordTransport::Stream(stack)) =
        config.effective().stack()
    else {
        return Err("ASTM fixture".into());
    };
    Ok(stack)
}
fn autolyser(
    config: &Validated,
) -> Result<&analyte_config::AutolyserStack, Box<dyn std::error::Error>> {
    let analyte_config::Stack::Autolyser(stack) = config.effective().stack() else {
        return Err("Autolyser fixture".into());
    };
    Ok(stack)
}
fn vendor_namespace(
    config: &Validated,
) -> Result<&analyte_model::Identifier, Box<dyn std::error::Error>> {
    match config.effective().stack() {
        analyte_config::Stack::Vidas(s) => Ok(s.orders().ok_or("orders")?.patient_namespace()),
        analyte_config::Stack::Autolyser(s) => Ok(s.orders().ok_or("orders")?.patient_namespace()),
        analyte_config::Stack::Astm(analyte_config::RecordTransport::Stream(_))
        | analyte_config::Stack::Hl7(analyte_config::RecordTransport::Stream(_))
        | analyte_config::Stack::Law(_)
        | analyte_config::Stack::Poct(_)
        | analyte_config::Stack::Astm(analyte_config::RecordTransport::Folder(_))
        | analyte_config::Stack::Hl7(analyte_config::RecordTransport::Folder(_)) => {
            Err("vendor fixture".into())
        }
    }
}
