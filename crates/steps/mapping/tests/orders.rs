//! Tests configured outgoing orders: machine test and specimen codes as written, explicit priorities.
use analyte_config::{Draft, Validated};
use analyte_fixtures::ConfigEdit as _;
use analyte_mapping::{AppOrder, AppOrderPriority, OrderMappingError};
use analyte_model::{Identifier, NamespacedId, SampleId};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// Safety rule 7 (ADR 0003)
#[test]
fn component_at_configured_ceiling_is_kept() -> TestResult {
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
    assert_eq!(mapped.encode(), b"O|1|^^^001a||^^^SEND|R||||||N\r");
    Ok(())
}

// Safety rule 7 (ADR 0003)
#[test]
fn display_bound_is_inclusive_and_refuses_delimiters() -> TestResult {
    let section = include_str!("../../../core/config/examples/hl7_orders.toml").to_owned();
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
            Err(analyte_mapping::hl7::OrderError::DisplayLineBounds)
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

// Safety rule 7 (ADR 0003)
#[test]
fn record_and_message_limits_are_inclusive() -> TestResult {
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
                assert_eq!(mapped?.encode(), expected);
            } else {
                assert!(matches!(
                    mapped,
                    Err(analyte_mapping::astm::OrderError::RecordLimit)
                ));
            }
        }
    }
    let mapped = map_order(
        &order()?,
        bounded_order_config("components", 4)?.effective().stack(),
    )?;
    assert_eq!(mapped.encode(), expected);
    Ok(())
}

// Safety rule 5 (ADR 0003)
#[test]
fn reserved_sample_characters_are_refused_unchanged() -> TestResult {
    let settings = config(include_str!("../../../core/config/examples/orders.toml"))?;
    for sample in ["A|B", "A^B", "A&B", "A\\B", "café"] {
        let mut input = order()?;
        input.sample = SampleId::new(sample)?;
        assert!(
            matches!(
                map_order(&input, settings.effective().stack()),
                Err(analyte_mapping::astm::OrderError::TextRepertoire)
            ),
            "{sample}"
        );
        assert_eq!(input.sample.as_str(), sample);
    }
    Ok(())
}

// Safety rule 7 (ADR 0003)
#[test]
fn test_count_is_nonempty_and_inclusive() -> TestResult {
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
            assert!(matches!(
                mapped,
                Err(analyte_mapping::astm::OrderError::TestCount)
            ));
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
            namespace: Identifier::new("synthetic")?,
            value: Identifier::new("SEND")?,
        }],
        test_names: vec![None],
        priority: AppOrderPriority::Routine,
        position: None,
        patient: None,
        specimen_type: None,
    })
}

// CLSI LIS02-A2 §8
#[test]
fn outbound_table_keeps_sample_and_never_reverses() -> TestResult {
    let config = config(include_str!("../../../core/config/examples/orders.toml"))?;
    let mapped = map_order(&order()?, config.effective().stack())?;
    assert_eq!(mapped.encode(), b"O|1|001a||^^^SEND|R||||||N\r");
    Ok(())
}

// ADR 0004
#[test]
fn disabled_foreign_or_unmapped_orders_refuse_output() -> TestResult {
    assert!(matches!(
        map_order(&order()?, config("")?.effective().stack()),
        Err(analyte_mapping::astm::OrderError::Shared(
            OrderMappingError::Disabled
        ))
    ));
    let section = include_str!("../../../core/config/examples/orders.toml");
    let settings = config(section)?;
    let mut input = order()?;
    input.tests[0].namespace = Identifier::new("different-system")?;
    assert!(matches!(
        map_order(&input, settings.effective().stack()),
        Err(analyte_mapping::astm::OrderError::Shared(
            OrderMappingError::ForeignTest(_)
        ))
    ));
    input = order()?;
    input.priority = AppOrderPriority::Stat;
    assert!(matches!(
        map_order(&input, settings.effective().stack()),
        Err(analyte_mapping::astm::OrderError::Shared(
            OrderMappingError::UnmappedPriority
        ))
    ));
    let position = format!(
        "{section}\nposition = {{group=\"order\",record=\"O\",field=4,component=1,repetition=1}}\n"
    );
    assert!(matches!(
        map_order(&order()?, config(&position)?.effective().stack()),
        Err(analyte_mapping::astm::OrderError::Shared(
            OrderMappingError::MissingPosition
        ))
    ));
    Ok(())
}

// ADR 0030: the specimen code system is declared exactly with its destination.
fn context_section(specimen: bool) -> String {
    let section = include_str!("../../../core/config/examples/orders.toml").to_owned();
    if specimen {
        section.rewrite(
            "[orders.query]",
            "specimen_system='demo-specimens'\n[orders.query]",
        )
    } else {
        section
    }
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
        value: Identifier::new("SER")?,
    });
    Ok(value)
}
const POSITION: &str = "position={group='order',record='O',field=4,component=2,repetition=1}\n";
const SPECIMEN_TYPE: &str =
    "specimen_type={group='order',record='O',field=16,component=1,repetition=1}\n";

// CLSI LIS02-A2 §8.4.4, §8.4.16.1
#[test]
fn destinations_get_only_explicit_context() -> TestResult {
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
        let specimen = destinations.contains("specimen_type");
        let settings = config(&format!("{}\n{destinations}", context_section(specimen)))?;
        assert_eq!(
            map_order(&context_order()?, settings.effective().stack())?.encode(),
            expected
        );
    }
    Ok(())
}
// CLSI LIS02-A2 §8.4.16.1; ADR 0030
#[test]
fn foreign_specimen_never_guesses_a_code() -> TestResult {
    let settings = config(&format!("{}\n{SPECIMEN_TYPE}", context_section(true)))?;
    let mut value = context_order()?;
    value.specimen_type.as_mut().ok_or("specimen")?.namespace =
        Identifier::new("different-system")?;
    assert!(matches!(
        map_order(&value, settings.effective().stack()),
        Err(analyte_mapping::astm::OrderError::Shared(
            OrderMappingError::ForeignSpecimenType(_)
        ))
    ));
    // With no configured destination the same value is not transmitted.
    assert_eq!(
        map_order(&value, config(&context_section(false))?.effective().stack())?.encode(),
        b"O|1|001a||^^^SEND|R||||||N\r"
    );
    // LIS02 §6: a code holding a delimiter cannot be carried, so it is refused,
    // never escaped or trimmed.
    let mut delimited = context_order()?;
    delimited.specimen_type.as_mut().ok_or("specimen")?.value = Identifier::new("S^R")?;
    assert!(matches!(
        map_order(&delimited, settings.effective().stack()),
        Err(analyte_mapping::astm::OrderError::TextRepertoire)
    ));
    value.specimen_type = None;
    assert!(matches!(
        map_order(&value, settings.effective().stack()),
        Err(analyte_mapping::astm::OrderError::Shared(
            OrderMappingError::MissingSpecimenType
        ))
    ));
    Ok(())
}

// HL7 2.5.1 §5.10
#[test]
fn dsp_layout_is_explicit_without_guessed_context() -> TestResult {
    let section = include_str!("../../../core/config/examples/hl7_orders.toml").to_owned();
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
        Err(analyte_mapping::hl7::OrderError::DisplayLineBounds)
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
        Err(analyte_mapping::hl7::OrderError::Shared(
            OrderMappingError::ForeignTest(_)
        ))
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
        Err(analyte_mapping::hl7::OrderError::Shared(
            OrderMappingError::UnmappedPriority
        ))
    ));
    Ok(())
}

// HL7 2.5.1 §5.10
#[test]
fn dsp_optional_context_uses_explicit_sources() -> TestResult {
    let section = include_str!("../../../core/config/examples/hl7_orders.toml")
        .rewrite(
            "[orders.query]",
            r#"patient_namespace = "patients"
specimen_system = "types"
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
        value: Identifier::new("SER")?,
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
    // ADR 0030: a specimen type in another code system is refused, never shown.
    input.specimen_type.as_mut().ok_or("specimen")?.namespace = Identifier::new("lab-app")?;
    assert!(matches!(
        analyte_mapping::map_display(
            &input,
            "order-a",
            settings
                .effective()
                .stack()
                .hl7_orders()
                .expect("HL7 order fixture"),
            settings.effective().stack().limits(),
        ),
        Err(analyte_mapping::hl7::OrderError::Shared(
            OrderMappingError::ForeignSpecimenType(_)
        ))
    ));
    input.specimen_type.as_mut().ok_or("specimen")?.namespace = Identifier::new("types")?;
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
        Err(analyte_mapping::hl7::OrderError::Shared(
            OrderMappingError::PatientNamespace
        ))
    ));
    Ok(())
}

// HL7 chapter 4
#[test]
fn patient_and_orc_group_refusals_are_typed() -> TestResult {
    use analyte_mapping::hl7::WireError;
    for (require_patient, id, expected) in [
        (true, "order-1".to_owned(), WireError::PatientNameRequired),
        (false, "x".repeat(23), WireError::GroupIdTooLong),
    ] {
        let definition = format!(
            "{}\n{}",
            include_str!("../../../core/config/examples/hl7.toml"),
            include_str!("../../../core/config/examples/hl7_send.toml").rewrite(
                "require_patient_identification = false",
                &format!("require_patient_identification = {require_patient}"),
            ),
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
        let analyte_config::Stack::Hl7(analyte_config::RecordTransport::Stream(stack)) =
            cfg.effective().stack()
        else {
            return Err("HL7 stream fixture expected".into());
        };
        let raw =
            include_str!("../../../../docs/contract/vectors/order.json").replace("order-1", &id);
        let app = analyte_contract::decode(raw.as_bytes())?;
        let result = analyte_mapping::map_hl7_order(
            stack.settings().syntax(),
            stack.orders().ok_or("orders")?,
            &app,
            analyte_hl7::Options::default(),
        );
        assert!(matches!(
            (&result, &expected),
            (
                Err(WireError::PatientNameRequired),
                WireError::PatientNameRequired
            ) | (Err(WireError::GroupIdTooLong), WireError::GroupIdTooLong)
        ));
    }
    Ok(())
}

// HL7 2.5.1 §4.5.3.4 OBR-4 CE; ADR 0029: the order's code goes out as
// written with the machine's code system in CE.3; CE.2 carries the lab app's
// name when it gives one and stays empty otherwise. Another code system is refused.
#[test]
fn hl7_send_writes_the_code_its_system_and_the_lab_app_name() -> TestResult {
    let definition = format!(
        "{}\n{}",
        include_str!("../../../core/config/examples/hl7.toml"),
        include_str!("../../../core/config/examples/hl7_send.toml"),
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
    let analyte_config::Stack::Hl7(analyte_config::RecordTransport::Stream(stack)) =
        cfg.effective().stack()
    else {
        return Err("HL7 stream fixture expected".into());
    };
    let order = |test: &str| {
        format!(
            r#"{{"id":"order-1","version":1,"state":"requested","sample_id":"000Ab12","tests":[{test}],"priority":"routine","target_analyzers":null,"expires_at":null,"tombstone":false,"specimen_type":{{"namespace":"L","value":"SER"}}}}"#
        )
    };
    let named = order(r#"{"namespace":"synthetic","value":"TEST","name":"Synthetic test"}"#);
    let plain = order(r#"{"namespace":"synthetic","value":"TEST"}"#);
    let foreign = order(r#"{"namespace":"lab-app","value":"TEST"}"#);
    for (raw, expected) in [
        (named, Some("|TEST^Synthetic test^synthetic|")),
        (plain, Some("|TEST^^synthetic|")),
        (foreign, None),
    ] {
        let app = analyte_contract::decode(raw.as_bytes())?;
        let command = analyte_mapping::map_hl7_order(
            stack.settings().syntax(),
            stack.orders().ok_or("orders")?,
            &app,
            analyte_hl7::Options::default(),
        )?;
        let namespace = Identifier::new("LAB")?;
        let tests = command.tests().to_vec();
        let items = [analyte_mapping::OutboundItem {
            placer: "placer-1",
            namespace: Some(&namespace),
            tests: &tests,
        }];
        let mapped = command.map(
            &items,
            false,
            analyte_hl7::Response {
                control_id: analyte_hl7::ControlId::new(b"send-1")?,
                timestamp: b"20261122120000",
            },
        );
        match expected {
            Some(obr4) => {
                let message = mapped?;
                let text = String::from_utf8_lossy(message.message().raw()).into_owned();
                assert!(text.contains(obr4), "{text}");
            }
            None => assert!(matches!(
                mapped,
                Err(analyte_mapping::hl7::WireError::ForeignTest(_))
            )),
        }
    }
    Ok(())
}

fn map_order(
    order: &AppOrder,
    stack: &analyte_config::Stack,
) -> Result<analyte_astm::OrderRecord, analyte_mapping::astm::OrderError> {
    let settings = stack.astm_orders().ok_or(OrderMappingError::Disabled)?;
    analyte_mapping::astm::map_order(
        order,
        stack
            .astm_syntax()
            .ok_or(OrderMappingError::Disabled)?
            .limits(),
        settings,
    )
}
