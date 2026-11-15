//! Protocol combinations constructed only after semantic validation.
use crate::*;
use analyte_model::Identifier;

/// A supported, validated protocol combination. Each payload has only its own settings.
/// This runtime representation is not serialized; canonical TOML keeps the original layout.
/// Boxed payloads keep the enum small despite the different-sized mapping and policy tables.
#[derive(Debug, Clone)]
pub enum Stack {
    /// LIS02 records over a LIS01 serial or TCP link.
    Astm(Box<AstmStack>),
    /// Generic HL7 v2 receiver over MLLP/TCP.
    Hl7(Box<Hl7Stack>),
    /// IHE LAW Analyzer Manager over one persistent MLLP/TCP link.
    Law(Box<LawStack>),
    /// POCT1-A reviewer over TCP.
    Poct(Box<PoctStack>),
    /// VIDAS Literal over serial or TCP.
    VidasLiteral(Box<VidasStack>),
    /// Autolyser polling over serial or TCP.
    Autolyser(Box<AutolyserStack>),
    /// Watched-folder LIS02 records, without a wire dialog.
    FileAstm(Box<FileAstmStack>),
    /// Watched-folder HL7 records, without a wire dialog.
    FileHl7(Box<FileHl7Stack>),
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
    pub role: TcpRole,
    /// Deadline for external operations.
    pub operation_timeout_ms: u32,
}

/// Receive and send deadlines shared by the host dialogs.
#[derive(Debug, Clone)]
pub struct DialogTimeouts {
    /// Receive deadline.
    pub receive_timeout_ms: u32,
    /// Send deadline.
    pub send_timeout_ms: u32,
}

/// Explicit syntax resource ceilings retained from the config.
#[derive(Debug, Clone)]
pub struct SyntaxLimits {
    /// Total message bytes.
    pub bytes: usize,
    /// Maximum records or segments.
    pub records: usize,
    /// Maximum record bytes.
    pub record_bytes: usize,
    /// Maximum fields.
    pub fields: usize,
    /// Maximum repetitions.
    pub repetitions: usize,
    /// Maximum components.
    pub components: usize,
}

/// LIS02 syntax settings, without HL7-only options.
#[derive(Debug, Clone)]
pub struct AstmSyntax {
    /// Strict input encoding.
    pub encoding: Encoding,
    /// Resource ceilings.
    pub limits: SyntaxLimits,
}

/// HL7 syntax settings with every required option present.
#[derive(Debug, Clone)]
pub struct Hl7Syntax {
    /// Strict input encoding.
    pub encoding: Encoding,
    /// Negotiated HL7 edition.
    pub edition: Edition,
    /// Negotiated segment ending.
    pub terminator: Terminator,
    /// Malformed escape policy.
    pub escapes: Escapes,
    /// Resource ceilings.
    pub limits: SyntaxLimits,
}

/// Outbound field-based translations shared by ASTM and HL7.
#[derive(Debug, Clone)]
pub struct OrderTables {
    /// Exact outbound test translations.
    pub tests: Vec<OutboundTest>,
    /// Explicit specimen translations; absence maps no values.
    pub specimen_types: Option<Vec<OutboundSpecimen>>,
    /// Exact outbound priority translations.
    pub priorities: Vec<OrderPriority>,
}

/// Optional ASTM query policy and required O-record layout.
#[derive(Debug, Clone)]
pub struct AstmOrders {
    /// Outbound translations.
    pub tables: OrderTables,
    /// Explicit query capability, if enabled.
    pub query: Option<OrderQuery>,
    /// Explicit work-list permission.
    pub send_ahead: bool,
    /// Required ASTM destinations.
    pub layout: AstmOrderFields,
}

/// An enabled HL7 query capability always carries its layout.
#[derive(Debug, Clone)]
pub struct Hl7Query {
    /// Deadline and untrusted-cache policy.
    pub policy: OrderQuery,
    /// Required legacy query layout.
    pub layout: Hl7OrderQuery,
}

/// An enabled unsolicited HL7 capability always has its namespace.
#[derive(Debug, Clone)]
pub struct Hl7SendOrders {
    /// Required header and specimen layout.
    pub settings: Hl7Send,
    /// Required assigning namespace.
    pub placer_namespace: Identifier,
}

/// Independent, explicitly enabled HL7 query and send capabilities.
#[derive(Debug, Clone)]
pub struct Hl7Orders {
    /// Outbound translations.
    pub tables: OrderTables,
    /// Query capability; omission disables it.
    pub query: Option<Hl7Query>,
    /// Send-ahead capability; omission disables it.
    pub send: Option<Hl7SendOrders>,
}

/// Persistent LAW link policy; work is carried by the enabled order capability.
#[derive(Debug, Clone)]
pub struct LawPolicy {
    /// Supported lookup scopes.
    pub query_forms: Vec<LawQueryForm>,
    /// Explicit peer capabilities.
    pub peer: LawPeer,
    /// Query storage, lookup and reservation deadline.
    pub query_deadline_ms: u32,
    /// Application response deadline.
    pub response_ms: u32,
}

/// Enabled LAW work always has a layout, namespace and specimen mappings.
#[derive(Debug, Clone)]
pub struct LawOrders {
    /// Required LAW work layout.
    pub work: LawWork,
    /// Exact outbound test translations.
    pub tests: Vec<OutboundTest>,
    /// Required specimen translations.
    pub specimen_types: Vec<OutboundSpecimen>,
    /// Exact priority translations.
    pub priorities: Vec<OrderPriority>,
    /// Required assigning namespace.
    pub placer_namespace: Identifier,
    /// Permission for unsolicited work.
    pub send_ahead: bool,
}

/// VIDAS work lists carry only supported outbound settings.
#[derive(Debug, Clone)]
pub struct VidasOrders {
    /// Exact assay translations.
    pub tests: Vec<OutboundTest>,
    /// Explicit accepted app priorities.
    pub priorities: Vec<OrderPriority>,
    /// Required patient assigning namespace.
    pub patient_namespace: Identifier,
    /// Explicit download permission.
    pub send_ahead: bool,
}

/// Autolyser work lists carry installed clone behavior and patient namespace.
#[derive(Debug, Clone)]
pub struct AutolyserOrders {
    /// Outbound translations.
    pub tables: OrderTables,
    /// Required patient assigning namespace.
    pub patient_namespace: Identifier,
    /// Explicit clone behavior, including false.
    pub clone: bool,
    /// Explicit download permission.
    pub send_ahead: bool,
}

/// LIS01/LIS02 settings; no independently selectable layer kinds.
#[derive(Debug, Clone)]
pub struct AstmStack {
    /// Serial or TCP link.
    pub transport: StreamTransport,
    /// Maximum frame bytes including overhead.
    pub frame_limit: usize,
    /// Required LIS01 checksum policy.
    pub checksum_hex: ChecksumHex,
    /// Host receive and send deadlines.
    pub timeouts: DialogTimeouts,
    /// Required simultaneous-ENQ policy.
    pub contention_priority: Priority,
    /// LIS02 settings.
    pub syntax: AstmSyntax,
    /// Required field selectors and code tables.
    pub mappings: Mappings,
    /// Explicit outbound capability; omission disables it.
    pub orders: Option<AstmOrders>,
}

/// Generic HL7 over MLLP/TCP.
#[derive(Debug, Clone)]
pub struct Hl7Stack {
    /// TCP link.
    pub transport: TcpTransport,
    /// Maximum MLLP payload bytes.
    pub frame_limit: usize,
    /// Host receive and send deadlines.
    pub timeouts: DialogTimeouts,
    /// Required acknowledgment promise.
    pub acknowledgment: Responsibility,
    /// Complete HL7 settings.
    pub syntax: Hl7Syntax,
    /// Required field selectors and code tables.
    pub mappings: Mappings,
    /// Explicit outbound capability; omission disables it.
    pub orders: Option<Hl7Orders>,
}

/// LAW over one persistent MLLP/TCP link.
#[derive(Debug, Clone)]
pub struct LawStack {
    /// TCP link.
    pub transport: TcpTransport,
    /// Maximum MLLP payload bytes.
    pub frame_limit: usize,
    /// Host receive and send deadlines.
    pub timeouts: DialogTimeouts,
    /// Required acknowledgment promise.
    pub acknowledgment: Responsibility,
    /// LAW fixes HL7 2.5.1, UTF-8, CR endings and strict escapes.
    pub limits: SyntaxLimits,
    /// Required field selectors and code tables.
    pub mappings: Mappings,
    /// Required Analyzer Manager policy.
    pub policy: LawPolicy,
    /// Explicit outbound capability; omission disables it.
    pub orders: Option<LawOrders>,
}

/// Poct stack with its required installed policy.
#[derive(Debug, Clone)]
pub struct PoctStack {
    /// Supported connection settings.
    pub transport: TcpTransport,
    /// Maximum framed payload bytes.
    pub frame_limit: usize,
    /// Host receive and send deadlines.
    pub timeouts: DialogTimeouts,
    /// Syntax resource ceilings.
    pub limits: SyntaxLimits,
    /// Required exact test and unit translations.
    pub codes: CodeTables,
    /// Required installed protocol policy.
    pub policy: Poct,
}

/// Vidas stack with its required installed policy.
#[derive(Debug, Clone)]
pub struct VidasStack {
    /// Supported connection settings.
    pub transport: StreamTransport,
    /// Maximum framed payload bytes.
    pub frame_limit: usize,
    /// Host receive and send deadlines.
    pub timeouts: DialogTimeouts,
    /// Syntax resource ceilings.
    pub limits: SyntaxLimits,
    /// Required exact test and unit translations.
    pub codes: CodeTables,
    /// Required installed protocol policy.
    pub policy: Vidas,
    /// Explicit outbound capability; omission disables it.
    pub orders: Option<VidasOrders>,
}

/// Autolyser stack with its required installed policy.
#[derive(Debug, Clone)]
pub struct AutolyserStack {
    /// Supported connection settings.
    pub transport: StreamTransport,
    /// Maximum framed payload bytes.
    pub frame_limit: usize,
    /// Host receive and send deadlines.
    pub timeouts: DialogTimeouts,
    /// Syntax resource ceilings.
    pub limits: SyntaxLimits,
    /// Required exact test and unit translations.
    pub codes: CodeTables,
    /// Required installed protocol policy.
    pub policy: Autolyser,
    /// Explicit outbound capability; omission disables it.
    pub orders: Option<AutolyserOrders>,
}

/// Watched-folder records without any stream framing or acknowledgment policy.
#[derive(Debug, Clone)]
pub struct FileAstmStack {
    /// External operation deadline.
    pub operation_timeout_ms: u32,
    /// Configured file payload ceiling.
    pub frame_limit: usize,
    /// Host receive and send deadlines.
    pub timeouts: DialogTimeouts,
    /// Record syntax settings.
    pub syntax: AstmSyntax,
    /// Required field selectors and code tables.
    pub mappings: Mappings,
    /// Retained configured outbound policy; file pickup performs no machine sends.
    pub orders: Option<AstmOrders>,
}

/// Watched-folder records without any stream framing or acknowledgment policy.
#[derive(Debug, Clone)]
pub struct FileHl7Stack {
    /// External operation deadline.
    pub operation_timeout_ms: u32,
    /// Configured file payload ceiling.
    pub frame_limit: usize,
    /// Host receive and send deadlines.
    pub timeouts: DialogTimeouts,
    /// Record syntax settings.
    pub syntax: Hl7Syntax,
    /// Required field selectors and code tables.
    pub mappings: Mappings,
    /// Retained configured outbound policy; file pickup performs no machine sends.
    pub orders: Option<Hl7Orders>,
}

impl Stack {
    // This is called only after both original and effective layers pass validation.
    #[expect(
        clippy::expect_used,
        reason = "private conversion runs only after validate_machine proves required sections and options"
    )]
    pub(crate) fn from_validated(machine: &Machine) -> Self {
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
            DialogKind::AstmLink => Self::Astm(Box::new(AstmStack {
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
            })),
            DialogKind::Hl7Receiver => Self::Hl7(Box::new(Hl7Stack {
                transport: tcp(&machine.transport),
                frame_limit,
                timeouts,
                acknowledgment: acknowledgment(),
                syntax: hl7_syntax(&machine.syntax),
                mappings: mappings(),
                orders: machine.orders.as_ref().map(hl7_orders),
            })),
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
                SyntaxKind::AstmLis02 => Self::FileAstm(Box::new(FileAstmStack {
                    operation_timeout_ms: machine.transport.operation_timeout_ms,
                    frame_limit,
                    timeouts,
                    syntax: astm_syntax(&machine.syntax),
                    mappings: mappings(),
                    orders: machine.orders.as_ref().map(astm_orders),
                })),
                SyntaxKind::Hl7V2 => Self::FileHl7(Box::new(FileHl7Stack {
                    operation_timeout_ms: machine.transport.operation_timeout_ms,
                    frame_limit,
                    timeouts,
                    syntax: hl7_syntax(&machine.syntax),
                    mappings: mappings(),
                    orders: machine.orders.as_ref().map(hl7_orders),
                })),
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
