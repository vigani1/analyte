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

/// Wire envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FramingKind {
    /// Explicit `AstmLis01` selection.
    AstmLis01,
    /// Explicit `Mllp` selection.
    Mllp,
    /// Explicit `None` selection.
    None,
    /// Documented Poct protocol layer.
    Poct,
    /// Documented VIDAS Literal protocol layer.
    VidasLiteral,
    /// Documented Autolyser protocol layer.
    Autolyser,
}

/// Connection state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DialogKind {
    /// Explicit `AstmLink` selection.
    AstmLink,
    /// Explicit `Hl7Receiver` selection.
    Hl7Receiver,
    /// IHE LAW Analyzer Manager, using the LAW application acknowledgment rules.
    LawManager,
    /// Explicit `FilePickup` selection.
    FilePickup,
    /// Documented Poct protocol layer.
    Poct,
    /// Documented VIDAS Literal protocol layer.
    VidasLiteral,
    /// Documented Autolyser protocol layer.
    Autolyser,
}

/// Record syntax.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyntaxKind {
    /// Explicit `AstmLis02` selection.
    AstmLis02,
    /// Explicit `Hl7V2` selection.
    Hl7V2,
    /// Documented Poct protocol layer.
    Poct,
    /// Documented VIDAS Literal protocol layer.
    VidasLiteral,
    /// Documented Autolyser protocol layer.
    Autolyser,
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

/// Supported HL7 editions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Edition {
    /// Explicit HL7 2.3 selection; no patch-edition upgrade.
    V23,
    /// Explicit HL7 2.3.1 selection.
    V231,
    /// Explicit HL7 2.4 selection.
    V24,
    /// Explicit HL7 2.5 selection; no patch-edition upgrade.
    V25,
    /// Explicit HL7 2.5.1 selection.
    V251,
    /// Explicit HL7 2.6 selection.
    V26,
    /// Explicit HL7 2.8.2 selection.
    V282,
    /// Explicit HL7 2.9.1 selection.
    V291,
}

impl From<Edition> for analyte_hl7::Version {
    fn from(edition: Edition) -> Self {
        match edition {
            Edition::V23 => Self::V23,
            Edition::V231 => Self::V231,
            Edition::V24 => Self::V24,
            Edition::V25 => Self::V25,
            Edition::V251 => Self::V251,
            Edition::V26 => Self::V26,
            Edition::V282 => Self::V282,
            Edition::V291 => Self::V291,
        }
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
    pub kind: FramingKind,
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
    pub kind: DialogKind,
    /// Receive deadline.
    pub receive_timeout_ms: u32,
    /// Send deadline.
    pub send_timeout_ms: u32,
    /// Required only for ASTM.
    pub contention_priority: Option<Priority>,
    /// Required only for HL7.
    pub acknowledgment: Option<Responsibility>,
}

/// Syntax options checked against the selected protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Syntax {
    /// Syntax family.
    pub kind: SyntaxKind,
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

/// Named departure attached to an exact config key.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Deviation {
    /// Reviewable name.
    pub name: Identifier,
    /// Affected protocol key.
    pub key: Identifier,
    /// Why it is needed.
    pub reason: analyte_model::Reason,
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
    pub protocol: Protocol,
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
    pub deviations: Vec<Deviation>,
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
    pub base_url: String,
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
    pub deviations: Option<Vec<Deviation>>,
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

/// Protocol family, independent of physical transport.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
    /// ASTM LIS02.
    Astm,
    /// Generic HL7 v2.
    Hl7,
    /// IHE LAW profile.
    Law,
    /// POCT reviewer.
    Poct,
    /// VIDAS Literal.
    Vidas,
    /// Autolyser polling.
    Autolyser,
}

mod mappings;
pub use mappings::*;

mod poct;
pub use poct::*;

mod autolyser;
pub use autolyser::*;
