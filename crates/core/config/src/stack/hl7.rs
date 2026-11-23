//! Constructs validated HL7 protocol settings.
use super::{
    DialogTimeouts, Edition, Encoding, Error, Escapes, Hl7OrderQuery, Hl7Send, Mappings,
    OrderQuery, OrderTables, Orders, SyntaxLimits, TcpTransport, Terminator, invalid, limits,
    order_tables, tcp,
};
use crate::ParsedMachine;
use analyte_model::Identifier;

/// HL7 syntax settings with every required option present.
#[derive(Debug, Clone)]
pub struct Hl7Syntax {
    msh16_message_category: bool,
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
    /// Shared link, resource and interpretation settings.
    pub(super) settings: super::StackSettings<TcpTransport, Hl7Syntax, Mappings>,
    /// Explicit outbound capability; omission disables it.
    pub(super) orders: Option<Hl7Orders>,
}

pub(super) fn hl7_syntax(machine: &ParsedMachine<'_>, path: &str) -> Result<Hl7Syntax, Error> {
    let syntax = &machine.syntax;
    Ok(Hl7Syntax {
        msh16_message_category: machine
            .deviations
            .iter()
            .any(|rule| matches!(rule, crate::MachineDeparture::Hl7Msh16MessageCategory(_))),
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

pub(super) fn hl7_orders(
    orders: &Orders,
    machine: &ParsedMachine<'_>,
    path: &str,
) -> Result<Hl7Orders, Error> {
    Ok(Hl7Orders {
        patient_namespace: orders.patient_namespace.clone(),
        tables: order_tables(orders, machine, path)?,
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
    /// Whether an evidenced departure authorizes MSH-16 result categories.
    #[must_use]
    pub fn msh16_message_category(&self) -> bool {
        self.msh16_message_category
    }

    /// Text decoding policy; undecodable bytes remain retained evidence.
    #[must_use]
    pub fn encoding(&self) -> Encoding {
        self.encoding
    }
    /// HL7 grammar edition used for parsing and encoding.
    #[must_use]
    pub fn edition(&self) -> Edition {
        self.edition
    }
    /// Segment ending accepted on this link.
    #[must_use]
    pub fn terminator(&self) -> Terminator {
        self.terminator
    }
    /// Policy for malformed HL7 escapes; original bytes remain available.
    #[must_use]
    pub fn escapes(&self) -> Escapes {
        self.escapes
    }
    /// Whether this dialect explicitly permits UTF-8 under HL7 2.3.1.
    #[must_use]
    pub fn allow_utf8_in_v231(&self) -> bool {
        self.allow_utf8_in_v231
    }
    /// Size and count ceilings applied before parsing untrusted records.
    #[must_use]
    pub fn limits(&self) -> &SyntaxLimits {
        &self.limits
    }
}

impl Hl7Query {
    /// Protocol behavior selected by configuration and checked at load time.
    #[must_use]
    pub fn policy(&self) -> &OrderQuery {
        &self.policy
    }
    /// Record field positions used to read this configured dialect.
    #[must_use]
    pub fn layout(&self) -> &Hl7OrderQuery {
        &self.layout
    }
}

impl Hl7SendOrders {
    /// Protocol-specific settings checked for internal consistency.
    #[must_use]
    pub fn settings(&self) -> &Hl7Send {
        &self.settings
    }
    /// Assigning system required for retained placer order identities.
    #[must_use]
    pub fn placer_namespace(&self) -> &Identifier {
        &self.placer_namespace
    }
}

impl Hl7Orders {
    /// Assigning system for patient identities from this connection.
    #[must_use]
    pub fn patient_namespace(&self) -> Option<&Identifier> {
        self.patient_namespace.as_ref()
    }
    /// Configured code tables used without guessing unknown values.
    #[must_use]
    pub fn tables(&self) -> &OrderTables {
        &self.tables
    }
    /// Host-query settings validated for this protocol.
    #[must_use]
    pub fn query(&self) -> Option<&Hl7Query> {
        self.query.as_ref()
    }
    /// Validated outgoing command settings.
    #[must_use]
    pub fn send(&self) -> Option<&Hl7SendOrders> {
        self.send.as_ref()
    }
}

impl Hl7Stack {
    /// Validated order policy used before encoding commands.
    #[must_use]
    pub fn orders(&self) -> Option<&Hl7Orders> {
        self.orders.as_ref()
    }
}

pub(super) fn build(
    machine: &ParsedMachine<'_>,
    timeouts: DialogTimeouts,
    path: &str,
) -> Result<Hl7Stack, Error> {
    Ok(Hl7Stack {
        settings: super::StackSettings {
            transport: tcp(&machine.transport, path)?,
            frame_limit: machine.framing.limit,
            timeouts,
            syntax: hl7_syntax(machine, path)?,
            codes: machine
                .mappings
                .clone()
                .ok_or_else(|| invalid(path, "mappings", "record stack requires mappings"))?,
        },
        orders: machine
            .orders
            .as_ref()
            .map(|o| hl7_orders(o, machine, path))
            .transpose()?,
    })
}

impl Hl7Stack {
    /// Borrows the shared validated settings without changing protocol-specific types.
    #[must_use]
    pub fn settings(&self) -> &super::StackSettings<TcpTransport, Hl7Syntax, Mappings> {
        &self.settings
    }
}
