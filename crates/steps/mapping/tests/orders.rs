//! Tests configured outgoing orders and code mappings.
use analyte_config::{Draft, Validated};
use analyte_fixtures::ConfigEdit as _;
use analyte_mapping::{AppOrder, AppOrderPriority, OrderMappingError, map_order};
use analyte_model::{Identifier, NamespacedId, SampleId};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn rule7_outbound_selected_component_at_configured_ceiling_is_preserved() -> TestResult {
    let base = include_str!("../../../core/config/examples/astm.toml")
        .rewrite("components = 128", "components = 4");
    let section = include_str!("../../../core/config/examples/orders.toml")
        .rewrite("field=3,component=1", "field=3,component=4");
    let definition = format!("{base}\n{section}");
    let cfg = Draft::load(
        ("machine", &definition),
        (
            "site",
            include_str!("../../../core/config/examples/site.toml"),
        ),
        ("local", "schema_version=1"),
    )?
    .validate()?;
    let mapped = map_order(&order()?, cfg.effective().stack())?;
    assert_eq!(mapped.record()?, b"O|1|^^^001a||^^^SEND|R||||||N\r");
    Ok(())
}

#[test]
fn rule7_display_text_bound_is_inclusive_and_refuses_delimiter_injection() -> TestResult {
    let section = include_str!("../../../core/config/examples/hl7_orders.toml").rewrite(
        "namespace=\"synthetic\",value=\"TEST\"",
        "namespace=\"demo\",value=\"DEMO\"",
    );
    let definition = format!(
        "{}\n{section}",
        include_str!("../../../core/config/examples/hl7.toml")
    );
    let cfg = Draft::load(
        ("machine", &definition),
        (
            "site",
            include_str!("../../../core/config/examples/site.toml"),
        ),
        ("local", "schema_version=1"),
    )?
    .validate()?;
    for (length, accepted) in [(299, true), (300, true), (301, false)] {
        assert_eq!(
            analyte_mapping::map_display(
                &order()?,
                &"X".repeat(length),
                cfg.effective()
                    .stack()
                    .hl7_orders()
                    .expect("HL7 order fixture"),
                cfg.effective().stack().limits()
            )
            .is_ok(),
            accepted
        );
    }
    for text in ["injected|field", "injected^component", "café"] {
        assert!(matches!(
            analyte_mapping::map_display(
                &order()?,
                text,
                cfg.effective()
                    .stack()
                    .hl7_orders()
                    .expect("HL7 order fixture"),
                cfg.effective().stack().limits()
            ),
            Err(OrderMappingError::DisplayLineBounds)
        ));
    }
    Ok(())
}

fn bounded_order_config(key: &str, value: usize) -> Result<Validated, analyte_config::Error> {
    let base = include_str!("../../../core/config/examples/astm.toml");
    let old = match key {
        "bytes" => 1048576,
        "record_bytes" => 65536,
        "components" => 128,
        _ => unreachable!("fixed test keys"),
    };
    let section = include_str!("../../../core/config/examples/orders.toml");
    let definition = format!(
        "{}\n{section}",
        base.rewrite(&format!("{key} = {old}"), &format!("{key} = {value}"))
    );
    Draft::load(
        ("machine", &definition),
        (
            "site",
            include_str!("../../../core/config/examples/site.toml"),
        ),
        ("local", "schema_version=1"),
    )?
    .validate()
}

#[test]
fn rule7_outbound_encoded_record_and_message_limits_are_inclusive() -> TestResult {
    let expected = b"O|1|001a||^^^SEND|R||||||N\r";
    for (key, ceiling) in [
        ("bytes", expected.len()),
        ("record_bytes", expected.len() - 1),
    ] {
        for (bound, accepted) in [(ceiling - 1, false), (ceiling, true), (ceiling + 1, true)] {
            let mapped = map_order(
                &order()?,
                bounded_order_config(key, bound)?.effective().stack(),
            );
            assert_eq!(mapped.is_ok(), accepted, "{key}={bound}");
            if accepted {
                assert_eq!(mapped?.record()?, expected);
            } else {
                assert!(matches!(mapped, Err(OrderMappingError::AstmRecordLimit)));
            }
        }
    }
    let mapped = map_order(
        &order()?,
        bounded_order_config("components", 4)?.effective().stack(),
    )?;
    assert_eq!(mapped.record()?, expected);
    Ok(())
}

#[test]
fn rule5_outbound_sample_reserved_characters_are_refused_without_rewriting() -> TestResult {
    let settings = config(include_str!("../../../core/config/examples/orders.toml"))?;
    for sample in ["A|B", "A^B", "A&B", "A\\B", "café"] {
        let mut input = order()?;
        input.sample = SampleId::new(sample)?;
        assert!(
            matches!(
                map_order(&input, settings.effective().stack()),
                Err(OrderMappingError::AstmTextRepertoire)
            ),
            "{sample}"
        );
        assert_eq!(input.sample.as_str(), sample);
    }
    Ok(())
}

#[test]
fn rule7_outbound_test_count_is_nonempty_and_inclusive() -> TestResult {
    let definition = format!(
        "{}\n{}",
        include_str!("../../../core/config/examples/astm.toml")
            .rewrite("repetitions = 128", "repetitions = 2"),
        include_str!("../../../core/config/examples/orders.toml")
    );
    let settings = Draft::load(
        ("machine", &definition),
        (
            "site",
            include_str!("../../../core/config/examples/site.toml"),
        ),
        ("local", "schema_version=1"),
    )?
    .validate()?;
    for (count, accepted) in [(0, false), (1, true), (2, true), (3, false)] {
        let mut input = order()?;
        let test = input.tests.first().ok_or("test")?.clone();
        input.tests = vec![test; count];
        let mapped = map_order(&input, settings.effective().stack());
        assert_eq!(mapped.is_ok(), accepted, "count={count}");
        if !accepted {
            assert!(matches!(mapped, Err(OrderMappingError::AstmTestCount)));
        }
    }
    Ok(())
}

fn config(section: &str) -> Result<Validated, analyte_config::Error> {
    let definition = format!(
        "{}\n{section}",
        include_str!("../../../core/config/examples/astm.toml")
    );
    Draft::load(
        ("machine", &definition),
        (
            "site",
            include_str!("../../../core/config/examples/site.toml"),
        ),
        ("overrides", "schema_version=1"),
    )?
    .validate()
}

fn order() -> Result<AppOrder, analyte_model::TextError> {
    Ok(AppOrder {
        sample: SampleId::new("001a")?,
        tests: vec![NamespacedId {
            namespace: Identifier::new("demo")?,
            value: Identifier::new("DEMO")?,
        }],
        priority: AppOrderPriority::Routine,
        position: None,
        patient: None,
        specimen_type: None,
    })
}

#[test]
fn lis02_section8_outbound_table_preserves_sample_and_never_reverses_inbound() -> TestResult {
    let config = config(include_str!("../../../core/config/examples/orders.toml"))?;
    let mapped = map_order(&order()?, config.effective().stack())?;
    assert_eq!(mapped.record()?, b"O|1|001a||^^^SEND|R||||||N\r");
    Ok(())
}

#[test]
fn adr0004_disabled_unmapped_system_priority_and_missing_position_refuse_output() -> TestResult {
    assert!(matches!(
        map_order(&order()?, config("")?.effective().stack()),
        Err(OrderMappingError::Disabled)
    ));
    let section = include_str!("../../../core/config/examples/orders.toml");
    let settings = config(section)?;
    let mut input = order()?;
    input.tests[0].namespace = Identifier::new("different-system")?;
    assert!(matches!(
        map_order(&input, settings.effective().stack()),
        Err(OrderMappingError::UnmappedTest(_))
    ));
    input = order()?;
    input.priority = AppOrderPriority::Stat;
    assert!(matches!(
        map_order(&input, settings.effective().stack()),
        Err(OrderMappingError::UnmappedPriority)
    ));
    let position = format!(
        "{section}\nposition = {{group=\"order\",record=\"O\",field=4,component=1,repetition=1}}\n"
    );
    assert!(matches!(
        map_order(&order()?, config(&position)?.effective().stack()),
        Err(OrderMappingError::MissingPosition)
    ));
    Ok(())
}

fn context_section() -> String {
    include_str!("../../../core/config/examples/orders.toml").rewrite("[orders.query]", "specimen_types=[{app={namespace='demo-specimens',value='SERUM'},machine='SER'}]\n[orders.query]")
}
fn context_order() -> Result<AppOrder, analyte_model::TextError> {
    let mut value = order()?;
    value.position = Some(analyte_model::SamplePosition {
        kind: analyte_model::CarrierKind::Tray,
        carrier: Some(Identifier::new("A")?),
        slot: Identifier::new("03")?,
    });
    value.specimen_type = Some(NamespacedId {
        namespace: Identifier::new("demo-specimens")?,
        value: Identifier::new("SERUM")?,
    });
    Ok(value)
}
const POSITION: &str = "position={group='order',record='O',field=4,component=2,repetition=1}\n";
const SPECIMEN_TYPE: &str =
    "specimen_type={group='order',record='O',field=16,component=1,repetition=1}\n";

#[test]
fn lis02_section8_4_4_and_8_4_16_1_configured_destinations_get_only_explicit_context() -> TestResult
{
    for (destinations, expected) in [
        (
            format!("{POSITION}{SPECIMEN_TYPE}"),
            b"O|1|001a|^03|^^^SEND|R||||||N||||SER\r".as_slice(),
        ),
        (
            POSITION.into(),
            b"O|1|001a|^03|^^^SEND|R||||||N\r".as_slice(),
        ),
        (
            SPECIMEN_TYPE.into(),
            b"O|1|001a||^^^SEND|R||||||N||||SER\r".as_slice(),
        ),
        (String::new(), b"O|1|001a||^^^SEND|R||||||N\r".as_slice()),
    ] {
        let settings = config(&format!("{}\n{destinations}", context_section()))?;
        assert_eq!(
            map_order(&context_order()?, settings.effective().stack())?.record()?,
            expected
        );
    }
    Ok(())
}
#[test]
fn lis02_section8_4_16_1_unmapped_or_missing_specimen_never_guesses_a_code() -> TestResult {
    let settings = config(&format!("{}\n{SPECIMEN_TYPE}", context_section()))?;
    let mut value = context_order()?;
    value.specimen_type.as_mut().ok_or("specimen")?.namespace =
        Identifier::new("different-system")?;
    assert!(matches!(
        map_order(&value, settings.effective().stack()),
        Err(OrderMappingError::UnmappedSpecimenType)
    ));
    // With no configured destination the same unknown value is not transmitted.
    assert_eq!(
        map_order(&value, config(&context_section())?.effective().stack())?.record()?,
        b"O|1|001a||^^^SEND|R||||||N\r"
    );
    value.specimen_type = None;
    assert!(matches!(
        map_order(&value, settings.effective().stack()),
        Err(OrderMappingError::MissingSpecimenType)
    ));
    Ok(())
}

#[test]
fn hl7_251_section5_10_dsp_layout_is_explicit_and_context_is_never_guessed() -> TestResult {
    let section = include_str!("../../../core/config/examples/hl7_orders.toml").rewrite(
        "namespace=\"synthetic\",value=\"TEST\"",
        "namespace=\"demo\",value=\"DEMO\"",
    );
    let definition = format!(
        "{}\n{section}",
        include_str!("../../../core/config/examples/hl7.toml")
    );
    let settings = Draft::load(
        ("machine", &definition),
        (
            "site",
            include_str!("../../../core/config/examples/site.toml"),
        ),
        ("overrides", "schema_version=1"),
    )?
    .validate()?;
    let input = order()?;
    assert_eq!(
        analyte_mapping::map_display(
            &input,
            "order-a",
            settings
                .effective()
                .stack()
                .hl7_orders()
                .expect("HL7 order fixture"),
            settings.effective().stack().limits()
        )?,
        vec![
            b"001a".to_vec(),
            b"SEND".to_vec(),
            b"R".to_vec(),
            Vec::new(),
            b"order-a".to_vec(),
            Vec::new()
        ]
    );
    let mut wrong = order()?;
    wrong.sample = SampleId::new("bad|delimiter")?;
    assert!(matches!(
        analyte_mapping::map_display(
            &wrong,
            "order-a",
            settings
                .effective()
                .stack()
                .hl7_orders()
                .expect("HL7 order fixture"),
            settings.effective().stack().limits()
        ),
        Err(OrderMappingError::DisplayLineBounds)
    ));
    wrong = order()?;
    wrong.tests.first_mut().ok_or("test")?.namespace = Identifier::new("unmapped-system")?;
    assert!(matches!(
        analyte_mapping::map_display(
            &wrong,
            "order-a",
            settings
                .effective()
                .stack()
                .hl7_orders()
                .expect("HL7 order fixture"),
            settings.effective().stack().limits()
        ),
        Err(OrderMappingError::UnmappedTest(_))
    ));
    wrong = order()?;
    wrong.priority = AppOrderPriority::Stat;
    assert!(matches!(
        analyte_mapping::map_display(
            &wrong,
            "order-a",
            settings
                .effective()
                .stack()
                .hl7_orders()
                .expect("HL7 order fixture"),
            settings.effective().stack().limits()
        ),
        Err(OrderMappingError::UnmappedPriority)
    ));
    Ok(())
}

#[test]
fn hl7_251_section5_10_optional_patient_specimen_and_slot_use_only_explicit_sources() -> TestResult
{
    let section = include_str!("../../../core/config/examples/hl7_orders.toml")
        .rewrite(
            "namespace=\"synthetic\",value=\"TEST\"",
            "namespace=\"demo\",value=\"DEMO\"",
        )
        .rewrite(
            "[orders.query]",
            r#"patient_namespace = "patients"
specimen_types = [{app={namespace="types",value="SERUM"},machine="SER"}]
[orders.query]"#,
        )
        .rewrite(
            r#"{source="position"}"#,
            r#"{source="position"},{source="specimen_type"},{source="patient_id"}"#,
        );
    let definition = format!(
        "{}\n{section}",
        include_str!("../../../core/config/examples/hl7.toml")
    );
    let settings = Draft::load(
        ("machine", &definition),
        (
            "site",
            include_str!("../../../core/config/examples/site.toml"),
        ),
        ("overrides", "schema_version=1"),
    )?
    .validate()?;
    let mut input = order()?;
    let empty = analyte_mapping::map_display(
        &input,
        "order-a",
        settings
            .effective()
            .stack()
            .hl7_orders()
            .expect("HL7 order fixture"),
        settings.effective().stack().limits(),
    )?;
    assert_eq!(
        empty.get(5..),
        Some([Vec::new(), Vec::new(), Vec::new()].as_slice())
    );
    input.position = Some(analyte_model::SamplePosition {
        kind: analyte_model::CarrierKind::Rack,
        carrier: Some(Identifier::new("rack-a")?),
        slot: Identifier::new("007b")?,
    });
    input.specimen_type = Some(NamespacedId {
        namespace: Identifier::new("types")?,
        value: Identifier::new("SERUM")?,
    });
    input.patient = Some(NamespacedId {
        namespace: Identifier::new("patients")?,
        value: Identifier::new("000Patient")?,
    });
    let mapped = analyte_mapping::map_display(
        &input,
        "order-a",
        settings
            .effective()
            .stack()
            .hl7_orders()
            .expect("HL7 order fixture"),
        settings.effective().stack().limits(),
    )?;
    assert_eq!(
        mapped.get(5..),
        Some([b"007b".to_vec(), b"SER".to_vec(), b"000Patient".to_vec()].as_slice())
    );
    input.patient.as_mut().ok_or("patient")?.namespace = Identifier::new("wrong-namespace")?;
    assert!(matches!(
        analyte_mapping::map_display(
            &input,
            "order-a",
            settings
                .effective()
                .stack()
                .hl7_orders()
                .expect("HL7 order fixture"),
            settings.effective().stack().limits()
        ),
        Err(OrderMappingError::PatientNamespace)
    ));
    Ok(())
}
