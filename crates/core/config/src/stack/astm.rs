//! Constructs validated ASTM protocol settings.
use super::*;

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

#[expect(
    clippy::expect_used,
    reason = "private conversion runs only after validate_machine proves required sections and options"
)]
pub(super) fn astm_orders(orders: &Orders) -> AstmOrders {
    AstmOrders {
        tables: order_tables(orders),
        query: orders.query.clone(),
        send_ahead: orders.send_ahead,
        layout: orders
            .astm
            .clone()
            .expect("validated ASTM orders require O-record layout"),
    }
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
    /// Returns the validated encoding setting.
    #[must_use]
    pub fn encoding(&self) -> Encoding {
        self.encoding
    }
    /// Returns the validated limits setting.
    #[must_use]
    pub fn limits(&self) -> &SyntaxLimits {
        &self.limits
    }
}

impl AstmOrders {
    /// Returns the validated tables setting.
    #[must_use]
    pub fn tables(&self) -> &OrderTables {
        &self.tables
    }
    /// Returns the validated query setting.
    #[must_use]
    pub fn query(&self) -> &Option<OrderQuery> {
        &self.query
    }
    /// Returns the validated send ahead setting.
    #[must_use]
    pub fn send_ahead(&self) -> bool {
        self.send_ahead
    }
    /// Returns the validated layout setting.
    #[must_use]
    pub fn layout(&self) -> &AstmOrderFields {
        &self.layout
    }
}

impl AstmStack {
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
    /// Returns the validated checksum hex setting.
    #[must_use]
    pub fn checksum_hex(&self) -> ChecksumHex {
        self.checksum_hex
    }
    /// Returns the validated timeouts setting.
    #[must_use]
    pub fn timeouts(&self) -> &DialogTimeouts {
        &self.timeouts
    }
    /// Returns the validated contention priority setting.
    #[must_use]
    pub fn contention_priority(&self) -> Priority {
        self.contention_priority
    }
    /// Returns the validated syntax setting.
    #[must_use]
    pub fn syntax(&self) -> &AstmSyntax {
        &self.syntax
    }
    /// Returns the validated mappings setting.
    #[must_use]
    pub fn mappings(&self) -> &Mappings {
        &self.mappings
    }
    /// Returns the validated orders setting.
    #[must_use]
    pub fn orders(&self) -> &Option<AstmOrders> {
        &self.orders
    }
}

#[expect(clippy::expect_used, reason = "conversion follows semantic validation")]
pub(super) fn build(machine: &MachineDocument, timeouts: DialogTimeouts) -> AstmStack {
    AstmStack {
        transport: stream(&machine.transport),
        frame_limit: machine.framing.limit,
        timeouts,
        checksum_hex: machine
            .framing
            .checksum_hex
            .expect("validated LIS01 framing requires checksum policy"),
        contention_priority: machine
            .dialog
            .contention_priority
            .expect("validated LIS01 dialog requires contention priority"),
        syntax: astm_syntax(&machine.syntax),
        mappings: machine
            .mappings
            .clone()
            .expect("validated record stack requires mappings"),
        orders: machine.orders.as_ref().map(astm_orders),
    }
}
