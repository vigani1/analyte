//! Turns a validated machine document into a `Stack`: one typed settings bundle per supported protocol combination.
#[cfg(test)]
use crate::MachineDocument;
use crate::{
    AstmOrderFields, Autolyser, ChecksumHex, CodeTables, Edition, Encoding, Error, Escapes,
    Hl7OrderQuery, Hl7Send, LawConnectionMode, LawPeer, LawQueryForm, LawWork, MachineDeparture,
    Mappings, OrderPriority, OrderQuery, Orders, OutboundSpecimen, OutboundTest, Poct, Priority,
    Responsibility, Serial, Syntax, Terminator, TestMapping, Transport, TransportKind, UnitMapping,
    Vidas, invalid,
};
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
mod transport;
pub use transport::{RecordTransport, StreamKind, StreamTransport, TcpRole, TcpTransport};
use transport::{stream, stream_kind, stream_timeout, tcp, tcp_kind};
mod limits;
use limits::limits;
pub use limits::{DialogTimeouts, SyntaxLimits};
mod order_tables;
pub use order_tables::OrderTables;
use order_tables::order_tables;
mod capabilities;
pub use capabilities::Hl7OptionsSource;
pub(crate) use capabilities::RecordKind;
mod settings;
pub use settings::{StackSettings, SyntaxBounds};

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
        self.common().limits
    }

    /// Returns host dialog deadlines, including file handoff budgets.
    #[must_use]
    pub fn timeouts(&self) -> &DialogTimeouts {
        self.common().timeouts
    }

    /// Returns the required test and unit tables for this stack; there is no absent-table fallback.
    #[must_use]
    pub fn code_tables(&self) -> (&[TestMapping], &[UnitMapping]) {
        self.common().codes
    }
}

impl Stack {
    /// Returns the selected physical transport kind for binding and evidence metadata.
    #[must_use]
    pub fn transport_kind(&self) -> TransportKind {
        self.common().transport
    }

    /// Returns the configured external-operation deadline for the selected transport.
    #[must_use]
    pub fn operation_timeout_ms(&self) -> u32 {
        self.common().operation_timeout_ms
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
        let choice = self.choice();
        let [_, framing, dialog, syntax] = choice.layer_names();
        [transport, framing, dialog, syntax]
    }
}

impl Stack {
    /// Returns field selectors only for record-based stacks; vendor code tables have no selectors.
    #[must_use]
    pub fn field_mappings(&self) -> Option<&Mappings> {
        self.common().fields
    }
}

struct CommonView<'a> {
    limits: &'a SyntaxLimits,
    timeouts: &'a DialogTimeouts,
    codes: (&'a [TestMapping], &'a [UnitMapping]),
    transport: TransportKind,
    operation_timeout_ms: u32,
    fields: Option<&'a Mappings>,
}

impl Stack {
    fn common(&self) -> CommonView<'_> {
        macro_rules! view {
            ($s:expr, $kind:expr, $timeout:expr, $fields:expr) => {{
                let settings = &$s.settings;
                CommonView {
                    limits: settings.limits(),
                    timeouts: settings.timeouts(),
                    codes: (&settings.codes().tests, &settings.codes().units),
                    transport: $kind,
                    operation_timeout_ms: $timeout,
                    fields: $fields,
                }
            }};
        }
        match self {
            Self::Astm(RecordTransport::Stream(s)) => view!(
                s,
                stream_kind(s.settings.transport()),
                stream_timeout(s.settings.transport()),
                Some(s.settings.codes())
            ),
            Self::Hl7(RecordTransport::Stream(s)) => view!(
                s,
                tcp_kind(s.settings.transport().role()),
                s.settings.transport().operation_timeout_ms(),
                Some(s.settings.codes())
            ),
            Self::Astm(RecordTransport::Folder(s)) => view!(
                s,
                TransportKind::WatchedFolder,
                *s.settings.transport(),
                Some(s.settings.codes())
            ),
            Self::Hl7(RecordTransport::Folder(s)) => view!(
                s,
                TransportKind::WatchedFolder,
                *s.settings.transport(),
                Some(s.settings.codes())
            ),
            Self::Law(s) => view!(
                s,
                tcp_kind(s.settings.transport().role()),
                s.settings.transport().operation_timeout_ms(),
                Some(s.settings.codes())
            ),
            Self::Poct(s) => view!(
                s,
                tcp_kind(s.settings.transport().role()),
                s.settings.transport().operation_timeout_ms(),
                None
            ),
            Self::Vidas(s) => view!(
                s,
                stream_kind(s.settings.transport()),
                stream_timeout(s.settings.transport()),
                None
            ),
            Self::Autolyser(s) => view!(
                s,
                stream_kind(s.settings.transport()),
                stream_timeout(s.settings.transport()),
                None
            ),
        }
    }
}

fn code_tables(machine: &ParsedMachine<'_>, path: &str) -> Result<CodeTables, Error> {
    machine
        .codes
        .clone()
        .ok_or_else(|| invalid(path, "codes", "vendor stack requires code tables"))
}
