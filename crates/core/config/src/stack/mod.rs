//! Turns a validated machine document into a `Stack`: one typed settings bundle per supported protocol combination.
#[cfg(test)]
use crate::MachineDocument;
use crate::{
    AstmOrderFields, Autolyser, ChecksumHex, CodeTables, Edition, Encoding, Error, Escapes,
    Hl7OrderQuery, Hl7Send, LawConnectionMode, LawPeer, LawQueryForm, LawWork, MachineDeparture,
    Mappings, OrderPriority, OrderQuery, Orders, Poct, Priority, Responsibility, Serial, Syntax,
    Terminator, Transport, TransportKind, UnitMapping, Vidas, invalid,
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
    /// IHE LAW Analyzer Manager over a persistent MLLP/TCP link or independent directional links.
    Law(Box<LawStack>),
    /// POCT1-A reviewer over TCP.
    Poct(Box<PoctStack>),
    /// VIDAS Literal over serial or TCP.
    Vidas(Box<VidasStack>),
    /// Autolyser polling over serial or TCP.
    Autolyser(Box<AutolyserStack>),
    /// Record only (ADR 0034): one protocol's link rules from its shipped
    /// capture profile; no mappings, no orders and no lab app.
    Record(Box<crate::RecordStack>),
}

/// The serial or TCP link of a record-only stack.
pub(crate) fn record_transport(
    transport: &Transport,
    path: &str,
) -> Result<StreamTransport, Error> {
    stream(transport, path)
}

impl Stack {
    #[cfg(test)]
    pub(crate) fn parse(document: &MachineDocument, path: &str) -> Result<Self, Error> {
        let machine = ParsedMachine::parse(document, path)?;
        let astm_frame_limit = crate::validation::validate_dialog_and_framing(&machine, path)?;
        Self::from_parsed(&machine, astm_frame_limit, path)
    }

    // Conversion returns required settings as types, never relying on validation to prevent a panic.
    pub(crate) fn from_parsed(
        machine: &ParsedMachine<'_>,
        astm_frame_limit: Option<analyte_framing::astm::Limit>,
        path: &str,
    ) -> Result<Self, Error> {
        let timeouts = DialogTimeouts {
            receive_timeout_ms: machine.dialog.receive_timeout_ms,
            send_timeout_ms: machine.dialog.send_timeout_ms,
        };
        Ok(match machine.choice() {
            StackChoice::AstmLis02 => Self::Astm(RecordTransport::Stream(Box::new(astm::build(
                machine,
                timeouts,
                astm_frame_limit.ok_or_else(|| {
                    invalid(
                        path,
                        "framing.limit",
                        "LIS01 requires a validated frame limit",
                    )
                })?,
                path,
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

    /// Returns the machine's test code system and unit table; neither has a
    /// fallback. A record-only stack reads no result, so it has none.
    #[must_use]
    pub fn code_tables(&self) -> Option<(&analyte_model::Identifier, &[UnitMapping])> {
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

struct CommonView<'a> {
    limits: &'a SyntaxLimits,
    timeouts: &'a DialogTimeouts,
    codes: Option<(&'a analyte_model::Identifier, &'a [UnitMapping])>,
    transport: TransportKind,
    operation_timeout_ms: u32,
}

impl Stack {
    fn common(&self) -> CommonView<'_> {
        macro_rules! view {
            ($s:expr, $kind:expr, $timeout:expr) => {{
                let settings = &$s.settings;
                CommonView {
                    limits: settings.limits(),
                    timeouts: settings.timeouts(),
                    codes: Some((&settings.codes().test_system, &settings.codes().units)),
                    transport: $kind,
                    operation_timeout_ms: $timeout,
                }
            }};
        }
        match self {
            Self::Astm(RecordTransport::Stream(s)) => view!(
                s,
                stream_kind(s.settings.transport()),
                stream_timeout(s.settings.transport())
            ),
            Self::Hl7(RecordTransport::Stream(s)) => view!(
                s,
                tcp_kind(s.settings.transport().role()),
                s.settings.transport().operation_timeout_ms()
            ),
            Self::Astm(RecordTransport::Folder(s)) => view!(
                s,
                TransportKind::WatchedFolder,
                *s.settings.transport()
            ),
            Self::Hl7(RecordTransport::Folder(s)) => view!(
                s,
                TransportKind::WatchedFolder,
                *s.settings.transport()
            ),
            Self::Law(s) => view!(
                s,
                tcp_kind(s.settings.transport().role()),
                s.settings.transport().operation_timeout_ms()
            ),
            Self::Poct(s) => view!(
                s,
                tcp_kind(s.settings.transport().role()),
                s.settings.transport().operation_timeout_ms()
            ),
            Self::Vidas(s) => view!(
                s,
                stream_kind(s.settings.transport()),
                stream_timeout(s.settings.transport())
            ),
            Self::Autolyser(s) => view!(
                s,
                stream_kind(s.settings.transport()),
                stream_timeout(s.settings.transport())
            ),
            Self::Record(s) => CommonView {
                limits: s.limits(),
                timeouts: s.timeouts(),
                codes: None,
                transport: stream_kind(s.transport()),
                operation_timeout_ms: stream_timeout(s.transport()),
            },
        }
    }
}

fn code_tables(machine: &ParsedMachine<'_>, path: &str) -> Result<CodeTables, Error> {
    machine
        .codes
        .clone()
        .ok_or_else(|| invalid(path, "codes", "vendor stack requires code tables"))
}

/// The machine's own test and specimen code systems and the priority
/// translations shared by every order-sending protocol.
#[derive(Debug, Clone)]
pub struct OrderTables {
    /// The machine's own test code system; order tests in another are refused.
    test_system: analyte_model::Identifier,
    /// The machine's own specimen code system; absent when it takes none.
    specimen_system: Option<analyte_model::Identifier>,
    /// Exact outbound priority translations.
    priorities: Vec<OrderPriority>,
}

fn order_tables(
    orders: &Orders,
    machine: &ParsedMachine<'_>,
    path: &str,
) -> Result<OrderTables, Error> {
    let test_system = match (&machine.mappings, &machine.codes) {
        (Some(mappings), _) => &mappings.test_system,
        (None, Some(codes)) => &codes.test_system,
        (None, None) => {
            return Err(invalid(
                path,
                "mappings",
                "orders need the machine's test_system",
            ));
        }
    };
    Ok(OrderTables {
        test_system: test_system.clone(),
        specimen_system: orders.specimen_system.clone(),
        priorities: orders.priorities.clone(),
    })
}

impl OrderTables {
    /// The machine's own test code system (ADR 0029). An order test is sent
    /// only when its namespace is exactly this system; its code goes out as written.
    #[must_use]
    pub fn test_system(&self) -> &analyte_model::Identifier {
        &self.test_system
    }
    /// The machine's own specimen code system (ADR 0030). An order's specimen
    /// type is sent only when its namespace is exactly this system; its code
    /// goes out as written. `None`: this machine takes no specimen type.
    #[must_use]
    pub fn specimen_system(&self) -> Option<&analyte_model::Identifier> {
        self.specimen_system.as_ref()
    }
    /// Explicit order-priority mapping.
    #[must_use]
    pub fn priorities(&self) -> &[OrderPriority] {
        &self.priorities
    }
}
