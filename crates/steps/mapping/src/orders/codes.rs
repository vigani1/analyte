//! Translates outgoing codes using explicit tables independent of result mappings.
use crate::orders::{AppOrderPriority, OrderMappingError};
use analyte_config::{OrderPriority, OutboundSpecimen, OutboundTest};
use analyte_model::NamespacedId;
/// Finds an exact app namespace/code in a validated outbound test table.
/// # Errors
/// Returns the requested unmapped identity, never a default or inbound-table match.
pub fn map_test<'a>(
    table: &'a [OutboundTest],
    requested: &NamespacedId,
) -> Result<&'a OutboundTest, OrderMappingError> {
    table
        .iter()
        .find(|entry| entry.app == *requested)
        .ok_or_else(|| OrderMappingError::UnmappedTest(requested.clone()))
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
