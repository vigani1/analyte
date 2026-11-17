//! Holds raw TOML configuration types.
pub(crate) mod law;
pub use law::*;
mod vidas;
pub use vidas::{AckPoint, ChecksumAcceptance, ChecksumInclusion, HexCase, Settings as Vidas};

use analyte_model::Identifier;
use serde::{Deserialize, Serialize};

/// Declared evidence; loading never promotes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceLevel {
    /// Explicit `Documented` selection.
    Documented,
    /// Explicit `Simulated` selection.
    Simulated,
    /// Explicit `Proven` selection.
    Proven,
}

/// Physical connection role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportKind {
    /// Explicit `Serial` selection.
    Serial,
    /// Explicit `TcpClient` selection.
    TcpClient,
    /// Explicit `TcpServer` selection.
    TcpServer,
    /// Explicit `WatchedFolder` selection.
    WatchedFolder,
}

/// Serial parity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Parity {
    /// Explicit `None` selection.
    None,
    /// Explicit `Odd` selection.
    Odd,
    /// Explicit `Even` selection.
    Even,
}

/// Serial flow control.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowControl {
    /// Explicit `None` selection.
    None,
    /// Explicit `Hardware` selection.
    Hardware,
    /// Explicit `Software` selection.
    Software,
}

/// LIS01 checksum compatibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChecksumHex {
    /// Explicit `Strict` selection.
    Strict,
    /// Explicit `AcceptLowercase` selection.
    AcceptLowercase,
}

/// Side that wins simultaneous ENQ.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    /// Explicit `Instrument` selection.
    Instrument,
    /// Explicit `Host` selection.
    Host,
}

/// HL7 acknowledgment promise, ADR 0003.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Responsibility {
    /// Explicit `StoreAndForward` selection.
    StoreAndForward,
}

/// Strict text decoding; invalid bytes must be retained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    /// Explicit `Ascii` selection.
    Ascii,
    /// Explicit `Latin1` selection.
    Latin1,
    /// Explicit `Utf8` selection.
    Utf8,
}

/// Negotiated segment ending.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Terminator {
    /// Explicit `Cr` selection.
    Cr,
    /// Explicit `CrLf` selection.
    CrLf,
    /// Explicit `Lf` selection.
    Lf,
}

/// Malformed escape handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Escapes {
    /// Explicit `Reject` selection.
    Reject,
    /// Explicit `Preserve` selection.
    Preserve,
}

/// A config spelling of the protocol-owned HL7 edition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Edition(analyte_hl7::Version);

impl Edition {
    /// Explicit HL7 23 selection.
    pub const V23: Self = Self(analyte_hl7::Version::V23);
    /// Explicit HL7 231 selection.
    pub const V231: Self = Self(analyte_hl7::Version::V231);
    /// Explicit HL7 24 selection.
    pub const V24: Self = Self(analyte_hl7::Version::V24);
    /// Explicit HL7 25 selection.
    pub const V25: Self = Self(analyte_hl7::Version::V25);
    /// Explicit HL7 251 selection.
    pub const V251: Self = Self(analyte_hl7::Version::V251);
    /// Explicit HL7 26 selection.
    pub const V26: Self = Self(analyte_hl7::Version::V26);
    /// Explicit HL7 282 selection.
    pub const V282: Self = Self(analyte_hl7::Version::V282);
    /// Explicit HL7 291 selection.
    pub const V291: Self = Self(analyte_hl7::Version::V291);
}
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
    /// Explicit `Barcode` selection.
    Barcode,
    /// Explicit `HostOrder` selection.
    HostOrder,
    /// Explicit `Operator` selection.
    Operator,
    /// Explicit `Unknown` selection.
    Unknown,
}

/// Field ownership; selections never search globally.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Group {
    /// Explicit `Message` selection.
    Message,
    /// Explicit `Patient` selection.
    Patient,
    /// Explicit `Order` selection.
    Order,
    /// Explicit `Observation` selection.
    Observation,
}

/// Classification evidence, never a QC decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Classification {
    /// Explicit `Patient` selection.
    Patient,
    /// Explicit `Control` selection.
    Control,
    /// Explicit `Calibration` selection.
    Calibration,
    /// Explicit `Unknown` selection.
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
    /// Envelope.
    #[serde(deserialize_with = "crate::selection::framing")]
    pub kind: String,
    /// Frame bytes including ASTM overhead, or payload bytes otherwise.
    pub limit: usize,
    /// Required only for LIS01.
    pub checksum_hex: Option<ChecksumHex>,
}

/// Dialog settings consumed by the host state machine.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dialog {
    /// State machine.
    #[serde(deserialize_with = "crate::selection::dialog")]
    pub kind: String,
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
    /// Syntax family.
    #[serde(deserialize_with = "crate::selection::syntax")]
    pub kind: String,
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

/// Installed POCT identity evidence, never inferred from values or receipt IDs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PoctIdentity {
    /// Require the device sequence in addition to device, observation time and analyte.
    pub sequence_number: bool,
    /// Explicit installed processing mode; no source mode is inferred from Hello.
    pub processing: analyte_model::ProcessingMode,
}

mod mappings;
pub use mappings::*;

mod poct;
pub use poct::*;

mod autolyser;
pub use autolyser::*;

mod departures;
pub use departures::*;

mod refusal;
pub use refusal::DepartureRefusal;
