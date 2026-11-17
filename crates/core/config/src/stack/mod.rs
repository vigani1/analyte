//! Turns a validated machine document into a `Stack`: one typed settings bundle per supported protocol combination.
use crate::*;
use crate::{ParsedMachine, StackChoice};
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
    Vidas(Box<VidasStack>),
    /// Autolyser polling over serial or TCP.
    Autolyser(Box<AutolyserStack>),
}

impl Stack {
    #[cfg(test)]
    pub(crate) fn parse(document: &MachineDocument, path: &str) -> Result<Self, Error> {
        Self::from_parsed(&ParsedMachine::parse(document, path)?, path)
    }

    // Conversion returns required settings as types, never relying on validation to prevent a panic.
    pub(crate) fn from_parsed(machine: &ParsedMachine<'_>, path: &str) -> Result<Self, Error> {
        let timeouts = DialogTimeouts {
            receive_timeout_ms: machine.dialog.receive_timeout_ms,
            send_timeout_ms: machine.dialog.send_timeout_ms,
        };
        Ok(match machine.choice() {
            StackChoice::AstmLis02 => Self::Astm(RecordTransport::Stream(Box::new(astm::build(
                machine, timeouts, path,
            )?))),
            StackChoice::Hl7V2 => Self::Hl7(RecordTransport::Stream(Box::new(hl7::build(
                machine, timeouts, path,
            )?))),
            StackChoice::LawManager => Self::Law(Box::new(law_stack(machine, timeouts, path)?)),
            StackChoice::Poct => Self::Poct(Box::new(poct::build(machine, timeouts, path)?)),
            StackChoice::VidasLiteral => {
                Self::Vidas(Box::new(vidas::build(machine, timeouts, path)?))
            }
            StackChoice::Autolyser => {
                Self::Autolyser(Box::new(autolyser::build(machine, timeouts, path)?))
            }
            StackChoice::FileAstm | StackChoice::FileHl7 => file_stack(machine, timeouts, path)?,
        })
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
            Self::Vidas(s) => &s.limits,
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
            Self::Vidas(s) => &s.timeouts,
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
            Self::Vidas(s) => (&s.codes.tests, &s.codes.units),
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
            Self::Vidas(s) => stream_kind(&s.transport),
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
            Self::Vidas(s) => stream_timeout(&s.transport),
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
        let choice = match self {
            Self::Astm(RecordTransport::Stream(_)) => StackChoice::AstmLis02,
            Self::Hl7(RecordTransport::Stream(_)) => StackChoice::Hl7V2,
            Self::Law(_) => StackChoice::LawManager,
            Self::Poct(_) => StackChoice::Poct,
            Self::Vidas(_) => StackChoice::VidasLiteral,
            Self::Autolyser(_) => StackChoice::Autolyser,
            Self::Astm(RecordTransport::Folder(_)) => StackChoice::FileAstm,
            Self::Hl7(RecordTransport::Folder(_)) => StackChoice::FileHl7,
        };
        let [_, framing, dialog, syntax] = choice.layer_names();
        [transport, framing, dialog, syntax]
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
            Self::Poct(_) | Self::Vidas(_) | Self::Autolyser(_) => None,
        }
    }
}

fn code_tables(machine: &ParsedMachine<'_>, path: &str) -> Result<CodeTables, Error> {
    machine
        .codes
        .clone()
        .ok_or_else(|| invalid(path, "codes", "vendor stack requires code tables"))
}

mod transport;
pub use transport::{RecordTransport, StreamKind, StreamTransport, TcpRole, TcpTransport};
use transport::{stream, stream_kind, stream_timeout, tcp, tcp_kind};

mod limits;
pub use limits::{DialogTimeouts, SyntaxLimits};
use limits::{hl7_options, limits};

mod order_tables;
pub use order_tables::OrderTables;
use order_tables::order_tables;

mod capabilities;
