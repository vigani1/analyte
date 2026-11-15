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
    })
}

#[test]
fn lis02_section8_outbound_table_preserves_sample_and_never_reverses_inbound() -> TestResult {
    let config = config(include_str!("../../config/examples/orders.toml"))?;
    let mapped = map_order(&order()?, &config)?;
    assert_eq!(mapped.record(), b"O|1|001a||^^^SEND|R||||||N\r");
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
