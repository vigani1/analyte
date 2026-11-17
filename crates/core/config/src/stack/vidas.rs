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
    /// Supported connection settings.
    pub(super) transport: StreamTransport,
    /// Maximum framed payload bytes.
    pub(super) frame_limit: usize,
    /// Host receive and send deadlines.
    pub(super) timeouts: DialogTimeouts,
    /// Syntax resource ceilings.
    pub(super) limits: SyntaxLimits,
    /// Required exact test and unit translations.
    pub(super) codes: CodeTables,
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
    /// Physical link settings, with unsupported protocol/link combinations rejected at load time.
    #[must_use]
    pub fn transport(&self) -> &StreamTransport {
        &self.transport
    }
    /// Maximum retained frame size before framing refuses more bytes.
    #[must_use]
    pub fn frame_limit(&self) -> usize {
        self.frame_limit
    }
    /// Bounded protocol waits used by the connection state machine.
    #[must_use]
    pub fn timeouts(&self) -> &DialogTimeouts {
        &self.timeouts
    }
    /// Size and count ceilings applied before parsing untrusted records.
    #[must_use]
    pub fn limits(&self) -> &SyntaxLimits {
        &self.limits
    }
    /// Explicit instrument code mappings used during interpretation.
    #[must_use]
    pub fn codes(&self) -> &CodeTables {
        &self.codes
    }
    /// Protocol behavior selected by configuration and checked at load time.
    #[must_use]
    pub fn policy(&self) -> &Vidas {
        &self.policy
    }
    /// Validated order policy used before encoding commands.
    #[must_use]
    pub fn orders(&self) -> &Option<VidasOrders> {
        &self.orders
    }
}

pub(super) fn build(
    machine: &ParsedMachine<'_>,
    timeouts: DialogTimeouts,
    path: &str,
) -> Result<VidasStack, Error> {
    Ok(VidasStack {
        transport: stream(&machine.transport, path)?,
        frame_limit: machine.framing.limit,
        timeouts,
        limits: limits(&machine.syntax),
        codes: code_tables(machine, path)?,
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
        self.orders().as_ref().is_some_and(VidasOrders::send_ahead)
    }
}
