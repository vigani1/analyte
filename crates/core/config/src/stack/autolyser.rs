//! Constructs validated AUTOLYSER protocol settings.
use super::{
    Autolyser, CodeTables, DialogTimeouts, Error, OrderTables, StreamTransport, SyntaxLimits,
    code_tables, invalid, limits, order_tables, stream,
};
use crate::ParsedMachine;
use analyte_model::Identifier;

/// Autolyser work lists carry installed clone behavior and patient namespace.
#[derive(Debug, Clone)]
pub struct AutolyserOrders {
    /// Outbound translations.
    pub(super) tables: OrderTables,
    /// Required patient assigning namespace.
    pub(super) patient_namespace: Identifier,
    /// Explicit clone behavior, including false.
    pub(super) clone: bool,
    /// Explicit download permission.
    pub(super) send_ahead: bool,
}

/// Autolyser stack with its required installed policy.
#[derive(Debug, Clone)]
pub struct AutolyserStack {
    /// Shared link, resource and interpretation settings.
    pub(super) settings: super::StackSettings<StreamTransport, SyntaxLimits, CodeTables>,
    /// Required installed protocol policy.
    pub(super) policy: Autolyser,
    /// Explicit outbound capability; omission disables it.
    pub(super) orders: Option<AutolyserOrders>,
}

impl AutolyserOrders {
    /// Configured code tables used without guessing unknown values.
    #[must_use]
    pub fn tables(&self) -> &OrderTables {
        &self.tables
    }
    /// Assigning system for patient identities from this connection.
    #[must_use]
    pub fn patient_namespace(&self) -> &Identifier {
        &self.patient_namespace
    }
    /// Whether work lists request sample cloning (explicit, including false).
    #[must_use]
    pub fn clone_enabled(&self) -> bool {
        self.clone
    }
    /// Work-list settings; absence does not authorize unsolicited orders.
    #[must_use]
    pub fn send_ahead(&self) -> bool {
        self.send_ahead
    }
}

impl AutolyserStack {
    /// Protocol behavior selected by configuration and checked at load time.
    #[must_use]
    pub fn policy(&self) -> &Autolyser {
        &self.policy
    }
    /// Validated order policy used before encoding commands.
    #[must_use]
    pub fn orders(&self) -> Option<&AutolyserOrders> {
        self.orders.as_ref()
    }
}

pub(super) fn build(
    machine: &ParsedMachine<'_>,
    timeouts: DialogTimeouts,
    path: &str,
) -> Result<AutolyserStack, Error> {
    Ok(AutolyserStack {
        settings: super::StackSettings {
            transport: stream(&machine.transport, path)?,
            frame_limit: machine.framing.limit,
            timeouts,
            syntax: limits(&machine.syntax),
            codes: code_tables(machine, path)?,
        },
        policy: machine
            .autolyser
            .clone()
            .ok_or_else(|| invalid(path, "autolyser", "Autolyser stack requires policy"))?,
        orders: machine
            .orders
            .as_ref()
            .map(|orders| {
                Ok::<_, Error>(AutolyserOrders {
                    tables: order_tables(orders),
                    patient_namespace: orders.patient_namespace.clone().ok_or_else(|| {
                        invalid(
                            path,
                            "orders.patient_namespace",
                            "Autolyser orders require patient namespace",
                        )
                    })?,
                    clone: orders.clone.ok_or_else(|| {
                        invalid(
                            path,
                            "orders.clone",
                            "Autolyser orders require clone behavior",
                        )
                    })?,
                    send_ahead: orders.send_ahead,
                })
            })
            .transpose()?,
    })
}

impl AutolyserStack {
    /// Returns whether this selected protocol permits unsolicited work.
    #[must_use]
    pub fn send_ahead(&self) -> bool {
        self.orders().is_some_and(AutolyserOrders::send_ahead)
    }
}

impl AutolyserStack {
    /// Borrows the shared validated settings without changing protocol-specific types.
    #[must_use]
    pub fn settings(&self) -> &super::StackSettings<StreamTransport, SyntaxLimits, CodeTables> {
        &self.settings
    }
}
