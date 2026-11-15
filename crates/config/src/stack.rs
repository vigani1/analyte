//! Protocol combinations constructed only after semantic validation.
use crate::*;
use analyte_model::Identifier;

/// A supported, validated protocol combination. Each payload has only its own settings.
/// This runtime representation is not serialized; canonical TOML keeps the original layout.
/// Boxed payloads keep the enum small despite the different-sized mapping and policy tables.
#[derive(Debug, Clone)]
pub enum Stack {
    /// LIS02 records over a LIS01 serial or TCP link.
    Astm(RecordTransport<Box<AstmStack>, Box<FileAstmStack>>),
    /// Generic HL7 v2 receiver over MLLP/TCP.
    Hl7(RecordTransport<Box<Hl7Stack>, Box<FileHl7Stack>>),
    /// IHE LAW Analyzer Manager over one persistent MLLP/TCP link.
    Law(Box<LawStack>),
    /// POCT1-A reviewer over TCP.
    Poct(Box<PoctStack>),
    /// VIDAS Literal over serial or TCP.
    VidasLiteral(Box<VidasStack>),
    /// Autolyser polling over serial or TCP.
    Autolyser(Box<AutolyserStack>),
}

/// TCP connection direction; cannot select serial or folder transport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TcpRole {
    /// Connect to the configured endpoint.
    Client,
    /// Listen on the configured endpoint.
    Server,
}

/// Transport for a stack supporting either serial or TCP, never a watched folder.
#[derive(Debug, Clone)]
pub enum StreamTransport {
    /// Serial transport always has installed serial settings.
    Serial {
        /// Installed serial parameters.
        settings: Serial,
        /// Deadline for external operations.
        operation_timeout_ms: u32,
    },
    /// TCP transport has a direction and no serial parameters.
    Tcp {
        /// Connection direction.
        role: TcpRole,
        /// Deadline for external operations.
        operation_timeout_ms: u32,
    },
}

/// Validated TCP settings without serial or folder alternatives.
#[derive(Debug, Clone)]
pub struct TcpTransport {
    /// Connection direction.
    role: TcpRole,
    /// Deadline for external operations.
    operation_timeout_ms: u32,
}

/// Receive and send deadlines shared by the host dialogs.
#[derive(Debug, Clone)]
pub struct DialogTimeouts {
    /// Receive deadline.
    receive_timeout_ms: u32,
    /// Send deadline.
    send_timeout_ms: u32,
}

/// Explicit syntax resource ceilings retained from the config.
#[derive(Debug, Clone)]
pub struct SyntaxLimits {
    /// Total message bytes.
    bytes: usize,
    /// Maximum records or segments.
    records: usize,
    /// Maximum record bytes.
    record_bytes: usize,
    /// Maximum fields.
    fields: usize,
    /// Maximum repetitions.
    repetitions: usize,
    /// Maximum components.
    components: usize,
}

/// LIS02 syntax settings, without HL7-only options.
#[derive(Debug, Clone)]
pub struct AstmSyntax {
    /// Strict input encoding.
    encoding: Encoding,
    /// Resource ceilings.
    limits: SyntaxLimits,
}

/// HL7 syntax settings with every required option present.
#[derive(Debug, Clone)]
pub struct Hl7Syntax {
    /// Strict input encoding.
    encoding: Encoding,
    /// Negotiated HL7 edition.
    edition: Edition,
    /// Negotiated segment ending.
    terminator: Terminator,
    /// Malformed escape policy.
    escapes: Escapes,
    /// Validated named vendor departure for UTF-8 in HL7 2.3.1.
    allow_utf8_in_v231: bool,
    /// Resource ceilings.
    limits: SyntaxLimits,
}

/// Outbound field-based translations shared by ASTM and HL7.
#[derive(Debug, Clone)]
pub struct OrderTables {
    /// Exact outbound test translations.
    tests: Vec<OutboundTest>,
    /// Explicit specimen translations; absence maps no values.
    specimen_types: Option<Vec<OutboundSpecimen>>,
    /// Exact outbound priority translations.
    priorities: Vec<OrderPriority>,
}

/// Optional ASTM query policy and required O-record layout.
#[derive(Debug, Clone)]
pub struct AstmOrders {
    /// Outbound translations.
    tables: OrderTables,
    /// Explicit query capability, if enabled.
    query: Option<OrderQuery>,
    /// Explicit work-list permission.
    send_ahead: bool,
    /// Required ASTM destinations.
    layout: AstmOrderFields,
}

/// An enabled HL7 query capability always carries its layout.
#[derive(Debug, Clone)]
pub struct Hl7Query {
    /// Deadline and untrusted-cache policy.
    policy: OrderQuery,
    /// Required legacy query layout.
    layout: Hl7OrderQuery,
}

/// An enabled unsolicited HL7 capability always has its namespace.
#[derive(Debug, Clone)]
pub struct Hl7SendOrders {
    /// Required header and specimen layout.
    settings: Hl7Send,
    /// Required assigning namespace.
    placer_namespace: Identifier,
}

/// Independent, explicitly enabled HL7 query and send capabilities.
#[derive(Debug, Clone)]
pub struct Hl7Orders {
    /// Optional assigning namespace for explicitly supplied patient identities.
    patient_namespace: Option<Identifier>,
    /// Outbound translations.
    tables: OrderTables,
    /// Query capability; omission disables it.
    query: Option<Hl7Query>,
    /// Send-ahead capability; omission disables it.
    send: Option<Hl7SendOrders>,
}

/// Validated LAW connection ownership, with no absent independent destination.
#[derive(Debug, Clone, Copy)]
pub enum LawLinks {
    /// All transactions share the configured persistent link.
    Persistent,
    /// Separate inbound query/result links and a host-owned outbound work destination.
    Independent {
        /// Analyzer's LAB-28 listener.
        outbound: std::net::SocketAddr,
    },
}

/// LAW link policy; work is carried by the enabled order capability.
#[derive(Debug, Clone)]
pub struct LawPolicy {
    /// Validated ownership and destination.
    links: LawLinks,
    /// Supported lookup scopes.
    query_forms: Vec<LawQueryForm>,
    /// Explicit peer capabilities.
    peer: LawPeer,
    /// Query storage, lookup and reservation deadline.
    query_deadline_ms: u32,
    /// Application response deadline.
    response_ms: u32,
}

/// Enabled LAW work always has a layout, namespace and specimen mappings.
#[derive(Debug, Clone)]
pub struct LawOrders {
    /// Required LAW work layout.
    work: LawWork,
    /// Exact outbound test translations.
    tests: Vec<OutboundTest>,
    /// Required specimen translations.
    specimen_types: Vec<OutboundSpecimen>,
    /// Exact priority translations.
    priorities: Vec<OrderPriority>,
    /// Required assigning namespace.
    placer_namespace: Identifier,
    /// Permission for unsolicited work.
    send_ahead: bool,
}

/// VIDAS work lists carry only supported outbound settings.
#[derive(Debug, Clone)]
pub struct VidasOrders {
    /// Exact assay translations.
    tests: Vec<OutboundTest>,
    /// Explicit accepted app priorities.
    priorities: Vec<OrderPriority>,
    /// Required patient assigning namespace.
    patient_namespace: Identifier,
    /// Explicit download permission.
    send_ahead: bool,
}

/// Autolyser work lists carry installed clone behavior and patient namespace.
#[derive(Debug, Clone)]
pub struct AutolyserOrders {
    /// Outbound translations.
    tables: OrderTables,
    /// Required patient assigning namespace.
    patient_namespace: Identifier,
    /// Explicit clone behavior, including false.
    clone: bool,
    /// Explicit download permission.
    send_ahead: bool,
}

/// LIS01/LIS02 settings; no independently selectable layer kinds.
#[derive(Debug, Clone)]
pub struct AstmStack {
    /// Serial or TCP link.
    transport: StreamTransport,
    /// Maximum frame bytes including overhead.
    frame_limit: usize,
    /// Required LIS01 checksum policy.
    checksum_hex: ChecksumHex,
    /// Host receive and send deadlines.
    timeouts: DialogTimeouts,
    /// Required simultaneous-ENQ policy.
    contention_priority: Priority,
    /// LIS02 settings.
    syntax: AstmSyntax,
    /// Required field selectors and code tables.
    mappings: Mappings,
    /// Explicit outbound capability; omission disables it.
    orders: Option<AstmOrders>,
}

/// Generic HL7 over MLLP/TCP.
#[derive(Debug, Clone)]
pub struct Hl7Stack {
    /// TCP link.
    transport: TcpTransport,
    /// Maximum MLLP payload bytes.
    frame_limit: usize,
    /// Host receive and send deadlines.
    timeouts: DialogTimeouts,
    /// Required acknowledgment promise.
    acknowledgment: Responsibility,
    /// Complete HL7 settings.
    syntax: Hl7Syntax,
    /// Required field selectors and code tables.
    mappings: Mappings,
    /// Explicit outbound capability; omission disables it.
    orders: Option<Hl7Orders>,
}

/// LAW over MLLP/TCP with explicit connection ownership.
#[derive(Debug, Clone)]
pub struct LawStack {
    /// TCP link.
    transport: TcpTransport,
    /// Maximum MLLP payload bytes.
    frame_limit: usize,
    /// Host receive and send deadlines.
    timeouts: DialogTimeouts,
    /// Required acknowledgment promise.
    acknowledgment: Responsibility,
    /// LAW fixes HL7 2.5.1, UTF-8, CR endings and strict escapes.
    limits: SyntaxLimits,
    /// Required field selectors and code tables.
    mappings: Mappings,
    /// Required Analyzer Manager policy.
    policy: LawPolicy,
    /// Explicit outbound capability; omission disables it.
    orders: Option<LawOrders>,
}

/// Poct stack with its required installed policy.
#[derive(Debug, Clone)]
pub struct PoctStack {
    /// Supported connection settings.
    transport: TcpTransport,
    /// Maximum framed payload bytes.
    frame_limit: usize,
    /// Host receive and send deadlines.
    timeouts: DialogTimeouts,
    /// Syntax resource ceilings.
    limits: SyntaxLimits,
    /// Required exact test and unit translations.
    codes: CodeTables,
    /// Required installed protocol policy.
    policy: Poct,
}

/// Vidas stack with its required installed policy.
#[derive(Debug, Clone)]
pub struct VidasStack {
    /// Supported connection settings.
    transport: StreamTransport,
    /// Maximum framed payload bytes.
    frame_limit: usize,
    /// Host receive and send deadlines.
    timeouts: DialogTimeouts,
    /// Syntax resource ceilings.
    limits: SyntaxLimits,
    /// Required exact test and unit translations.
    codes: CodeTables,
    /// Required installed protocol policy.
    policy: Vidas,
    /// Explicit outbound capability; omission disables it.
    orders: Option<VidasOrders>,
}

/// Autolyser stack with its required installed policy.
#[derive(Debug, Clone)]
pub struct AutolyserStack {
    /// Supported connection settings.
    transport: StreamTransport,
    /// Maximum framed payload bytes.
    frame_limit: usize,
    /// Host receive and send deadlines.
    timeouts: DialogTimeouts,
    /// Syntax resource ceilings.
    limits: SyntaxLimits,
    /// Required exact test and unit translations.
    codes: CodeTables,
    /// Required installed protocol policy.
    policy: Autolyser,
    /// Explicit outbound capability; omission disables it.
    orders: Option<AutolyserOrders>,
}

/// Watched-folder records without any stream framing or acknowledgment policy.
#[derive(Debug, Clone)]
pub struct FileAstmStack {
    /// External operation deadline.
    operation_timeout_ms: u32,
    /// Configured file payload ceiling.
    frame_limit: usize,
    /// Host receive and send deadlines.
    timeouts: DialogTimeouts,
    /// Record syntax settings.
    syntax: AstmSyntax,
    /// Required field selectors and code tables.
    mappings: Mappings,
    /// Retained configured outbound policy; file pickup performs no machine sends.
    orders: Option<AstmOrders>,
}

/// Watched-folder records without any stream framing or acknowledgment policy.
#[derive(Debug, Clone)]
pub struct FileHl7Stack {
    /// External operation deadline.
    operation_timeout_ms: u32,
    /// Configured file payload ceiling.
    frame_limit: usize,
    /// Host receive and send deadlines.
    timeouts: DialogTimeouts,
    /// Record syntax settings.
    syntax: Hl7Syntax,
    /// Required field selectors and code tables.
    mappings: Mappings,
    /// Retained configured outbound policy; file pickup performs no machine sends.
    orders: Option<Hl7Orders>,
}

impl Stack {
    // This is called only after both original and effective layers pass validation.
    #[expect(
        clippy::expect_used,
        reason = "private conversion runs only after validate_machine proves required sections and options"
    )]
    pub(crate) fn from_validated(machine: &MachineDocument) -> Self {
        let timeouts = DialogTimeouts {
            receive_timeout_ms: machine.dialog.receive_timeout_ms,
            send_timeout_ms: machine.dialog.send_timeout_ms,
        };
        let frame_limit = machine.framing.limit;
        let mappings = || {
            machine
                .mappings
                .clone()
                .expect("validated record stack requires mappings")
        };
        let codes = || {
            machine
                .codes
                .clone()
                .expect("validated vendor stack requires code tables")
        };
        let acknowledgment = || {
            machine
                .dialog
                .acknowledgment
                .expect("validated MLLP dialog requires acknowledgment policy")
        };
        match machine.dialog.kind {
            DialogKind::AstmLink => Self::Astm(RecordTransport::Stream(Box::new(AstmStack {
                transport: stream(&machine.transport),
                frame_limit,
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
                mappings: mappings(),
                orders: machine.orders.as_ref().map(astm_orders),
            }))),
            DialogKind::Hl7Receiver => Self::Hl7(RecordTransport::Stream(Box::new(Hl7Stack {
                transport: tcp(&machine.transport),
                frame_limit,
                timeouts,
                acknowledgment: acknowledgment(),
                syntax: hl7_syntax(&machine.syntax),
                mappings: mappings(),
                orders: machine.orders.as_ref().map(hl7_orders),
            }))),
            DialogKind::LawManager => {
                let law = machine
                    .law
                    .as_ref()
                    .expect("validated LAW stack requires policy");
                Self::Law(Box::new(LawStack {
                    transport: tcp(&machine.transport),
                    frame_limit,
                    timeouts,
                    acknowledgment: acknowledgment(),
                    limits: limits(&machine.syntax),
                    mappings: mappings(),
                    policy: LawPolicy {
                        links: match law.connection_mode {
                            LawConnectionMode::Persistent => LawLinks::Persistent,
                            LawConnectionMode::IndependentPerTransaction => LawLinks::Independent {
                                outbound: law
                                    .outbound_endpoint
                                    .expect("validated independent LAW has an outbound endpoint"),
                            },
                        },
                        query_forms: law.query_forms.clone(),
                        peer: law.peer.clone(),
                        query_deadline_ms: law.query_deadline_ms,
                        response_ms: law.response_ms,
                    },
                    orders: machine.orders.as_ref().map(|orders| LawOrders {
                        work: law
                            .work
                            .clone()
                            .expect("validated LAW orders require work layout"),
                        tests: orders.tests.clone(),
                        priorities: orders.priorities.clone(),
                        specimen_types: orders
                            .specimen_types
                            .clone()
                            .expect("validated LAW orders require specimen mappings"),
                        placer_namespace: orders
                            .placer_namespace
                            .clone()
                            .expect("validated LAW orders require placer namespace"),
                        send_ahead: orders.send_ahead,
                    }),
                }))
            }
            DialogKind::Poct => Self::Poct(Box::new(PoctStack {
                transport: tcp(&machine.transport),
                frame_limit,
                timeouts,
                limits: limits(&machine.syntax),
                codes: codes(),
                policy: machine
                    .poct
                    .clone()
                    .expect("validated POCT stack requires policy"),
            })),
            DialogKind::VidasLiteral => Self::VidasLiteral(Box::new(VidasStack {
                transport: stream(&machine.transport),
                frame_limit,
                timeouts,
                limits: limits(&machine.syntax),
                codes: codes(),
                policy: machine
                    .vidas
                    .clone()
                    .expect("validated VIDAS stack requires policy"),
                orders: machine.orders.as_ref().map(|orders| VidasOrders {
                    tests: orders.tests.clone(),
                    priorities: orders.priorities.clone(),
                    patient_namespace: orders
                        .patient_namespace
                        .clone()
                        .expect("validated VIDAS orders require patient namespace"),
                    send_ahead: orders.send_ahead,
                }),
            })),
            DialogKind::Autolyser => Self::Autolyser(Box::new(AutolyserStack {
                transport: stream(&machine.transport),
                frame_limit,
                timeouts,
                limits: limits(&machine.syntax),
                codes: codes(),
                policy: machine
                    .autolyser
                    .clone()
                    .expect("validated Autolyser stack requires policy"),
                orders: machine.orders.as_ref().map(|orders| AutolyserOrders {
                    tables: order_tables(orders),
                    patient_namespace: orders
                        .patient_namespace
                        .clone()
                        .expect("validated Autolyser orders require patient namespace"),
                    clone: orders
                        .clone
                        .expect("validated Autolyser orders require clone behavior"),
                    send_ahead: orders.send_ahead,
                }),
            })),
            DialogKind::FilePickup => match machine.syntax.kind {
                SyntaxKind::AstmLis02 => {
                    Self::Astm(RecordTransport::Folder(Box::new(FileAstmStack {
                        operation_timeout_ms: machine.transport.operation_timeout_ms,
                        frame_limit,
                        timeouts,
                        syntax: astm_syntax(&machine.syntax),
                        mappings: mappings(),
                        orders: machine.orders.as_ref().map(astm_orders),
                    })))
                }
                SyntaxKind::Hl7V2 => Self::Hl7(RecordTransport::Folder(Box::new(FileHl7Stack {
                    operation_timeout_ms: machine.transport.operation_timeout_ms,
                    frame_limit,
                    timeouts,
                    syntax: hl7_syntax(&machine.syntax),
                    mappings: mappings(),
                    orders: machine.orders.as_ref().map(hl7_orders),
                }))),
                SyntaxKind::Poct | SyntaxKind::VidasLiteral | SyntaxKind::Autolyser => {
                    unreachable!("validated file pickup supports only LIS02 or HL7")
                }
            },
        }
    }
}

fn tcp(transport: &Transport) -> TcpTransport {
    let role = match transport.kind {
        TransportKind::TcpClient => TcpRole::Client,
        TransportKind::TcpServer => TcpRole::Server,
        TransportKind::Serial | TransportKind::WatchedFolder => {
            unreachable!("validated TCP stack requires TCP transport")
        }
    };
    TcpTransport {
        role,
        operation_timeout_ms: transport.operation_timeout_ms,
    }
}

#[expect(
    clippy::expect_used,
    reason = "private conversion runs only after validate_machine proves required sections and options"
)]
fn stream(transport: &Transport) -> StreamTransport {
    match transport.kind {
        TransportKind::Serial => StreamTransport::Serial {
            settings: transport
                .serial
                .clone()
                .expect("validated serial transport requires settings"),
            operation_timeout_ms: transport.operation_timeout_ms,
        },
        TransportKind::TcpClient | TransportKind::TcpServer => {
            let settings = tcp(transport);
            StreamTransport::Tcp {
                role: settings.role,
                operation_timeout_ms: settings.operation_timeout_ms,
            }
        }
        TransportKind::WatchedFolder => {
            unreachable!("validated stream stack excludes watched folders")
        }
    }
}

fn limits(syntax: &Syntax) -> SyntaxLimits {
    SyntaxLimits {
        bytes: syntax.bytes,
        records: syntax.records,
        record_bytes: syntax.record_bytes,
        fields: syntax.fields,
        repetitions: syntax.repetitions,
        components: syntax.components,
    }
}

fn astm_syntax(syntax: &Syntax) -> AstmSyntax {
    AstmSyntax {
        encoding: syntax.encoding,
        limits: limits(syntax),
    }
}

#[expect(
    clippy::expect_used,
    reason = "private conversion runs only after validate_machine proves required sections and options"
)]
fn hl7_syntax(syntax: &Syntax) -> Hl7Syntax {
    Hl7Syntax {
        allow_utf8_in_v231: syntax.allow_utf8_in_v231,
        encoding: syntax.encoding,
        edition: syntax
            .edition
            .expect("validated HL7 syntax requires edition"),
        terminator: syntax
            .terminator
            .expect("validated HL7 syntax requires terminator"),
        escapes: syntax
            .escapes
            .expect("validated HL7 syntax requires escape policy"),
        limits: limits(syntax),
    }
}

fn order_tables(orders: &Orders) -> OrderTables {
    OrderTables {
        tests: orders.tests.clone(),
        specimen_types: orders.specimen_types.clone(),
        priorities: orders.priorities.clone(),
    }
}

#[expect(
    clippy::expect_used,
    reason = "private conversion runs only after validate_machine proves required sections and options"
)]
fn astm_orders(orders: &Orders) -> AstmOrders {
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

#[expect(
    clippy::expect_used,
    reason = "private conversion runs only after validate_machine proves required sections and options"
)]
fn hl7_orders(orders: &Orders) -> Hl7Orders {
    Hl7Orders {
        patient_namespace: orders.patient_namespace.clone(),
        tables: order_tables(orders),
        query: orders.query.as_ref().map(|policy| Hl7Query {
            policy: policy.clone(),
            layout: orders
                .hl7
                .clone()
                .expect("validated HL7 query requires layout"),
        }),
        send: orders.hl7_send.as_ref().map(|settings| Hl7SendOrders {
            settings: settings.clone(),
            placer_namespace: orders
                .placer_namespace
                .clone()
                .expect("validated HL7 send requires placer namespace"),
        }),
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

impl LawStack {
    /// Supplies LAW's fixed HL7 2.5.1 syntax policy and configured resource ceilings.
    #[must_use]
    pub fn options(&self) -> analyte_hl7::Options {
        hl7_options(
            Encoding::Utf8,
            Terminator::Cr,
            Escapes::Reject,
            &self.limits,
        )
    }
}

fn hl7_options(
    encoding: Encoding,
    terminator: Terminator,
    escapes: Escapes,
    limits: &SyntaxLimits,
) -> analyte_hl7::Options {
    analyte_hl7::Options {
        allow_utf8_in_v231: false,
        charset: match encoding {
            Encoding::Ascii => analyte_hl7::Charset::Ascii,
            Encoding::Latin1 => analyte_hl7::Charset::Latin1,
            Encoding::Utf8 => analyte_hl7::Charset::Utf8,
        },
        terminator: match terminator {
            Terminator::Cr => analyte_hl7::Terminator::Cr,
            Terminator::CrLf => analyte_hl7::Terminator::CrLf,
            Terminator::Lf => analyte_hl7::Terminator::Lf,
        },
        escapes: match escapes {
            Escapes::Reject => analyte_hl7::EscapePolicy::Reject,
            Escapes::Preserve => analyte_hl7::EscapePolicy::Preserve,
        },
        limits: analyte_hl7::Limits {
            message_bytes: limits.bytes,
            segments: limits.records,
            fields: limits.fields,
            repetitions: limits.repetitions,
        },
    }
}

impl Stack {
    /// Returns the selected stack's resource ceilings without reconstructing syntax kinds.
    #[must_use]
    pub fn limits(&self) -> &SyntaxLimits {
        match self {
            Self::Astm(RecordTransport::Stream(s)) => &s.syntax.limits,
            Self::Hl7(RecordTransport::Stream(s)) => &s.syntax.limits,
            Self::Astm(RecordTransport::Folder(s)) => &s.syntax.limits,
            Self::Hl7(RecordTransport::Folder(s)) => &s.syntax.limits,
            Self::Law(s) => &s.limits,
            Self::Poct(s) => &s.limits,
            Self::VidasLiteral(s) => &s.limits,
            Self::Autolyser(s) => &s.limits,
        }
    }

    /// Returns host dialog deadlines, including file handoff budgets.
    #[must_use]
    pub fn timeouts(&self) -> &DialogTimeouts {
        match self {
            Self::Astm(RecordTransport::Stream(s)) => &s.timeouts,
            Self::Hl7(RecordTransport::Stream(s)) => &s.timeouts,
            Self::Astm(RecordTransport::Folder(s)) => &s.timeouts,
            Self::Hl7(RecordTransport::Folder(s)) => &s.timeouts,
            Self::Law(s) => &s.timeouts,
            Self::Poct(s) => &s.timeouts,
            Self::VidasLiteral(s) => &s.timeouts,
            Self::Autolyser(s) => &s.timeouts,
        }
    }

    /// Returns the required test and unit tables for this stack; there is no absent-table fallback.
    #[must_use]
    pub fn code_tables(&self) -> (&[TestMapping], &[UnitMapping]) {
        match self {
            Self::Astm(RecordTransport::Stream(s)) => (&s.mappings.tests, &s.mappings.units),
            Self::Hl7(RecordTransport::Stream(s)) => (&s.mappings.tests, &s.mappings.units),
            Self::Astm(RecordTransport::Folder(s)) => (&s.mappings.tests, &s.mappings.units),
            Self::Hl7(RecordTransport::Folder(s)) => (&s.mappings.tests, &s.mappings.units),
            Self::Law(s) => (&s.mappings.tests, &s.mappings.units),
            Self::Poct(s) => (&s.codes.tests, &s.codes.units),
            Self::VidasLiteral(s) => (&s.codes.tests, &s.codes.units),
            Self::Autolyser(s) => (&s.codes.tests, &s.codes.units),
        }
    }
}

impl Stack {
    /// Returns the selected physical transport kind for binding and evidence metadata.
    #[must_use]
    pub fn transport_kind(&self) -> TransportKind {
        match self {
            Self::Astm(RecordTransport::Stream(s)) => stream_kind(&s.transport),
            Self::VidasLiteral(s) => stream_kind(&s.transport),
            Self::Autolyser(s) => stream_kind(&s.transport),
            Self::Hl7(RecordTransport::Stream(s)) => tcp_kind(s.transport.role),
            Self::Law(s) => tcp_kind(s.transport.role),
            Self::Poct(s) => tcp_kind(s.transport.role),
            Self::Astm(RecordTransport::Folder(_)) | Self::Hl7(RecordTransport::Folder(_)) => {
                TransportKind::WatchedFolder
            }
        }
    }

    /// Returns the configured external-operation deadline for the selected transport.
    #[must_use]
    pub fn operation_timeout_ms(&self) -> u32 {
        match self {
            Self::Astm(RecordTransport::Stream(s)) => stream_timeout(&s.transport),
            Self::VidasLiteral(s) => stream_timeout(&s.transport),
            Self::Autolyser(s) => stream_timeout(&s.transport),
            Self::Hl7(RecordTransport::Stream(s)) => s.transport.operation_timeout_ms,
            Self::Law(s) => s.transport.operation_timeout_ms,
            Self::Poct(s) => s.transport.operation_timeout_ms,
            Self::Astm(RecordTransport::Folder(s)) => s.operation_timeout_ms,
            Self::Hl7(RecordTransport::Folder(s)) => s.operation_timeout_ms,
        }
    }

    /// Returns stable layer names for recording headers; never selects runtime behavior.
    #[must_use]
    pub fn layer_names(&self) -> [&'static str; 4] {
        let transport = match self.transport_kind() {
            TransportKind::Serial => "serial",
            TransportKind::TcpClient => "tcp_client",
            TransportKind::TcpServer => "tcp_server",
            TransportKind::WatchedFolder => "watched_folder",
        };
        match self {
            Self::Astm(RecordTransport::Stream(_)) => {
                [transport, "astm_lis01", "astm_link", "astm_lis02"]
            }
            Self::Hl7(RecordTransport::Stream(_)) => [transport, "mllp", "hl7_receiver", "hl7_v2"],
            Self::Law(_) => [transport, "mllp", "law_manager", "hl7_v2"],
            Self::Poct(_) => [transport, "poct", "poct", "poct"],
            Self::VidasLiteral(_) => [transport, "vidas_literal", "vidas_literal", "vidas_literal"],
            Self::Autolyser(_) => [transport, "autolyser", "autolyser", "autolyser"],
            Self::Astm(RecordTransport::Folder(_)) => {
                [transport, "none", "file_pickup", "astm_lis02"]
            }
            Self::Hl7(RecordTransport::Folder(_)) => [transport, "none", "file_pickup", "hl7_v2"],
        }
    }
}

fn tcp_kind(role: TcpRole) -> TransportKind {
    match role {
        TcpRole::Client => TransportKind::TcpClient,
        TcpRole::Server => TransportKind::TcpServer,
    }
}
fn stream_kind(transport: &StreamTransport) -> TransportKind {
    match transport {
        StreamTransport::Serial { .. } => TransportKind::Serial,
        StreamTransport::Tcp { role, .. } => tcp_kind(*role),
    }
}
fn stream_timeout(transport: &StreamTransport) -> u32 {
    match transport {
        StreamTransport::Serial {
            operation_timeout_ms,
            ..
        }
        | StreamTransport::Tcp {
            operation_timeout_ms,
            ..
        } => *operation_timeout_ms,
    }
}

impl Stack {
    /// Returns field selectors only for record-based stacks; vendor code tables have no selectors.
    #[must_use]
    pub fn field_mappings(&self) -> Option<&Mappings> {
        match self {
            Self::Astm(RecordTransport::Stream(s)) => Some(&s.mappings),
            Self::Astm(RecordTransport::Folder(s)) => Some(&s.mappings),
            Self::Hl7(RecordTransport::Stream(s)) => Some(&s.mappings),
            Self::Hl7(RecordTransport::Folder(s)) => Some(&s.mappings),
            Self::Law(s) => Some(&s.mappings),
            Self::Poct(_) | Self::VidasLiteral(_) | Self::Autolyser(_) => None,
        }
    }
}

impl TcpTransport {
    /// Returns the validated role setting.
    #[must_use]
    pub fn role(&self) -> TcpRole {
        self.role
    }
    /// Returns the validated operation timeout ms setting.
    #[must_use]
    pub fn operation_timeout_ms(&self) -> u32 {
        self.operation_timeout_ms
    }
}
impl DialogTimeouts {
    /// Returns the validated receive timeout ms setting.
    #[must_use]
    pub fn receive_timeout_ms(&self) -> u32 {
        self.receive_timeout_ms
    }
    /// Returns the validated send timeout ms setting.
    #[must_use]
    pub fn send_timeout_ms(&self) -> u32 {
        self.send_timeout_ms
    }
}
impl SyntaxLimits {
    /// Returns the validated bytes setting.
    #[must_use]
    pub fn bytes(&self) -> usize {
        self.bytes
    }
    /// Returns the validated records setting.
    #[must_use]
    pub fn records(&self) -> usize {
        self.records
    }
    /// Returns the validated record bytes setting.
    #[must_use]
    pub fn record_bytes(&self) -> usize {
        self.record_bytes
    }
    /// Returns the validated fields setting.
    #[must_use]
    pub fn fields(&self) -> usize {
        self.fields
    }
    /// Returns the validated repetitions setting.
    #[must_use]
    pub fn repetitions(&self) -> usize {
        self.repetitions
    }
    /// Returns the validated components setting.
    #[must_use]
    pub fn components(&self) -> usize {
        self.components
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
impl OrderTables {
    /// Returns the validated tests setting.
    #[must_use]
    pub fn tests(&self) -> &Vec<OutboundTest> {
        &self.tests
    }
    /// Returns the validated specimen types setting.
    #[must_use]
    pub fn specimen_types(&self) -> &Option<Vec<OutboundSpecimen>> {
        &self.specimen_types
    }
    /// Returns the validated priorities setting.
    #[must_use]
    pub fn priorities(&self) -> &Vec<OrderPriority> {
        &self.priorities
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
impl LawPolicy {
    /// Returns the validated links setting.
    #[must_use]
    pub fn links(&self) -> LawLinks {
        self.links
    }
    /// Returns the validated query forms setting.
    #[must_use]
    pub fn query_forms(&self) -> &[LawQueryForm] {
        &self.query_forms
    }
    /// Returns the validated peer setting.
    #[must_use]
    pub fn peer(&self) -> &LawPeer {
        &self.peer
    }
    /// Returns the validated query deadline ms setting.
    #[must_use]
    pub fn query_deadline_ms(&self) -> u32 {
        self.query_deadline_ms
    }
    /// Returns the validated response ms setting.
    #[must_use]
    pub fn response_ms(&self) -> u32 {
        self.response_ms
    }
}
impl LawOrders {
    /// Returns the validated work setting.
    #[must_use]
    pub fn work(&self) -> &LawWork {
        &self.work
    }
    /// Returns the validated tests setting.
    #[must_use]
    pub fn tests(&self) -> &Vec<OutboundTest> {
        &self.tests
    }
    /// Returns the validated specimen types setting.
    #[must_use]
    pub fn specimen_types(&self) -> &Vec<OutboundSpecimen> {
        &self.specimen_types
    }
    /// Returns the validated priorities setting.
    #[must_use]
    pub fn priorities(&self) -> &Vec<OrderPriority> {
        &self.priorities
    }
    /// Returns the validated placer namespace setting.
    #[must_use]
    pub fn placer_namespace(&self) -> &Identifier {
        &self.placer_namespace
    }
    /// Returns the validated send ahead setting.
    #[must_use]
    pub fn send_ahead(&self) -> bool {
        self.send_ahead
    }
}
impl VidasOrders {
    /// Returns the validated tests setting.
    #[must_use]
    pub fn tests(&self) -> &Vec<OutboundTest> {
        &self.tests
    }
    /// Returns the validated priorities setting.
    #[must_use]
    pub fn priorities(&self) -> &Vec<OrderPriority> {
        &self.priorities
    }
    /// Returns the validated patient namespace setting.
    #[must_use]
    pub fn patient_namespace(&self) -> &Identifier {
        &self.patient_namespace
    }
    /// Returns the validated send ahead setting.
    #[must_use]
    pub fn send_ahead(&self) -> bool {
        self.send_ahead
    }
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
impl LawStack {
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
    /// Returns the validated limits setting.
    #[must_use]
    pub fn limits(&self) -> &SyntaxLimits {
        &self.limits
    }
    /// Returns the validated mappings setting.
    #[must_use]
    pub fn mappings(&self) -> &Mappings {
        &self.mappings
    }
    /// Returns the validated policy setting.
    #[must_use]
    pub fn policy(&self) -> &LawPolicy {
        &self.policy
    }
    /// Returns the validated orders setting.
    #[must_use]
    pub fn orders(&self) -> &Option<LawOrders> {
        &self.orders
    }
}
impl PoctStack {
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
    pub fn policy(&self) -> &Poct {
        &self.policy
    }
}
impl VidasStack {
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
    pub fn policy(&self) -> &Vidas {
        &self.policy
    }
    /// Returns the validated orders setting.
    #[must_use]
    pub fn orders(&self) -> &Option<VidasOrders> {
        &self.orders
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
impl FileAstmStack {
    /// Returns the validated operation timeout ms setting.
    #[must_use]
    pub fn operation_timeout_ms(&self) -> u32 {
        self.operation_timeout_ms
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
impl FileHl7Stack {
    /// Returns the validated operation timeout ms setting.
    #[must_use]
    pub fn operation_timeout_ms(&self) -> u32 {
        self.operation_timeout_ms
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

/// Transport-specific settings for one record protocol. Folder pickup has no wire dialog.
#[derive(Debug, Clone)]
pub enum RecordTransport<S, F> {
    /// Serial or TCP framing and conversation settings.
    Stream(S),
    /// Complete-file handoff settings, without wire acknowledgments.
    Folder(F),
}
