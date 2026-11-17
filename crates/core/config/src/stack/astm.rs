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
    /// Serial or TCP link.
    pub(super) transport: StreamTransport,
    /// Maximum frame bytes including overhead.
    pub(super) frame_limit: usize,
    record_boundaries: analyte_framing::astm::RecordBoundaries,
    /// Required LIS01 checksum policy.
    pub(super) checksum_hex: ChecksumHex,
    /// Host receive and send deadlines.
    pub(super) timeouts: DialogTimeouts,
    /// Required simultaneous-ENQ policy.
    pub(super) contention_priority: Priority,
    /// LIS02 settings.
    pub(super) syntax: AstmSyntax,
    /// Required field selectors and code tables.
    pub(super) mappings: Mappings,
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
    /// Supplies the LIS02 parser with the configured resource ceilings.
    #[must_use]
    pub fn parser_limits(&self) -> analyte_astm::Limits {
        analyte_astm::Limits {
            bytes: self.limits.bytes,
            record: self.limits.record_bytes,
            fields: self.limits.fields,
            repeats: self.limits.repetitions,
            components: self.limits.components,
        }
    }
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
    pub fn query(&self) -> &Option<OrderQuery> {
        &self.query
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
    /// Returns the inbound record policy selected only by a validated departure.
    #[must_use]
    pub fn record_boundaries(&self) -> analyte_framing::astm::RecordBoundaries {
        self.record_boundaries
    }
    /// Returns the selected digit-case policy without relaxing checksum correctness.
    #[must_use]
    pub fn checksum_policy(&self) -> analyte_framing::astm::HexPolicy {
        match self.checksum_hex {
            ChecksumHex::Strict => analyte_framing::astm::HexPolicy::Strict,
            ChecksumHex::AcceptLowercase => analyte_framing::astm::HexPolicy::AcceptLowercase,
        }
    }
    /// Checksum spelling accepted by the framer.
    #[must_use]
    pub fn checksum_hex(&self) -> ChecksumHex {
        self.checksum_hex
    }
    /// Bounded protocol waits used by the connection state machine.
    #[must_use]
    pub fn timeouts(&self) -> &DialogTimeouts {
        &self.timeouts
    }
    /// Side that wins simultaneous link establishment.
    #[must_use]
    pub fn contention_priority(&self) -> Priority {
        self.contention_priority
    }
    /// Record grammar settings applied after framing.
    #[must_use]
    pub fn syntax(&self) -> &AstmSyntax {
        &self.syntax
    }
    /// Explicit source-to-common-model mappings; unknown codes are not guessed.
    #[must_use]
    pub fn mappings(&self) -> &Mappings {
        &self.mappings
    }
    /// Validated order policy used before encoding commands.
    #[must_use]
    pub fn orders(&self) -> &Option<AstmOrders> {
        &self.orders
    }
}

pub(super) fn build(
    machine: &ParsedMachine<'_>,
    timeouts: DialogTimeouts,
    path: &str,
) -> Result<AstmStack, Error> {
    Ok(AstmStack {
        transport: stream(&machine.transport, path)?,
        frame_limit: machine.framing.limit,
        record_boundaries: if machine
            .deviations
            .iter()
            .any(|d| matches!(d, MachineDeparture::AstmPackedRecordsInFrame(_)))
        {
            analyte_framing::astm::RecordBoundaries::Packed
        } else {
            analyte_framing::astm::RecordBoundaries::Separate
        },
        timeouts,
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
        syntax: astm_syntax(&machine.syntax),
        mappings: machine
            .mappings
            .clone()
            .ok_or_else(|| invalid(path, "mappings", "record stack requires mappings"))?,
        orders: machine
            .orders
            .as_ref()
            .map(|o| astm_orders(o, path))
            .transpose()?,
    })
}

impl AstmStack {
    /// Returns whether this selected protocol permits unsolicited work.
    #[must_use]
    pub fn send_ahead(&self) -> bool {
        self.orders().as_ref().is_some_and(AstmOrders::send_ahead)
    }
}
