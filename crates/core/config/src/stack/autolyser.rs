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
    /// Independent copy of these immutable validated settings.
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
    pub fn policy(&self) -> &Autolyser {
        &self.policy
    }
    /// Validated order policy used before encoding commands.
    #[must_use]
    pub fn orders(&self) -> &Option<AutolyserOrders> {
        &self.orders
    }
}

pub(super) fn build(
    machine: &ParsedMachine<'_>,
    timeouts: DialogTimeouts,
    path: &str,
) -> Result<AutolyserStack, Error> {
    Ok(AutolyserStack {
        transport: stream(&machine.transport, path)?,
        frame_limit: machine.framing.limit,
        timeouts,
        limits: limits(&machine.syntax),
        codes: code_tables(machine, path)?,
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
        self.orders()
            .as_ref()
            .is_some_and(AutolyserOrders::send_ahead)
    }
}
