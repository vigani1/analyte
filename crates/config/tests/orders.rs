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
            format!("{VENDOR_ORDERS}clone = false\n")
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
    invalid_key(vendor_config(&definition, VENDOR_ORDERS), "orders.clone");
    for value in [true, false] {
        let section = format!("{VENDOR_ORDERS}clone = {value}\n");
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
        vendor_config(&vidas, &format!("{VENDOR_ORDERS}clone = false\n")),
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
            format!("{VENDOR_ORDERS}clone=false\n")
        } else {
            VENDOR_ORDERS.to_owned()
        };
        invalid_key(vendor_config(definition, &section), "orders.send_ahead");
        // Receive-only machines need neither outbound setting.
        vendor_config(definition, "")?;
    }
    Ok(())
}
