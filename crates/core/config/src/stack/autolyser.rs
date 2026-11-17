//! Constructs validated AUTOLYSER protocol settings.
use super::*;
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
    /// Returns the validated tables setting.
    #[must_use]
    pub fn tables(&self) -> &OrderTables {
        &self.tables
    }
    /// Returns the validated patient namespace setting.
    #[must_use]
    pub fn patient_namespace(&self) -> &Identifier {
        &self.patient_namespace
    }
    /// Returns the validated clone setting.
    #[must_use]
    pub fn clone_enabled(&self) -> bool {
        self.clone
    }
    /// Returns the validated send ahead setting.
    #[must_use]
    pub fn send_ahead(&self) -> bool {
        self.send_ahead
    }
}

impl AutolyserStack {
    /// Returns the validated transport setting.
    #[must_use]
    pub fn transport(&self) -> &StreamTransport {
        &self.transport
    }
    /// Returns the validated frame limit setting.
    #[must_use]
    pub fn frame_limit(&self) -> usize {
        self.frame_limit
    }
    /// Returns the validated timeouts setting.
    #[must_use]
    pub fn timeouts(&self) -> &DialogTimeouts {
        &self.timeouts
    }
    /// Returns the validated limits setting.
    #[must_use]
    pub fn limits(&self) -> &SyntaxLimits {
        &self.limits
    }
    /// Returns the validated codes setting.
    #[must_use]
    pub fn codes(&self) -> &CodeTables {
        &self.codes
    }
    /// Returns the validated policy setting.
    #[must_use]
    pub fn policy(&self) -> &Autolyser {
        &self.policy
    }
    /// Returns the validated orders setting.
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
