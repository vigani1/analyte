//! Checks outgoing test codes against the machine's code system and translates
//! specimen and priority codes through explicit tables.
use crate::orders::{AppOrderPriority, OrderMappingError};
use analyte_config::{OrderPriority, OutboundSpecimen};
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
/// Finds an exact app namespace/code in an explicitly configured specimen table.
/// # Errors
/// Returns `UnmappedSpecimenType` when the table or exact code is absent.
pub fn map_specimen<'a>(
    table: Option<&'a [OutboundSpecimen]>,
    requested: &NamespacedId,
) -> Result<&'a OutboundSpecimen, OrderMappingError> {
    table
        .and_then(|table| table.iter().find(|entry| entry.app == *requested))
        .ok_or(OrderMappingError::UnmappedSpecimenType)
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
