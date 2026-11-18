//! Tests explicit outgoing code tables without inferred mappings.
use analyte_config::{
    MachinePriority, OrderPriority, OutboundSpecimen, OutboundTest, PriorityCode,
};
use analyte_mapping::{AppOrderPriority, OrderMappingError, map_priority, map_specimen, map_test};
use analyte_model::{Identifier, NamespacedId};
type TestResult = Result<(), Box<dyn std::error::Error>>;
fn id(namespace: &str, value: &str) -> Result<NamespacedId, analyte_model::TextError> {
    Ok(NamespacedId {
        namespace: Identifier::new(namespace)?,
        value: Identifier::new(value)?,
    })
}
// ADR 0004
#[test]
fn test_lookup_needs_exact_namespace_and_code() -> TestResult {
    let table = vec![
        OutboundTest {
            app: id("other", "001a")?,
            machine_components: vec!["wrong".into()],
        },
        OutboundTest {
            app: id("lab", "001a")?,
            machine_components: vec!["".into(), "ExactCode".into()],
        },
    ];
    assert_eq!(
        map_test(&table, &id("lab", "001a")?)?.machine_components,
        ["", "ExactCode"]
    );
    for requested in [id("LAB", "001a")?, id("lab", "001A")?, id("lab", "1a")?] {
        assert!(
            matches!(map_test(&table, &requested), Err(OrderMappingError::UnmappedTest(actual)) if actual == requested)
        );
    }
    Ok(())
}
// ADR 0004
#[test]
fn missing_specimen_mapping_never_defaults() -> TestResult {
    let table = vec![OutboundSpecimen {
        app: id("lab", "serum")?,
        machine: Identifier::new("S")?,
    }];
    assert_eq!(
        map_specimen(Some(&table), &id("lab", "serum")?)?
            .machine
            .as_str(),
        "S"
    );
    assert!(matches!(
        map_specimen(None, &id("lab", "serum")?),
        Err(OrderMappingError::UnmappedSpecimenType)
    ));
    assert!(matches!(
        map_specimen(Some(&table), &id("other", "serum")?),
        Err(OrderMappingError::UnmappedSpecimenType)
    ));
    Ok(())
}
// ADR 0004
#[test]
fn priority_uses_only_configured_spelling() -> TestResult {
    let table = vec![OrderPriority {
        app: Identifier::new("urgent")?,
        machine: MachinePriority::Code(PriorityCode::T),
    }];
    assert_eq!(
        map_priority(&table, AppOrderPriority::Urgent)?
            .machine
            .as_str(),
        "T"
    );
    for priority in [AppOrderPriority::Routine, AppOrderPriority::Stat] {
        assert!(matches!(
            map_priority(&table, priority),
            Err(OrderMappingError::UnmappedPriority)
        ));
    }
    Ok(())
}
