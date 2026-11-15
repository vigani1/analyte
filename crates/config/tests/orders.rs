//! ADR 0002: outbound capabilities must be explicitly configured.
use analyte_config::{Draft, Validated};
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
    for invalid in [section.replace("deadline_ms = 1000", "deadline_ms = 0"), section.replace("when_untrusted = \"error\"", ""), section.replace("send_ahead = true", ""), section.replace("machine_components=[\"\", \"\", \"\", \"SEND\"]}", "machine_components=[\"SEND\"]},{app={namespace=\"demo\",value=\"DEMO\"},machine_components=[\"OTHER\"]}")] {
        assert!(config(&invalid).is_err());
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
        let definition = definition.replace("download = false", "download = true");
        let section = if definition.contains("[autolyser]") {
            format!("{}clone = false\n", autolyser_orders())
        } else {
            VENDOR_ORDERS.to_owned()
        };
        let valid = vendor_config(&definition, &section)?;
        assert_eq!(vendor_namespace(&valid)?.as_str(), "synthetic-patients");
        assert!(matches!(
            valid.effective().stack(),
            analyte_config::Stack::VidasLiteral(_) | analyte_config::Stack::Autolyser(_)
        ));
        invalid_key(
            vendor_config(
                &definition,
                &section.replace("patient_namespace = \"synthetic-patients\"", ""),
            ),
            "orders.patient_namespace",
        );
        for namespace in ["", "bad\nnamespace"] {
            let invalid = section.replace("synthetic-patients", namespace);
            assert!(vendor_config(&definition, &invalid).is_err());
        }
        let exact = section.replace("synthetic-patients", "000LabA");
        let valid = vendor_config(&definition, &exact)?;
        assert_eq!(vendor_namespace(&valid)?.as_str(), "000LabA");
    }
    Ok(())
}

#[test]
fn autolyser_rev18_ch23_clone_requires_explicit_true_or_false() -> TestResult {
    let definition =
        include_str!("../examples/autolyser.toml").replace("download = false", "download = true");
    invalid_key(
        vendor_config(&definition, &autolyser_orders()),
        "orders.clone",
    );
    for value in [true, false] {
        let section = format!("{}clone = {value}\n", autolyser_orders());
        let valid = vendor_config(&definition, &section)?;
        assert_eq!(
            autolyser(&valid)?
                .orders()
                .as_ref()
                .ok_or("orders")?
                .clone_enabled(),
            value
        );
        assert!(valid.canonical().contains(&format!("clone = {value}")));
    }
    assert!(vendor_config(&definition, &format!("{VENDOR_ORDERS}clone = \"false\"\n")).is_err());
    Ok(())
}

#[test]
fn adr0002_outbound_settings_cannot_silently_apply_to_another_stack() -> TestResult {
    let vidas = include_str!("../examples/vidas_literal.toml")
        .replace("download = false", "download = true");
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
        config(&astm_section.replace(
            "send_ahead = true",
            "send_ahead = true\npatient_namespace=\"patients\"",
        )),
        "orders.patient_namespace",
    );
    invalid_key(
        config(&astm_section.replace("send_ahead = true", "send_ahead = true\nclone=false")),
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
        .replace("download = false", "download = true");
    for components in [r#"["ABCDE"]"#, r#"["A", "B"]"#, r#"["A|B"]"#] {
        invalid_key(
            vendor_config(
                &definition,
                &VENDOR_ORDERS.replace(r#"["SYN"]"#, components),
            ),
            "orders.tests",
        );
    }
    Ok(())
}

fn autolyser_orders() -> String {
    VENDOR_ORDERS
        .replace(r#"["SYN"]"#, r#"["SYN "]"#)
        .replace(r#"machine="R""#, r#"machine="T""#)
}

#[test]
fn autolyser_rev18_ch23_outbound_medium_list_and_analysis_tables_are_explicit() -> TestResult {
    let definition =
        include_str!("../examples/autolyser.toml").replace("download = false", "download = true");
    let section = format!("{}clone=false\n", autolyser_orders());
    let types = r#"[{app={namespace="synthetic",value="SERUM"},machine="S"}]"#;
    vendor_config(&definition, &format!("{section}specimen_types={types}\n"))?;
    for table in [
        "[]".to_owned(),
        types.replace(r#"machine="S""#, r#"machine="X""#),
        types.replace(
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
        vendor_config(&definition, &section.replace(r#"["SYN "]"#, r#"["SYN"]"#)),
        "orders.tests",
    );
    invalid_key(
        vendor_config(
            &definition,
            &section.replace(r#"machine="T""#, r#"machine="routine""#),
        ),
        "orders.priorities",
    );
    Ok(())
}

#[test]
fn lis02_section8_4_16_1_specimen_destination_and_code_table_preserve_field_ownership() -> TestResult
{
    let section=include_str!("../examples/orders.toml").replace("[orders.query]", "specimen_types=[{app={namespace='specimens',value='SERUM'},machine='SER'}]\n[orders.query]");
    let destination = "specimen_type={group='order',record='O',field=16,component=1,repetition=1}";
    config(&format!("{section}\n{destination}"))?;
    for invalid in [
        destination.replace("field=16", "field=20"),
        destination.replace("component=1", "component=2"),
        destination.replace("record='O'", "record='R'"),
        destination.replace("group='order'", "group='observation'"),
        destination.replace("repetition=1", "repetition=2"),
    ] {
        invalid_key(
            config(&format!("{section}\n{invalid}")),
            "orders.astm.specimen_type",
        );
    }
    invalid_key(
        config(&section.replace("machine='SER'", "machine='S^R'")),
        "orders.specimen_types",
    );
    let duplicated = section.replace(
        "machine='SER'}]",
        "machine='SER'},{app={namespace='specimens',value='SERUM'},machine='OTHER'}]",
    );
    invalid_key(config(&duplicated), "orders.specimen_types");
    Ok(())
}

#[test]
fn adr0002_hl7_legacy_query_requires_explicit_bounded_layout_and_selectors() -> TestResult {
    let definition = include_str!("../examples/hl7.toml");
    let section = include_str!("../examples/hl7_orders.toml");
    let valid = vendor_config(definition, section)?;
    assert!(
        hl7(&valid)?
            .orders()
            .as_ref()
            .and_then(|o| o.query().as_ref())
            .is_some()
    );
    for (invalid, key) in [
        (
            section.replace("acknowledgment_ms = 500", "acknowledgment_ms = 0"),
            "orders.hl7.acknowledgment_ms",
        ),
        (
            section.replace("sample_field = 8", "sample_field = 9"),
            "orders.hl7.sample_field",
        ),
        (
            section.replace("{field=9,value=\"ORD\"},", ""),
            "orders.hl7.filters",
        ),
        (
            section.replace("{source=\"sample_id\"},", ""),
            "orders.hl7.lines",
        ),
        (
            section.replace("{source=\"test_code\"},", ""),
            "orders.hl7.lines",
        ),
        (
            section.replace("send_ahead = false", "send_ahead = true"),
            "orders",
        ),
        (
            section.replace(
                "value=\"TEST\"}]",
                "value=\"TEST\"},{field=9,value=\"OTHER\"}]",
            ),
            "orders.hl7.filters",
        ),
    ] {
        invalid_key(vendor_config(definition, &invalid), key);
    }
    assert!(
        vendor_config(
            definition,
            &section.replace("deadline_ms = 1000", "deadline_ms = 60001")
        )
        .is_err()
    );
    assert!(
        vendor_config(
            definition,
            &section.replace("source=\"test_code\"", "source=\"guessed\"")
        )
        .is_err()
    );
    assert!(
        vendor_config(
            definition,
            &section.replace("value=\"\"", "value=\"injected|field\"")
        )
        .is_err()
    );
    invalid_key(
        vendor_config(
            definition,
            &section.replace(
                r#"when_untrusted = "error""#,
                r#"when_untrusted = "no_answer""#,
            ),
        ),
        "orders.query.when_untrusted",
    );
    assert!(
        vendor_config(
            definition,
            &section.replace(r#"sample_suffix = "^^^^^^^LCL""#, r#"sample_suffix = """#)
        )
        .is_err()
    );
    assert!(config(section).is_err());
    Ok(())
}

#[test]
fn adr0002_hl7_send_requires_namespace_valid_headers_item_codes_and_deadline() -> TestResult {
    let definition = include_str!("../examples/hl7.toml");
    let section = include_str!("../examples/hl7_send.toml");
    vendor_config(definition, section)?;
    for (invalid, key) in [
        (
            section.replace(r#"placer_namespace = "LAB""#, ""),
            "orders.placer_namespace",
        ),
        (
            section.replace("response_ms = 1500", "response_ms = 0"),
            "orders.hl7_send",
        ),
        (
            section.replace("sample_field = 18", "sample_field = 17"),
            "orders.hl7_send.position_field",
        ),
        (
            section.replace(
                "sample_field = 18",
                "sample_field = 18\nposition_field = 18",
            ),
            "orders.hl7_send.position_field",
        ),
        (
            section.replace(r#"["SYN","","L"]"#, r#"["SYN"]"#),
            "orders.tests",
        ),
    ] {
        invalid_key(vendor_config(definition, &invalid), key);
    }
    let invalid_processing = section.replace(r#"processing = "T""#, r#"processing = "unknown""#);
    assert!(matches!(
        vendor_config(definition, &invalid_processing),
        Err(analyte_config::Error::Schema { .. })
    ));
    assert!(config(section).is_err());
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
        let section = section.replace(r#"processing = "T""#, &format!("processing = \"{wire}\""));
        let valid = vendor_config(definition, &section)?;
        assert_eq!(
            hl7(&valid)?
                .orders()
                .as_ref()
                .expect("orders")
                .send()
                .as_ref()
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
        let section = section.replace(r#"machine="R""#, &format!("machine=\"{wire}\""));
        let valid = vendor_config(definition, &section)?;
        assert!(
            matches!(hl7(&valid)?.orders().as_ref().expect("orders").tables().priorities().first().expect("priority").machine, MachinePriority::Code(code) if code == expected)
        );
    }
    invalid_key(
        vendor_config(
            definition,
            &section.replace(r#"machine="R""#, r#"machine="custom""#),
        ),
        "orders.priorities",
    );
    // Configurable ASTM priority text must not be restricted to HL7 table 0485.
    let astm_text =
        include_str!("../examples/orders.toml").replace(r#"machine="R""#, r#"machine="custom""#);
    let valid = config(&astm_text)?;
    assert!(
        matches!(&astm(&valid)?.orders().as_ref().expect("orders").tables().priorities().first().expect("priority").machine, MachinePriority::Custom(value) if value.as_str() == "custom")
    );
    Ok(())
}

fn hl7(config: &Validated) -> Result<&analyte_config::Hl7Stack, Box<dyn std::error::Error>> {
    let analyte_config::Stack::Hl7(stack) = config.effective().stack() else {
        return Err("HL7 fixture".into());
    };
    Ok(stack)
}
fn astm(config: &Validated) -> Result<&analyte_config::AstmStack, Box<dyn std::error::Error>> {
    let analyte_config::Stack::Astm(stack) = config.effective().stack() else {
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
        analyte_config::Stack::VidasLiteral(s) => {
            Ok(s.orders().as_ref().ok_or("orders")?.patient_namespace())
        }
        analyte_config::Stack::Autolyser(s) => {
            Ok(s.orders().as_ref().ok_or("orders")?.patient_namespace())
        }
        analyte_config::Stack::Astm(_)
        | analyte_config::Stack::Hl7(_)
        | analyte_config::Stack::Law(_)
        | analyte_config::Stack::Poct(_)
        | analyte_config::Stack::FileAstm(_)
        | analyte_config::Stack::FileHl7(_) => Err("vendor fixture".into()),
    }
}
