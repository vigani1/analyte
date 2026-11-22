//! Tests outgoing code checks and explicit tables without inferred mappings.
use analyte_config::{MachinePriority, OrderPriority, PriorityCode};
use analyte_mapping::{AppOrderPriority, OrderMappingError, map_priority, map_specimen, map_test};
use analyte_model::{Identifier, NamespacedId};
type TestResult = Result<(), Box<dyn std::error::Error>>;
fn id(namespace: &str, value: &str) -> Result<NamespacedId, analyte_model::TextError> {
    Ok(NamespacedId {
        namespace: Identifier::new(namespace)?,
        value: Identifier::new(value)?,
    })
}
// ADR 0029: an order test goes out exactly as written, and only in the
// machine's own code system; no other namespace is translated or folded.
#[test]
fn order_test_needs_the_machine_code_system_and_keeps_its_spelling() -> TestResult {
    let system = Identifier::new("lab")?;
    let requested = id("lab", "001a")?;
    assert_eq!(map_test(&system, &requested)?, "001a");
    for requested in [
        id("LAB", "001a")?,
        id("other", "001a")?,
        id("lab ", "001a")?,
    ] {
        assert!(
            matches!(map_test(&system, &requested), Err(OrderMappingError::ForeignTest(actual)) if actual == requested)
        );
    }
    Ok(())
}
// ADR 0030: an order's specimen type goes out exactly as written, and only in
// the machine's own specimen code system; nothing is translated or defaulted.
#[test]
fn specimen_type_needs_the_machine_specimen_system_and_keeps_its_spelling() -> TestResult {
    let system = Identifier::new("lab-specimens")?;
    let requested = id("lab-specimens", "0Ser")?;
    assert_eq!(map_specimen(Some(&system), &requested)?, "0Ser");
    for foreign in [
        id("LAB-SPECIMENS", "0Ser")?,
        id("other", "0Ser")?,
        id("lab-specimens ", "0Ser")?,
    ] {
        assert!(
            matches!(map_specimen(Some(&system), &foreign), Err(OrderMappingError::ForeignSpecimenType(actual)) if actual == foreign)
        );
    }
    // A machine that declares no specimen code system takes no specimen type.
    assert!(matches!(
        map_specimen(None, &requested),
        Err(OrderMappingError::ForeignSpecimenType(_))
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
