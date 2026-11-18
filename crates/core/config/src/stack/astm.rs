//! Constructs validated ASTM protocol settings.
use super::{
    AstmOrderFields, ChecksumHex, DialogTimeouts, Encoding, Error, MachineDeparture, Mappings,
    OrderQuery, OrderTables, Orders, Priority, StreamTransport, Syntax, SyntaxLimits, invalid,
    limits, order_tables, stream,
};
use crate::ParsedMachine;

/// LIS02 syntax settings, without HL7-only options.
#[derive(Debug, Clone)]
pub struct AstmSyntax {
    /// Strict input encoding.
    pub(super) encoding: Encoding,
    /// Resource ceilings.
    pub(super) limits: SyntaxLimits,
}

/// Optional ASTM query policy and required O-record layout.
#[derive(Debug, Clone)]
pub struct AstmOrders {
    /// Outbound translations.
    pub(super) tables: OrderTables,
    /// Explicit query capability, if enabled.
    pub(super) query: Option<OrderQuery>,
    /// Explicit work-list permission.
    pub(super) send_ahead: bool,
    /// Required ASTM destinations.
    pub(super) layout: AstmOrderFields,
}

/// LIS01/LIS02 settings; no independently selectable layer kinds.
#[derive(Debug, Clone)]
pub struct AstmStack {
    /// Shared link, resource and interpretation settings.
    pub(super) settings: super::StackSettings<StreamTransport, AstmSyntax, Mappings>,
    packed_records: bool,
    /// Required LIS01 checksum policy.
    pub(super) checksum_hex: ChecksumHex,
    /// Required simultaneous-ENQ policy.
    pub(super) contention_priority: Priority,
    /// Explicit outbound capability; omission disables it.
    pub(super) orders: Option<AstmOrders>,
}

pub(super) fn astm_syntax(syntax: &Syntax) -> AstmSyntax {
    AstmSyntax {
        encoding: syntax.encoding,
        limits: limits(syntax),
    }
}

pub(super) fn astm_orders(orders: &Orders, path: &str) -> Result<AstmOrders, Error> {
    Ok(AstmOrders {
        tables: order_tables(orders),
        query: orders.query.clone(),
        send_ahead: orders.send_ahead,
        layout: orders
            .astm
            .clone()
            .ok_or_else(|| invalid(path, "orders.astm", "ASTM orders require O-record layout"))?,
    })
}

impl AstmSyntax {
    /// Text decoding policy; undecodable bytes remain retained evidence.
    #[must_use]
    pub fn encoding(&self) -> Encoding {
        self.encoding
    }
    /// Size and count ceilings applied before parsing untrusted records.
    #[must_use]
    pub fn limits(&self) -> &SyntaxLimits {
        &self.limits
    }
}

impl AstmOrders {
    /// Configured code tables used without guessing unknown values.
    #[must_use]
    pub fn tables(&self) -> &OrderTables {
        &self.tables
    }
    /// Host-query settings validated for this protocol.
    #[must_use]
    pub fn query(&self) -> Option<&OrderQuery> {
        self.query.as_ref()
    }
    /// Work-list settings; absence does not authorize unsolicited orders.
    #[must_use]
    pub fn send_ahead(&self) -> bool {
        self.send_ahead
    }
    /// Record field positions used to read this configured dialect.
    #[must_use]
    pub fn layout(&self) -> &AstmOrderFields {
        &self.layout
    }
}

impl AstmStack {
    /// Whether a validated departure allows packed records in one frame.
    #[must_use]
    pub fn packed_records(&self) -> bool {
        self.packed_records
    }
    /// Checksum spelling accepted by the framer.
    #[must_use]
    pub fn checksum_hex(&self) -> ChecksumHex {
        self.checksum_hex
    }
    /// Side that wins simultaneous link establishment.
    #[must_use]
    pub fn contention_priority(&self) -> Priority {
        self.contention_priority
    }
    /// Validated order policy used before encoding commands.
    #[must_use]
    pub fn orders(&self) -> Option<&AstmOrders> {
        self.orders.as_ref()
    }
}

pub(super) fn build(
    machine: &ParsedMachine<'_>,
    timeouts: DialogTimeouts,
    path: &str,
) -> Result<AstmStack, Error> {
    Ok(AstmStack {
        settings: super::StackSettings {
            transport: stream(&machine.transport, path)?,
            frame_limit: machine.framing.limit,
            timeouts,
            syntax: astm_syntax(&machine.syntax),
            codes: machine
                .mappings
                .clone()
                .ok_or_else(|| invalid(path, "mappings", "record stack requires mappings"))?,
        },
        packed_records: machine
            .deviations
            .iter()
            .any(|d| matches!(d, MachineDeparture::AstmPackedRecordsInFrame(_))),
        checksum_hex: machine.framing.checksum_hex.ok_or_else(|| {
            invalid(
                path,
                "framing.checksum_hex",
                "LIS01 framing requires checksum policy",
            )
        })?,
        contention_priority: machine.dialog.contention_priority.ok_or_else(|| {
            invalid(
                path,
                "dialog.contention_priority",
                "LIS01 dialog requires contention priority",
            )
        })?,
        orders: machine
            .orders
            .as_ref()
            .map(|o| astm_orders(o, path))
            .transpose()?,
    })
}

impl AstmStack {
    /// Borrows the shared validated settings without changing protocol-specific types.
    #[must_use]
    pub fn settings(&self) -> &super::StackSettings<StreamTransport, AstmSyntax, Mappings> {
        &self.settings
    }
}
