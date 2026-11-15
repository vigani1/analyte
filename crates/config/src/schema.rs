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
}

/// Connection state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DialogKind {
    /// Explicit `AstmLink` selection.
    AstmLink,
    /// Explicit `Hl7Receiver` selection.
    Hl7Receiver,
    /// Explicit `FilePickup` selection.
    FilePickup,
}

/// Record syntax.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyntaxKind {
    /// Explicit `AstmLis02` selection.
    AstmLis02,
    /// Explicit `Hl7V2` selection.
    Hl7V2,
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
    /// Explicit `V231` selection.
    V231,
    /// Explicit `V251` selection.
    V251,
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

/// Group-relative one-based field selection.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Field {
    /// Owning group.
    pub group: Group,
    /// Record or segment name.
    pub record: Identifier,
    /// One-based field.
    pub field: u16,
    /// One-based component.
    pub component: u16,
    /// One-based repetition.
    pub repetition: u16,
}

/// Sample identity copied without normalization.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SampleSelection {
    /// Source location.
    pub field: Field,
    /// Acquisition evidence.
    pub source: SampleSource,
}

/// Exact code translation with assigning systems.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestMapping {
    /// Code as received.
    pub machine_code: Identifier,
    /// Source assigning system.
    pub machine_system: Identifier,
    /// Destination code.
    pub app_code: Identifier,
    /// Destination assigning system.
    pub app_system: Identifier,
}

/// Explicit spelling translation; no implicit numeric conversion.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitMapping {
    /// Exact source spelling.
    pub original: Identifier,
    /// Explicit destination UCUM spelling.
    pub ucum: Identifier,
}

/// Exact field equality classifies evidence only.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QcRule {
    /// Source location.
    pub field: Field,
    /// Exact value.
    pub value: Identifier,
    /// Classification to carry.
    pub classification: Classification,
}

/// Assigning namespace, explicitly fixed or selected from the source.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub enum OrderNamespace {
    /// A site-declared assigning system.
    Fixed {
        /// Exact assigning system.
        fixed: Identifier,
    },
    /// An assigning system reported by the source.
    Field {
        /// Group-relative source location.
        field: Field,
    },
}

/// Explicit absence of a sub-ID concept.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NoSubId {
    /// Deliberately use an empty sub-ID.
    #[serde(rename = "none")]
    None,
}

/// Sub-ID evidence or a deliberate declaration that it does not exist.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SubId {
    /// Source-provided sub-ID; missing values remain unresolved.
    Field(Field),
    /// The protocol profile has no sub-ID concept.
    None(NoSubId),
}

/// Explicit identity evidence; source status never assigns a revision.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityEvidence {
    /// Order identity within its assigning namespace.
    pub order_id: Field,
    /// Explicit assigning namespace.
    pub order_namespace: OrderNamespace,
    /// Source sub-ID or deliberate absence.
    pub sub_id: SubId,
    /// Ordered run evidence; empty means reruns cannot be distinguished.
    pub run: Vec<Field>,
    /// Exact source codes declaring a correction of the same observation.
    pub correction_status_codes: Vec<Identifier>,
}

/// Bounded declarative mapping tables without expressions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mappings {
    /// Sample selection.
    pub sample: SampleSelection,
    /// Test code table.
    pub tests: Vec<TestMapping>,
    /// Unit spelling table.
    pub units: Vec<UnitMapping>,
    /// Classification rules.
    pub qc: Vec<QcRule>,
    /// Identity evidence.
    pub identity: IdentityEvidence,
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
pub struct Machine {
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
    pub mappings: Mappings,
    /// Named vendor departures.
    pub deviations: Vec<Deviation>,
    /// Declared evidence.
    pub evidence: Evidence,
}

/// References credentials by name; stores no secret material.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Destination {
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
