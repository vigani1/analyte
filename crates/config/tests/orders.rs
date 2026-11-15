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
        let orders = valid
            .effective()
            .machine()
            .orders
            .as_ref()
            .ok_or("orders")?;
        assert_eq!(
            orders
                .patient_namespace
                .as_ref()
                .ok_or("namespace")?
                .as_str(),
            "synthetic-patients"
        );
        assert!(orders.astm.is_none());
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
        assert_eq!(
            valid
                .effective()
                .machine()
                .orders
                .as_ref()
                .ok_or("orders")?
                .patient_namespace
                .as_ref()
                .ok_or("namespace")?
                .as_str(),
            "000LabA"
        );
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
            valid
                .effective()
                .machine()
                .orders
                .as_ref()
                .ok_or("orders")?
                .clone,
            Some(value)
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
