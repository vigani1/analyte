//! Holds validated outbound order translation tables.
use super::{OrderPriority, Orders, OutboundSpecimen, OutboundTest};

/// Outbound field-based translations shared by ASTM and HL7.
#[derive(Debug, Clone)]
pub struct OrderTables {
    /// Exact outbound test translations.
    pub(super) tests: Vec<OutboundTest>,
    /// Explicit specimen translations; absence maps no values.
    pub(super) specimen_types: Option<Vec<OutboundSpecimen>>,
    /// Exact outbound priority translations.
    pub(super) priorities: Vec<OrderPriority>,
}

pub(super) fn order_tables(orders: &Orders) -> OrderTables {
    OrderTables {
        tests: orders.tests.clone(),
        specimen_types: orders.specimen_types.clone(),
        priorities: orders.priorities.clone(),
    }
}

impl OrderTables {
    /// Returns the validated tests setting.
    #[must_use]
    pub fn tests(&self) -> &Vec<OutboundTest> {
        &self.tests
    }
    /// Returns the validated specimen types setting.
    #[must_use]
    pub fn specimen_types(&self) -> &Option<Vec<OutboundSpecimen>> {
        &self.specimen_types
    }
    /// Returns the validated priorities setting.
    #[must_use]
    pub fn priorities(&self) -> &Vec<OrderPriority> {
        &self.priorities
    }
}
