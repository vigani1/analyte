//! Constructs validated HL7 protocol settings.
use super::*;
use analyte_model::Identifier;

/// HL7 syntax settings with every required option present.
#[derive(Debug, Clone)]
pub struct Hl7Syntax {
    /// Strict input encoding.
    pub(super) encoding: Encoding,
    /// Negotiated HL7 edition.
    pub(super) edition: Edition,
    /// Negotiated segment ending.
    pub(super) terminator: Terminator,
    /// Malformed escape policy.
    pub(super) escapes: Escapes,
    /// Validated named vendor departure for UTF-8 in HL7 2.3.1.
    pub(super) allow_utf8_in_v231: bool,
    /// Resource ceilings.
    pub(super) limits: SyntaxLimits,
}

/// An enabled HL7 query capability always carries its layout.
#[derive(Debug, Clone)]
pub struct Hl7Query {
    /// Deadline and untrusted-cache policy.
    pub(super) policy: OrderQuery,
    /// Required legacy query layout.
    pub(super) layout: Hl7OrderQuery,
}

/// An enabled unsolicited HL7 capability always has its namespace.
#[derive(Debug, Clone)]
pub struct Hl7SendOrders {
    /// Required header and specimen layout.
    pub(super) settings: Hl7Send,
    /// Required assigning namespace.
    pub(super) placer_namespace: Identifier,
}

/// Independent, explicitly enabled HL7 query and send capabilities.
#[derive(Debug, Clone)]
pub struct Hl7Orders {
    /// Optional assigning namespace for explicitly supplied patient identities.
    pub(super) patient_namespace: Option<Identifier>,
    /// Outbound translations.
    pub(super) tables: OrderTables,
    /// Query capability; omission disables it.
    pub(super) query: Option<Hl7Query>,
    /// Send-ahead capability; omission disables it.
    pub(super) send: Option<Hl7SendOrders>,
}

/// Generic HL7 over MLLP/TCP.
#[derive(Debug, Clone)]
pub struct Hl7Stack {
    /// TCP link.
    pub(super) transport: TcpTransport,
    /// Maximum MLLP payload bytes.
    pub(super) frame_limit: usize,
    /// Host receive and send deadlines.
    pub(super) timeouts: DialogTimeouts,
    /// Required acknowledgment promise.
    pub(super) acknowledgment: Responsibility,
    /// Complete HL7 settings.
    pub(super) syntax: Hl7Syntax,
    /// Required field selectors and code tables.
    pub(super) mappings: Mappings,
    /// Explicit outbound capability; omission disables it.
    pub(super) orders: Option<Hl7Orders>,
}

pub(super) fn hl7_syntax(syntax: &Syntax, path: &str) -> Result<Hl7Syntax, Error> {
    Ok(Hl7Syntax {
        allow_utf8_in_v231: syntax.allow_utf8_in_v231,
        encoding: syntax.encoding,
        edition: syntax
            .edition
            .ok_or_else(|| invalid(path, "syntax.edition", "HL7 syntax requires edition"))?,
        terminator: syntax
            .terminator
            .ok_or_else(|| invalid(path, "syntax.terminator", "HL7 syntax requires terminator"))?,
        escapes: syntax
            .escapes
            .ok_or_else(|| invalid(path, "syntax.escapes", "HL7 syntax requires escape policy"))?,
        limits: limits(syntax),
    })
}

pub(super) fn hl7_orders(orders: &Orders, path: &str) -> Result<Hl7Orders, Error> {
    Ok(Hl7Orders {
        patient_namespace: orders.patient_namespace.clone(),
        tables: order_tables(orders),
        query: orders
            .query
            .as_ref()
            .map(|policy| {
                Ok::<_, Error>(Hl7Query {
                    policy: policy.clone(),
                    layout: orders
                        .hl7
                        .clone()
                        .ok_or_else(|| invalid(path, "orders.hl7", "HL7 query requires layout"))?,
                })
            })
            .transpose()?,
        send: orders
            .hl7_send
            .as_ref()
            .map(|settings| {
                Ok::<_, Error>(Hl7SendOrders {
                    settings: settings.clone(),
                    placer_namespace: orders.placer_namespace.clone().ok_or_else(|| {
                        invalid(
                            path,
                            "orders.placer_namespace",
                            "HL7 send requires placer namespace",
                        )
                    })?,
                })
            })
            .transpose()?,
    })
}

impl Hl7Syntax {
    /// Supplies all negotiated HL7 parser options without optional required fields.
    #[must_use]
    pub fn options(&self) -> analyte_hl7::Options {
        analyte_hl7::Options {
            allow_utf8_in_v231: self.allow_utf8_in_v231,
            ..hl7_options(self.encoding, self.terminator, self.escapes, &self.limits)
        }
    }
}

impl Hl7Syntax {
    /// Returns the validated encoding setting.
    #[must_use]
    pub fn encoding(&self) -> Encoding {
        self.encoding
    }
    /// Returns the validated edition setting.
    #[must_use]
    pub fn edition(&self) -> Edition {
        self.edition
    }
    /// Returns the validated terminator setting.
    #[must_use]
    pub fn terminator(&self) -> Terminator {
        self.terminator
    }
    /// Returns the validated escapes setting.
    #[must_use]
    pub fn escapes(&self) -> Escapes {
        self.escapes
    }
    /// Returns the validated allow utf8 in v231 setting.
    #[must_use]
    pub fn allow_utf8_in_v231(&self) -> bool {
        self.allow_utf8_in_v231
    }
    /// Returns the validated limits setting.
    #[must_use]
    pub fn limits(&self) -> &SyntaxLimits {
        &self.limits
    }
}

impl Hl7Query {
    /// Returns the validated policy setting.
    #[must_use]
    pub fn policy(&self) -> &OrderQuery {
        &self.policy
    }
    /// Returns the validated layout setting.
    #[must_use]
    pub fn layout(&self) -> &Hl7OrderQuery {
        &self.layout
    }
}

impl Hl7SendOrders {
    /// Returns the validated settings setting.
    #[must_use]
    pub fn settings(&self) -> &Hl7Send {
        &self.settings
    }
    /// Returns the validated placer namespace setting.
    #[must_use]
    pub fn placer_namespace(&self) -> &Identifier {
        &self.placer_namespace
    }
}

impl Hl7Orders {
    /// Returns the validated patient namespace setting.
    #[must_use]
    pub fn patient_namespace(&self) -> &Option<Identifier> {
        &self.patient_namespace
    }
    /// Returns the validated tables setting.
    #[must_use]
    pub fn tables(&self) -> &OrderTables {
        &self.tables
    }
    /// Returns the validated query setting.
    #[must_use]
    pub fn query(&self) -> &Option<Hl7Query> {
        &self.query
    }
    /// Returns the validated send setting.
    #[must_use]
    pub fn send(&self) -> &Option<Hl7SendOrders> {
        &self.send
    }
}

impl Hl7Stack {
    /// Returns the validated transport setting.
    #[must_use]
    pub fn transport(&self) -> &TcpTransport {
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
    /// Returns the validated acknowledgment setting.
    #[must_use]
    pub fn acknowledgment(&self) -> Responsibility {
        self.acknowledgment
    }
    /// Returns the validated syntax setting.
    #[must_use]
    pub fn syntax(&self) -> &Hl7Syntax {
        &self.syntax
    }
    /// Returns the validated mappings setting.
    #[must_use]
    pub fn mappings(&self) -> &Mappings {
        &self.mappings
    }
    /// Returns the validated orders setting.
    #[must_use]
    pub fn orders(&self) -> &Option<Hl7Orders> {
        &self.orders
    }
}

pub(super) fn build(
    machine: &MachineDocument,
    timeouts: DialogTimeouts,
    path: &str,
) -> Result<Hl7Stack, Error> {
    Ok(Hl7Stack {
        transport: tcp(&machine.transport, path)?,
        frame_limit: machine.framing.limit,
        timeouts,
        acknowledgment: machine.dialog.acknowledgment.ok_or_else(|| {
            invalid(
                path,
                "dialog.acknowledgment",
                "MLLP dialog requires acknowledgment policy",
            )
        })?,
        syntax: hl7_syntax(&machine.syntax, path)?,
        mappings: machine
            .mappings
            .clone()
            .ok_or_else(|| invalid(path, "mappings", "record stack requires mappings"))?,
        orders: machine
            .orders
            .as_ref()
            .map(|o| hl7_orders(o, path))
            .transpose()?,
    })
}
