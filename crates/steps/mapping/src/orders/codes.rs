//! Checks outgoing test and specimen codes against the machine's own code
//! systems and translates contract priorities through the explicit table.
use crate::orders::{AppOrderPriority, OrderMappingError};
use analyte_config::OrderPriority;
use analyte_model::{Identifier, NamespacedId};
/// Returns the order test's code, exactly as the lab app wrote it, when its
/// namespace is exactly the machine's own test code system (ADR 0029).
/// # Errors
/// Returns `ForeignTest` for any other namespace; nothing is translated or defaulted.
pub fn map_test<'a>(
    test_system: &Identifier,
    requested: &'a NamespacedId,
) -> Result<&'a str, OrderMappingError> {
    if requested.namespace == *test_system {
        Ok(requested.value.as_str())
    } else {
        Err(OrderMappingError::ForeignTest(requested.clone()))
    }
}
/// Returns the order's specimen code, exactly as the lab app wrote it, when
/// its namespace is exactly the machine's own specimen code system (ADR 0030).
/// # Errors
/// Returns `ForeignSpecimenType` for any other namespace, or when the machine
/// declares no specimen code system; nothing is translated or defaulted.
pub fn map_specimen<'a>(
    specimen_system: Option<&Identifier>,
    requested: &'a NamespacedId,
) -> Result<&'a str, OrderMappingError> {
    match specimen_system {
        Some(system) if requested.namespace == *system => Ok(requested.value.as_str()),
        Some(_) | None => Err(OrderMappingError::ForeignSpecimenType(requested.clone())),
    }
}
/// Finds the explicitly configured app priority without interpreting its wire meaning.
/// # Errors
/// Returns `UnmappedPriority`; wire code meanings remain protocol-owned mapping rules.
pub fn map_priority(
    table: &[OrderPriority],
    requested: AppOrderPriority,
) -> Result<&OrderPriority, OrderMappingError> {
    table
        .iter()
        .find(|entry| entry.app.as_str() == requested.code())
        .ok_or(OrderMappingError::UnmappedPriority)
}
