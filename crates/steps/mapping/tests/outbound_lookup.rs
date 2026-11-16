//! Tests explicit outgoing code tables without inferred mappings.
use analyte_config::{
    MachinePriority, OrderPriority, OutboundSpecimen, OutboundTest, PriorityCode,
};
use analyte_mapping::{
    AppOrderPriority, OrderMappingError, outbound_priority, outbound_specimen, outbound_test,
};
use analyte_model::{Identifier, NamespacedId};
type TestResult = Result<(), Box<dyn std::error::Error>>;
fn id(namespace: &str, value: &str) -> Result<NamespacedId, analyte_model::TextError> {
    Ok(NamespacedId {
        namespace: Identifier::new(namespace)?,
        value: Identifier::new(value)?,
    })
}
#[test]
fn adr0004_outbound_test_requires_exact_namespace_case_and_code() -> TestResult {
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
        outbound_test(&table, &id("lab", "001a")?)?.machine_components,
        ["", "ExactCode"]
    );
    for requested in [id("LAB", "001a")?, id("lab", "001A")?, id("lab", "1a")?] {
        assert!(
            matches!(outbound_test(&table, &requested), Err(OrderMappingError::UnmappedTest(actual)) if actual == requested)
        );
    }
    Ok(())
}
#[test]
fn adr0004_outbound_specimen_absent_table_or_code_never_defaults() -> TestResult {
    let table = vec![OutboundSpecimen {
        app: id("lab", "serum")?,
        machine: Identifier::new("S")?,
    }];
    assert_eq!(
        outbound_specimen(Some(&table), &id("lab", "serum")?)?
            .machine
            .as_str(),
        "S"
    );
    assert!(matches!(
        outbound_specimen(None, &id("lab", "serum")?),
        Err(OrderMappingError::UnmappedSpecimenType)
    ));
    assert!(matches!(
        outbound_specimen(Some(&table), &id("other", "serum")?),
        Err(OrderMappingError::UnmappedSpecimenType)
    ));
    Ok(())
}
#[test]
fn adr0004_outbound_priority_preserves_configured_wire_spelling_without_guessing_meaning()
-> TestResult {
    let table = vec![OrderPriority {
        app: Identifier::new("urgent")?,
        machine: MachinePriority::Code(PriorityCode::T),
    }];
    assert_eq!(
        outbound_priority(&table, AppOrderPriority::Urgent)?
            .machine
            .as_str(),
        "T"
    );
    for priority in [AppOrderPriority::Routine, AppOrderPriority::Stat] {
        assert!(matches!(
            outbound_priority(&table, priority),
            Err(OrderMappingError::UnmappedPriority)
        ));
    }
    Ok(())
}
