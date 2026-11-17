//! Constructs validated VIDAS protocol settings.
use super::{
    CodeTables, DialogTimeouts, Error, OrderPriority, OutboundTest, StreamTransport, SyntaxLimits,
    Vidas, code_tables, invalid, limits, stream,
};
use crate::ParsedMachine;
use analyte_model::Identifier;

/// VIDAS work lists carry only supported outbound settings.
#[derive(Debug, Clone)]
pub struct VidasOrders {
    /// Exact assay translations.
    pub(super) tests: Vec<OutboundTest>,
    /// Explicit accepted app priorities.
    pub(super) priorities: Vec<OrderPriority>,
    /// Required patient assigning namespace.
    pub(super) patient_namespace: Identifier,
    /// Explicit download permission.
    pub(super) send_ahead: bool,
}

/// Vidas stack with its required installed policy.
#[derive(Debug, Clone)]
pub struct VidasStack {
    /// Shared link, resource and interpretation settings.
    pub(super) settings: super::StackSettings<StreamTransport, SyntaxLimits, CodeTables>,
    /// Required installed protocol policy.
    pub(super) policy: Vidas,
    /// Explicit outbound capability; omission disables it.
    pub(super) orders: Option<VidasOrders>,
}

impl VidasOrders {
    /// Explicit application-to-instrument test-code mapping.
    #[must_use]
    pub fn tests(&self) -> &Vec<OutboundTest> {
        &self.tests
    }
    /// Explicit order-priority mapping.
    #[must_use]
    pub fn priorities(&self) -> &Vec<OrderPriority> {
        &self.priorities
    }
    /// Assigning system for patient identities from this connection.
    #[must_use]
    pub fn patient_namespace(&self) -> &Identifier {
        &self.patient_namespace
    }
    /// Work-list settings; absence does not authorize unsolicited orders.
    #[must_use]
    pub fn send_ahead(&self) -> bool {
        self.send_ahead
    }
}

impl VidasStack {
    /// Protocol behavior selected by configuration and checked at load time.
    #[must_use]
    pub fn policy(&self) -> &Vidas {
        &self.policy
    }
    /// Validated order policy used before encoding commands.
    #[must_use]
    pub fn orders(&self) -> Option<&VidasOrders> {
        self.orders.as_ref()
    }
}

pub(super) fn build(
    machine: &ParsedMachine<'_>,
    timeouts: DialogTimeouts,
    path: &str,
) -> Result<VidasStack, Error> {
    Ok(VidasStack {
        settings: super::StackSettings {
            transport: stream(&machine.transport, path)?,
            frame_limit: machine.framing.limit,
            timeouts,
            syntax: limits(&machine.syntax),
            codes: code_tables(machine, path)?,
        },
        policy: machine
            .vidas
            .clone()
            .ok_or_else(|| invalid(path, "vidas", "VIDAS stack requires policy"))?,
        orders: machine
            .orders
            .as_ref()
            .map(|orders| {
                Ok::<_, Error>(VidasOrders {
                    tests: orders.tests.clone(),
                    priorities: orders.priorities.clone(),
                    patient_namespace: orders.patient_namespace.clone().ok_or_else(|| {
                        invalid(
                            path,
                            "orders.patient_namespace",
                            "VIDAS orders require patient namespace",
                        )
                    })?,
                    send_ahead: orders.send_ahead,
                })
            })
            .transpose()?,
    })
}

impl VidasStack {
    /// Returns whether this selected protocol permits unsolicited work.
    #[must_use]
    pub fn send_ahead(&self) -> bool {
        self.orders().is_some_and(VidasOrders::send_ahead)
    }
}

impl VidasStack {
    /// Borrows the shared validated settings without changing protocol-specific types.
    #[must_use]
    pub fn settings(&self) -> &super::StackSettings<StreamTransport, SyntaxLimits, CodeTables> {
        &self.settings
    }
}
