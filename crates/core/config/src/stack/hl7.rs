//! Constructs validated HL7 protocol settings.
use super::{
    DialogTimeouts, Edition, Encoding, Error, Escapes, Hl7OrderQuery, Hl7Send, Mappings,
    OrderQuery, OrderTables, Orders, Responsibility, Syntax, SyntaxLimits, TcpTransport,
    Terminator, hl7_options, invalid, limits, order_tables, tcp,
};
use crate::ParsedMachine;
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
    /// Shared link, resource and interpretation settings.
    pub(super) settings: super::StackSettings<TcpTransport, Hl7Syntax, Mappings>,
    /// Required acknowledgment promise.
    pub(super) acknowledgment: Responsibility,
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
    /// Acknowledgment responsibility promised only after durable custody.
    #[must_use]
    pub fn acknowledgment(&self) -> Responsibility {
        self.acknowledgment
    }
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
            syntax: hl7_syntax(&machine.syntax, path)?,
            codes: machine
                .mappings
                .clone()
                .ok_or_else(|| invalid(path, "mappings", "record stack requires mappings"))?,
        },
        acknowledgment: machine.dialog.acknowledgment.ok_or_else(|| {
            invalid(
                path,
                "dialog.acknowledgment",
                "MLLP dialog requires acknowledgment policy",
            )
        })?,
        orders: machine
            .orders
            .as_ref()
            .map(|o| hl7_orders(o, path))
            .transpose()?,
    })
}

impl Hl7Stack {
    /// Returns whether this selected protocol permits unsolicited work.
    #[must_use]
    pub fn send_ahead(&self) -> bool {
        self.orders().is_some_and(|orders| orders.send().is_some())
    }
}

impl Hl7Stack {
    /// Borrows the shared validated settings without changing protocol-specific types.
    #[must_use]
    pub fn settings(&self) -> &super::StackSettings<TcpTransport, Hl7Syntax, Mappings> {
        &self.settings
    }
}
