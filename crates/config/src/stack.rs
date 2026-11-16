//! Validated protocol selection, shared settings and dispatch at the config boundary.
use crate::*;
mod astm;
pub use astm::{AstmOrders, AstmStack, AstmSyntax};
mod hl7;
pub use hl7::{Hl7Orders, Hl7Query, Hl7SendOrders, Hl7Stack, Hl7Syntax};
mod law;
pub use law::{LawLinks, LawOrders, LawPolicy, LawStack};
mod poct;
pub use poct::PoctStack;
mod vidas;
pub use vidas::{VidasOrders, VidasStack};
mod autolyser;
pub use autolyser::{AutolyserOrders, AutolyserStack};
mod file;
use astm::{astm_orders, astm_syntax};
use file::file_stack;
pub use file::{FileAstmStack, FileHl7Stack};
use hl7::{hl7_orders, hl7_syntax};
use law::law_stack;

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
pub struct StreamTransport {
    kind: StreamKind,
    operation_timeout_ms: u32,
}

/// Read-only physical stream selection; the owning transport validates its settings.
#[derive(Debug, Clone)]
pub enum StreamKind {
    /// Installed serial settings.
    Serial(Serial),
    /// TCP connection direction.
    Tcp(TcpRole),
}

impl StreamTransport {
    /// Borrows the installed physical stream selection.
    #[must_use]
    pub fn kind(&self) -> &StreamKind {
        &self.kind
    }
    /// Returns the validated, nonzero external-operation deadline.
    #[must_use]
    pub fn operation_timeout_ms(&self) -> u32 {
        self.operation_timeout_ms
    }
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
/// Cloning validated settings never grants write access.
/// ```compile_fail
/// fn invalidate(settings: &analyte_config::DialogTimeouts) {
///     let mut settings = settings.clone();
///     settings.receive_timeout_ms = 0;
/// }
/// ```
#[derive(Debug, Clone)]
pub struct DialogTimeouts {
    /// Receive deadline.
    receive_timeout_ms: u32,
    /// Send deadline.
    send_timeout_ms: u32,
}

/// Explicit syntax resource ceilings retained from the config.
/// ```compile_fail
/// fn invalidate(limits: &analyte_config::SyntaxLimits) {
///     let mut limits = limits.clone();
///     limits.bytes = 0;
/// }
/// ```
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
        match machine.dialog.kind {
            DialogKind::AstmLink => Self::Astm(RecordTransport::Stream(Box::new(astm::build(
                machine, timeouts,
            )))),
            DialogKind::Hl7Receiver => Self::Hl7(RecordTransport::Stream(Box::new(hl7::build(
                machine, timeouts,
            )))),
            DialogKind::LawManager => Self::Law(Box::new(law_stack(machine, timeouts))),
            DialogKind::Poct => Self::Poct(Box::new(poct::build(machine, timeouts))),
            DialogKind::VidasLiteral => {
                Self::VidasLiteral(Box::new(vidas::build(machine, timeouts)))
            }
            DialogKind::Autolyser => Self::Autolyser(Box::new(autolyser::build(machine, timeouts))),
            DialogKind::FilePickup => file_stack(machine, timeouts),
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
    let kind = match transport.kind {
        TransportKind::Serial => StreamKind::Serial(
            transport
                .serial
                .clone()
                .expect("validated serial transport requires settings"),
        ),
        TransportKind::TcpClient | TransportKind::TcpServer => StreamKind::Tcp(tcp(transport).role),
        TransportKind::WatchedFolder => {
            unreachable!("validated stream stack excludes watched folders")
        }
    };
    StreamTransport {
        kind,
        operation_timeout_ms: transport.operation_timeout_ms,
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

fn order_tables(orders: &Orders) -> OrderTables {
    OrderTables {
        tests: orders.tests.clone(),
        specimen_types: orders.specimen_types.clone(),
        priorities: orders.priorities.clone(),
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
    match transport.kind() {
        StreamKind::Serial(_) => TransportKind::Serial,
        StreamKind::Tcp(role) => tcp_kind(*role),
    }
}

fn stream_timeout(transport: &StreamTransport) -> u32 {
    transport.operation_timeout_ms()
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

/// Transport-specific settings for one record protocol. Folder pickup has no wire dialog.
#[derive(Debug, Clone)]
pub enum RecordTransport<S, F> {
    /// Serial or TCP framing and conversation settings.
    Stream(S),
    /// Complete-file handoff settings, without wire acknowledgments.
    Folder(F),
}

#[expect(
    clippy::expect_used,
    reason = "private conversion runs only after validate_machine proves required sections and options"
)]
fn code_tables(machine: &MachineDocument) -> CodeTables {
    machine
        .codes
        .clone()
        .expect("validated vendor stack requires code tables")
}
