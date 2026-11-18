//! Holds raw TOML configuration types.
pub(crate) mod law;
pub use law::{Law, LawConnectionMode, LawPeer, LawQueryForm, LawWork, PeerSupport};
mod vidas;
pub use vidas::{AckPoint, ChecksumAcceptance, ChecksumInclusion, HexCase, Settings as Vidas};

use analyte_model::Identifier;
use serde::{Deserialize, Serialize};
mod mappings;
pub use mappings::{
    ComponentParts, Field, IdentityEvidence, Mappings, NoSubId, OrderNamespace, QcRule,
    SampleSelection, SubId, TestMapping, UnitMapping,
};
mod poct;
pub use poct::Poct;
mod autolyser;
pub use autolyser::Autolyser;
mod departures;
pub use departures::{
    DepartureDeclaration, DepartureDirection, DepartureEdition, DepartureGuarantee,
    DepartureMessage, DepartureReference, DepartureScope, MachineDeparture,
};
mod refusal;
pub use refusal::DepartureRefusal;

/// Declared evidence; loading never promotes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceLevel {
    /// Built from documents only; no simulator or hardware evidence claimed.
    Documented,
    /// Backed by simulator or vendor-trace evidence, without a hardware claim.
    Simulated,
    /// Backed by recorded conversations with the real instrument.
    Proven,
}

/// Physical connection role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportKind {
    /// Opens a host-owned serial port.
    Serial,
    /// Connects to the configured peer TCP endpoint.
    TcpClient,
    /// Accepts inbound TCP connections on the configured listener.
    TcpServer,
    /// Receives files from a host-owned watched directory.
    WatchedFolder,
}

/// Serial parity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Parity {
    /// Disables this serial line option.
    None,
    /// Requires an odd parity bit count.
    Odd,
    /// Requires an even parity bit count.
    Even,
}

/// Serial flow control.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowControl {
    /// Disables this serial line option.
    None,
    /// Uses hardware handshake lines for flow control.
    Hardware,
    /// Uses XON/XOFF bytes for flow control.
    Software,
}

impl From<Parity> for analyte_transport::serial::Parity {
    fn from(parity: Parity) -> Self {
        match parity {
            Parity::None => Self::None,
            Parity::Odd => Self::Odd,
            Parity::Even => Self::Even,
        }
    }
}

impl From<FlowControl> for analyte_transport::serial::FlowControl {
    fn from(flow: FlowControl) -> Self {
        match flow {
            FlowControl::None => Self::None,
            FlowControl::Hardware => Self::Hardware,
            FlowControl::Software => Self::Software,
        }
    }
}

/// LIS01 checksum compatibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChecksumHex {
    /// Accepts only the standard uppercase checksum spelling.
    Strict,
    /// Also accepts lowercase checksum hex digits.
    AcceptLowercase,
}

/// Side that wins simultaneous ENQ.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    /// Gives the instrument priority during simultaneous ENQ.
    Instrument,
    /// Gives the host priority during simultaneous ENQ.
    Host,
}

/// HL7 acknowledgment promise, ADR 0003.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Responsibility {
    /// Promises durable custody before acknowledging the transfer.
    StoreAndForward,
}

/// Strict text decoding; invalid bytes must be retained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    /// Rejects text bytes outside ASCII.
    Ascii,
    /// Decodes each byte using ISO-8859-1.
    Latin1,
    /// Requires well-formed UTF-8 text.
    Utf8,
}

/// Negotiated segment ending.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Terminator {
    /// Ends segments with carriage return.
    Cr,
    /// Ends segments with carriage return followed by line feed.
    CrLf,
    /// Ends segments with line feed.
    Lf,
}

/// Malformed escape handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Escapes {
    /// Refuses malformed escape sequences while retaining their bytes.
    Reject,
    /// Keeps malformed escape text for explicit downstream handling.
    Preserve,
}

/// A config spelling of the protocol-owned HL7 edition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Edition(analyte_hl7::Version);

impl TryFrom<String> for Edition {
    type Error = String;
    fn try_from(key: String) -> Result<Self, Self::Error> {
        analyte_hl7::Version::from_config_key(&key)
            .map(Self)
            .ok_or_else(|| {
                format!(
                    "unknown variant `{key}`, expected one of {}",
                    analyte_hl7::Version::config_keys()
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })
    }
}
impl From<analyte_hl7::Version> for Edition {
    fn from(version: analyte_hl7::Version) -> Self {
        Self(version)
    }
}
impl From<Edition> for String {
    fn from(edition: Edition) -> Self {
        edition.0.config_key().into()
    }
}
impl From<Edition> for analyte_hl7::Version {
    fn from(edition: Edition) -> Self {
        edition.0
    }
}

/// How the machine obtained the sample identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SampleSource {
    /// Sample identity comes from the instrument barcode reader.
    Barcode,
    /// Sample identity comes from the supplied host order.
    HostOrder,
    /// Sample identity was entered by an operator.
    Operator,
    /// Makes no claim about the source or specimen role.
    Unknown,
}

/// Field ownership; selections never search globally.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Group {
    /// Applies the configured rule to the whole message.
    Message,
    /// Applies the configured patient scope or patient specimen role.
    Patient,
    /// Applies the configured rule to an order group.
    Order,
    /// Applies the configured rule to one observation.
    Observation,
}

/// Classification evidence, never a QC decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Classification {
    /// Applies the configured patient scope or patient specimen role.
    Patient,
    /// Identifies a control specimen without making a QC decision.
    Control,
    /// Identifies a calibration specimen without interpreting it.
    Calibration,
    /// Makes no claim about the source or specimen role.
    Unknown,
}

/// Serial settings; endpoint belongs to the site.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Serial {
    /// Bits per second.
    pub baud: u32,
    /// Data bits, five through eight.
    pub data_bits: u8,
    /// Parity check.
    pub parity: Parity,
    /// One or two stop bits.
    pub stop_bits: u8,
    /// Flow control.
    pub flow_control: FlowControl,
}

/// Draft connection settings, checked before activation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Transport {
    /// Connection role.
    pub kind: TransportKind,
    /// Required only for serial.
    pub serial: Option<Serial>,
    /// Deadline for external operations.
    pub operation_timeout_ms: u32,
}

/// Explicit wire framing bounds.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Framing {
    /// Frame bytes including ASTM overhead, or payload bytes otherwise.
    pub limit: usize,
    /// Required only for LIS01.
    pub checksum_hex: Option<ChecksumHex>,
}

/// Dialog settings consumed by the host state machine.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dialog {
    /// Receive deadline.
    pub receive_timeout_ms: u32,
    /// Send deadline.
    pub send_timeout_ms: u32,
    /// Required only for ASTM.
    pub contention_priority: Option<Priority>,
    /// Required only for HL7.
    pub acknowledgment: Option<Responsibility>,
    /// Optional 2.9.1 capability request; true is refused until a separate channel is supported.
    pub enhanced_application_acknowledgments: Option<bool>,
}

/// Syntax options checked against the selected protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Syntax {
    /// Negotiated encoding.
    pub encoding: Encoding,
    /// Required only for HL7.
    pub edition: Option<Edition>,
    /// Required only for HL7.
    pub terminator: Option<Terminator>,
    /// Required only for HL7.
    pub escapes: Option<Escapes>,
    /// Named vendor departure for the later UTF-8 code in HL7 2.3.1.
    #[serde(default)]
    pub allow_utf8_in_v231: bool,
    /// Total message bytes.
    pub bytes: usize,
    /// Maximum records or segments.
    pub records: usize,
    /// Maximum ASTM record bytes.
    pub record_bytes: usize,
    /// Maximum fields.
    pub fields: usize,
    /// Maximum repetitions.
    pub repetitions: usize,
    /// Maximum ASTM components.
    pub components: usize,
}

/// Evidence applies only to these exercised capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capability {
    /// Analyzer model.
    pub model: Identifier,
    /// Firmware or explicit unknown.
    pub firmware: Identifier,
    /// Driver version.
    pub driver: Identifier,
    /// Digest of the tested config, or untested.
    pub config_digest: Identifier,
    /// Exercised transport.
    pub transport: TransportKind,
    /// Tested dialogs.
    pub dialogs: Vec<Identifier>,
    /// Exercised failures.
    pub failure_cases: Vec<Identifier>,
    /// Platform or untested.
    pub platform: Identifier,
    /// Evidence file references.
    pub evidence_files: Vec<Identifier>,
}

/// Declared summary and its scope, ADR 0005.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Evidence {
    /// Declared level.
    pub level: EvidenceLevel,
    /// Capability evidence.
    pub capabilities: Vec<Capability>,
}

/// Shipped definition contains protocol and mapping rules only.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MachineDocument {
    /// Protocol selection, independent of physical transport.
    #[serde(deserialize_with = "crate::selection::protocol")]
    pub protocol: String,
    /// Required only for the LAW Analyzer Manager stack.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law: Option<Law>,
    /// Explicit outbound behavior; omission disables queries and work lists.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orders: Option<crate::Orders>,
    /// Must be one.
    pub schema_version: u32,
    /// Definition identity.
    pub name: Identifier,
    /// Transport settings, without endpoint.
    pub transport: Transport,
    /// Framing options.
    pub framing: Framing,
    /// Dialog settings.
    pub dialog: Dialog,
    /// Syntax settings.
    pub syntax: Syntax,
    /// Field and code selection.
    pub mappings: Option<Mappings>,
    /// Code tables for vendor syntaxes, without field selectors.
    pub codes: Option<CodeTables>,
    /// Required POCT reviewer policy.
    pub poct: Option<Poct>,
    /// Required installed VIDAS settings.
    pub vidas: Option<Vidas>,
    /// Required Autolyser polling policy.
    pub autolyser: Option<Autolyser>,
    /// Named vendor departures.
    pub deviations: Vec<MachineDeparture>,
    /// Installed applicability, required when departures are declared.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub departure_scope: Option<DepartureScope>,
    /// Declared evidence.
    pub evidence: Evidence,
}

/// References credentials by name; stores no secret material.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Destination {
    /// Maximum trusted order-cache age in seconds, shared by every host path.
    #[serde(default = "default_order_freshness")]
    pub freshness_seconds: u64,
    /// Deadline in seconds for one whole lab-app HTTP request (a result delivery
    /// or one order page): connect, TLS, upload, the app storing and answering,
    /// and the body read. Validated as 1 through 120 seconds.
    #[serde(default = "default_request_timeout")]
    pub request_timeout_seconds: u64,
    /// Destination identity.
    pub name: Identifier,
    /// HTTPS base URL without user info, query or fragment.
    pub base_url: crate::HttpsUrl,
    /// Host credential reference.
    pub api_key_name: Identifier,
    /// Host signing credential reference.
    pub signing_key_name: Identifier,
}

/// Installation-specific values kept apart from protocol rules.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Site {
    /// Explicit retention in days (1 through 3650), or `keep` (ADR 0013 item 1).
    pub retention: crate::Retention,
    /// Must be one.
    pub schema_version: u32,
    /// Port, numeric socket address, or absolute folder path.
    pub endpoint: String,
    /// Selected lab app.
    pub destination: Destination,
    /// The lab's time zone: an IANA name or a fixed `+HH:MM` offset. Only a LAW
    /// MSH-7 sent without an offset is read in it, with a note; absent keeps
    /// refusing such a header.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_zone: Option<crate::SiteTimeZone>,
}

/// Explicit replacements; omitted sections retain their preceding layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Overrides {
    /// Explicit replacement of LAW peer declarations and query policy.
    pub law: Option<Law>,
    /// Explicit replacement of outbound behavior.
    pub orders: Option<crate::Orders>,
    /// Must be one.
    pub schema_version: u32,
    /// Replacement transport settings.
    pub transport: Option<Transport>,
    /// Replacement envelope.
    pub framing: Option<Framing>,
    /// Replacement dialog.
    pub dialog: Option<Dialog>,
    /// Replacement syntax.
    pub syntax: Option<Syntax>,
    /// Replacement mapping table.
    pub mappings: Option<Mappings>,
    /// Replacement deviation declarations.
    pub deviations: Option<Vec<MachineDeparture>>,
    /// Explicit installed applicability replacement.
    pub departure_scope: Option<DepartureScope>,
    /// Replacement site endpoint.
    pub endpoint: Option<String>,
    /// Replacement destination.
    pub destination: Option<Destination>,
}

/// Shared exact test and unit translations for vendor protocols.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodeTables {
    /// Source system and code to application code.
    pub tests: Vec<TestMapping>,
    /// Original unit to explicit UCUM spelling.
    pub units: Vec<UnitMapping>,
}

/// Installed VIDAS sample ID ceiling; only documented layouts are representable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub enum VidasSampleLimit {
    /// Twelve-character installed layout.
    Twelve,
    /// Seventeen-character installed layout.
    Seventeen,
}

impl TryFrom<u8> for VidasSampleLimit {
    type Error = &'static str;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            12 => Ok(Self::Twelve),
            17 => Ok(Self::Seventeen),
            _ => Err("VIDAS sample_limit must be 12 or 17"),
        }
    }
}

impl From<VidasSampleLimit> for u8 {
    fn from(value: VidasSampleLimit) -> Self {
        match value {
            VidasSampleLimit::Twelve => 12,
            VidasSampleLimit::Seventeen => 17,
        }
    }
}

/// Exact source codes for dilution application; all other values stay unknown.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DilutionSource {
    /// Group-relative source field.
    pub field: Field,
    /// Exact already-applied code.
    pub yes: Identifier,
    /// Exact not-applied code.
    pub no: Identifier,
}

/// Expected source value interpretation, without clinical interpretation.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueKind {
    /// Require an exact numeric interpretation.
    Numeric,
    /// Require qualitative text or a coded interpretation.
    Coded,
    /// Preserve existing protocol interpretation.
    #[default]
    Either,
}

fn default_order_freshness() -> u64 {
    30
}

// Covers TLS setup over a slow WAN plus a few seconds of app storage, while a
// hung order poll still fails and retries well inside the default 30-second
// freshness, so one stalled page does not make the order cache untrusted.
fn default_request_timeout() -> u64 {
    10
}

/// Installed POCT identity evidence, never inferred from values or receipt IDs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PoctIdentity {
    /// Require the device sequence in addition to device, observation time and analyte.
    pub sequence_number: bool,
    /// Explicit installed processing mode; no source mode is inferred from Hello.
    pub processing: analyte_model::ProcessingMode,
}
