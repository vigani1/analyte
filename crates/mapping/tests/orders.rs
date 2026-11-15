//! Synthetic cases: LIS02-A2 §8 order fields and ADR 0004 explicit mapping.
use analyte_config::{Draft, Validated};
use analyte_mapping::{AppOrder, AppOrderPriority, OrderMappingError, map_order};
use analyte_model::{Identifier, NamespacedId, SampleId};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn config(section: &str) -> Result<Validated, analyte_config::Error> {
    let definition = format!(
        "{}\n{section}",
        include_str!("../../config/examples/astm.toml")
    );
    Draft::load(
        ("machine", &definition),
        ("site", include_str!("../../config/examples/site.toml")),
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
    let config = config(include_str!("../../config/examples/orders.toml"))?;
    let mapped = map_order(&order()?, &config)?;
    assert_eq!(mapped.record()?, b"O|1|001a||^^^SEND|R||||||N\r");
    Ok(())
}

#[test]
fn adr0004_disabled_unmapped_system_priority_and_missing_position_refuse_output() -> TestResult {
    assert!(matches!(
        map_order(&order()?, &config("")?),
        Err(OrderMappingError::Disabled)
    ));
    let section = include_str!("../../config/examples/orders.toml");
    let settings = config(section)?;
    let mut input = order()?;
    input.tests[0].namespace = Identifier::new("different-system")?;
    assert!(matches!(
        map_order(&input, &settings),
        Err(OrderMappingError::UnmappedTest(_))
    ));
    input = order()?;
    input.priority = AppOrderPriority::Stat;
    assert!(matches!(
        map_order(&input, &settings),
        Err(OrderMappingError::UnmappedPriority)
    ));
    let position = format!(
        "{section}\nposition = {{group=\"order\",record=\"O\",field=4,component=1,repetition=1}}\n"
    );
    assert!(matches!(
        map_order(&order()?, &config(&position)?),
        Err(OrderMappingError::MissingPosition)
    ));
    Ok(())
}

fn context_section() -> String {
    include_str!("../../config/examples/orders.toml").replace("[orders.query]", "specimen_types=[{app={namespace='demo-specimens',value='SERUM'},machine='SER'}]\n[orders.query]")
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
        assert_eq!(map_order(&context_order()?, &settings)?.record()?, expected);
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
        map_order(&value, &settings),
        Err(OrderMappingError::UnmappedSpecimenType)
    ));
    // With no configured destination the same unknown value is not transmitted.
    assert_eq!(
        map_order(&value, &config(&context_section())?)?.record()?,
        b"O|1|001a||^^^SEND|R||||||N\r"
    );
    value.specimen_type = None;
    assert!(matches!(
        map_order(&value, &settings),
        Err(OrderMappingError::MissingSpecimenType)
    ));
    Ok(())
}
