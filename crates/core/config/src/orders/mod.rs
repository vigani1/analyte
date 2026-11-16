//! Holds explicit outbound order settings independently of result mappings.
use crate::{Error, Field, Group, MachineDocument, SyntaxKind, invalid};
use analyte_model::{Identifier, NamespacedId};
use serde::{Deserialize, Serialize};

/// Unknown cache answers never become an empty work list.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WhenUntrusted {
    /// Return the protocol's query error.
    Error,
    /// Send no query response.
    NoAnswer,
}

/// Presence enables queries; both policy and deadline must be explicit.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrderQuery {
    /// Total query response budget in milliseconds.
    pub deadline_ms: u32,
    /// Policy for stale, unavailable or late lookups.
    pub when_untrusted: WhenUntrusted,
}

/// One app code maps to explicitly ordered ASTM universal test components.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutboundTest {
    /// Exact app code system and code.
    pub app: NamespacedId,
    /// Universal test ID components, including significant empty positions.
    pub machine_components: Vec<String>,
}

/// Explicit app specimen type to protocol sample-medium code.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutboundSpecimen {
    /// Exact app code system and value.
    pub app: NamespacedId,
    /// Exact protocol code; ASTM uses explicit text, Autolyser supports S and U.
    pub machine: Identifier,
}

/// Fixed priority codes used by HL7 TQ fields and Autolyser work lists.
/// R and T have protocol-specific meanings; no meaning is inferred across stacks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PriorityCode {
    /// Wire code R.
    R,
    /// Wire code S.
    S,
    /// Wire code A.
    A,
    /// Wire code P.
    P,
    /// Wire code C.
    C,
    /// Wire code T.
    T,
    /// Wire code PRN.
    #[serde(rename = "PRN")]
    Prn,
}

/// A fixed wire code or explicit custom text for configurable ASTM interfaces.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MachinePriority {
    /// A supported fixed wire code.
    Code(PriorityCode),
    /// Explicit custom text; rejected for stacks with fixed priority tables.
    Custom(Identifier),
}

impl MachinePriority {
    /// Returns the exact configured spelling for field-based output.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Code(PriorityCode::R) => "R",
            Self::Code(PriorityCode::S) => "S",
            Self::Code(PriorityCode::A) => "A",
            Self::Code(PriorityCode::P) => "P",
            Self::Code(PriorityCode::C) => "C",
            Self::Code(PriorityCode::T) => "T",
            Self::Code(PriorityCode::Prn) => "PRN",
            Self::Custom(value) => value.as_str(),
        }
    }
}

/// Priority translations are independent of inbound tables.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrderPriority {
    /// App priority: routine, urgent or stat.
    pub app: Identifier,
    /// Exact machine priority.
    pub machine: MachinePriority,
}

/// ASTM O-record destinations; fields retain their standard ownership.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AstmOrderFields {
    /// Specimen ID destination.
    pub specimen: Field,
    /// Instrument specimen ID or position; presence requires supplied app data.
    pub position: Option<Field>,
    /// LIS02 section 8.4.16.1 specimen type: O field 16, component 1.
    pub specimen_type: Option<Field>,
    /// First component of each repeated universal test ID.
    pub test: Field,
    /// Priority destination.
    pub priority: Field,
    /// Action code destination.
    pub action: Field,
    /// Explicit new-order action code.
    pub action_code: Identifier,
}

/// Exact QRD filter agreed with this machine; unsupported filters fail closed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hl7QueryFilter {
    /// QRD field number, excluding metadata, quantity and the specimen selector.
    pub field: u16,
    /// Exact wire value, including any explicitly agreed components.
    pub value: String,
}

/// One DSP-3 line, repeated in this order for each requested test.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "snake_case", deny_unknown_fields)]
pub enum DsrLine {
    /// Exact specimen identifier.
    SampleId,
    /// Original app order identifier.
    OrderId,
    /// Explicit outbound test translation.
    TestCode,
    /// Explicit outbound priority translation.
    Priority,
    /// Supplied physical slot; absent stays empty.
    Position,
    /// Explicit specimen-type translation; absent stays empty.
    SpecimenType,
    /// Supplied patient ID under the configured namespace; absent stays empty.
    PatientId,
    /// Explicit presentation text, including empty padding lines.
    Literal {
        /// Exact plain display text.
        value: String,
    },
}

/// Legacy QRY/QCK/DSR interface agreement; no send-ahead is implied.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hl7OrderQuery {
    /// QRD-8 or QRD-10 interpreted as the exact specimen ID, component 1 only.
    pub sample_field: u16,
    /// Exact trailing components for the specimen selector (empty only when agreed).
    pub sample_suffix: String,
    /// Every other nonempty QRD selector must match an explicit filter.
    pub filters: Vec<Hl7QueryFilter>,
    /// Ordered DSP lines for each test; at most 128 lines per test.
    pub lines: Vec<DsrLine>,
    /// Budget for the DSR application ACK, measured from wire issuance.
    pub acknowledgment_ms: u32,
}

/// Supported HL7 MSH-11 processing IDs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Hl7Processing {
    /// Production processing.
    #[serde(rename = "P")]
    Production,
    /// Training processing.
    #[serde(rename = "T")]
    Training,
    /// Debugging processing.
    #[serde(rename = "D")]
    Debugging,
}

/// Explicit unsolicited HL7 order interface; edition selects ORM or OML.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hl7Send {
    /// Require PID patient identification. Contract v1 has no names, so orders
    /// for this interface are rejected before reservation rather than inventing PID-5.
    #[serde(default)]
    pub require_patient_identification: bool,
    /// Outgoing MSH-3.
    pub application: Identifier,
    /// Outgoing MSH-5.
    pub peer_application: Identifier,
    /// Outgoing MSH-4, empty if absent.
    pub facility: Option<Identifier>,
    /// Outgoing MSH-6, empty if absent.
    pub peer_facility: Option<Identifier>,
    /// Explicit MSH-11 P, T or D.
    pub processing: Hl7Processing,
    /// Application response deadline, also used for cancellation.
    pub response_ms: u32,
    /// ORM specimen placement: OBR-18 or OBR-19; OML uses SPM-2.
    pub sample_field: u16,
    /// Optional other placer-owned field for the supplied slot (ORM only).
    pub position_field: Option<u16>,
    /// Coding system for mapped specimen types, never inferred from a test.
    pub specimen_system: Option<Identifier>,
    /// Explicit coding-system version where required by SPM-4 CWE.
    pub specimen_version: Option<Identifier>,
}

/// Opt-in outbound order capability. Nothing is sent by its absence.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Orders {
    /// Optional query capability.
    pub query: Option<OrderQuery>,
    /// Explicit permission to send work lists ahead.
    pub send_ahead: bool,
    /// Outbound app-to-machine translations.
    pub tests: Vec<OutboundTest>,
    /// Optional explicit specimen-type translations; absence maps no values.
    pub specimen_types: Option<Vec<OutboundSpecimen>>,
    /// Explicit priority translations.
    pub priorities: Vec<OrderPriority>,
    /// Accepted assigning system for the unqualified vendor patient field.
    /// Required for VIDAS and Autolyser outbound orders; never inferred.
    pub patient_namespace: Option<Identifier>,
    /// Explicit Autolyser clone behavior, including an explicit false.
    pub clone: Option<bool>,
    /// ASTM record layout, required only by the ASTM stack.
    pub astm: Option<AstmOrderFields>,
    /// Explicit legacy HL7 query layout and acknowledgment deadline.
    pub hl7: Option<Hl7OrderQuery>,
    /// Required EI.2 for per-test HL7 identities.
    pub placer_namespace: Option<Identifier>,
    /// Explicit send-ahead and cancellation interface.
    pub hl7_send: Option<Hl7Send>,
}

mod hl7;
mod validation;
