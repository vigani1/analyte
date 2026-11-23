//! Defines version-1 result evidence types and validates their wire values.
use super::{
    Dilution, EvidenceList, Identifier, IdentityStatus, NamedSourceTime, NamespacedId,
    NonEmptyEvidenceList, ProcessingMode, ReceiptId, ResultValue, SampleIdentity, SourceText,
    Units,
};
use serde::{Deserialize, Serialize};
/// Source classification only; no QC decision is made here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QcClass {
    /// Patient specimen.
    Patient,
    /// Control material.
    Control,
    /// Calibration material.
    Calibration,
    /// Not classified by the source.
    Unknown,
}

/// Analyte summary, retained beside the source's own code. The summary never
/// replaces the source status; it only groups source codes with one meaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ResultStatus {
    /// Not yet available, for example in process on the instrument.
    Pending,
    /// Preliminary observation.
    Preliminary,
    /// Final observation.
    Final,
    /// Corrected revision of an earlier result.
    Corrected,
    /// An earlier result is withdrawn and must no longer be used (for example
    /// HL7 OBX-11 D or W); the raw history is kept.
    Withdrawn,
    /// The test could not be performed (for example HL7 X, ASTM X).
    NotPerformed,
    /// Source status not mapped to a summary.
    Unknown,
}

/// One source hierarchy node; the flat path avoids recursive untrusted trees.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SourceNode {
    /// One-based record or segment position in the source message.
    pub occurrence: u64,
    /// Source record/segment category, unchanged.
    pub kind: Identifier,
    /// Source sequence identifier when supplied.
    pub sequence: Option<SourceText>,
    /// Source status at this level when supplied.
    pub status: Option<SourceText>,
}

/// Source control material and its reported comparison data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ControlMaterial {
    /// Control identifier as sent.
    pub id: SourceText,
    /// Lot matched to the measurement by source index.
    pub lot: SourceText,
    /// Expiry text as sent.
    pub expiry: SourceText,
    /// Concentration level as sent.
    pub level: SourceText,
    /// Reported mean as sent.
    pub mean: SourceText,
    /// Reported standard deviation as sent.
    pub standard_deviation: SourceText,
}

/// Source role of one component in a sample interference panel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InterferenceComponent {
    /// Turbidity (lipemia).
    Turbidity,
    /// Hemolysis.
    Hemolysis,
    /// Jaundice (icterus).
    Jaundice,
}

/// Additional source evidence where a result upload defines these fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ResultDetails {
    /// Source-reported distinction between measured and calculated values.
    pub method: Option<MeasurementMethod>,
    /// Exact source specimen descriptor.
    pub specimen_descriptor: Option<SourceText>,
    /// Separate report-level status, with its original code in the source path.
    pub report_status: Option<ResultStatus>,
    /// Source-reported test cutoff, including an explicitly absent value.
    pub cutoff_index: Option<ResultValue>,
    /// Source message code accompanying a quantitative value.
    pub message_code: Option<SourceText>,
    /// Structured measurement alarms in original order.
    pub alarms: EvidenceList<ResultAlarm>,
    /// Source predilution indicator, independent of the selected factor.
    pub predilution: Option<SourceText>,
    /// Complete specimen or control label when its meaning is conditional.
    pub specimen_label: Option<SourceText>,
    /// Control name, only when the source layout establishes it.
    pub control_name: Option<SourceText>,
    /// Complete terminator code as sent.
    pub termination: Option<SourceText>,
    /// Interpretation of the source termination, when the layout declares one.
    pub completion: Option<ReportCompletion>,
}

/// Completion of the source upload, independent of result release.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReportCompletion {
    /// The analyzer signalled normal completion.
    Normal,
    /// The analyzer aborted the result upload.
    SenderAborted,
    /// Completion was not established.
    Unknown,
}

/// Source-reported test method.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MeasurementMethod {
    /// The analyzer measured the value.
    Measured,
    /// The analyzer calculated the value.
    Calculated,
}

/// Numeric alarm code and accompanying text, when transmitted by the source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ResultAlarm {
    /// Exact source code in the configured wire layout.
    pub number: SourceText,
    /// Source alarm text, if sent.
    pub text: Option<SourceText>,
}

/// A result retains source evidence beside summaries, without clinical decisions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ResultPart {
    /// The machine's own test code as sent, in its code system (ADR 0029).
    pub test: NamespacedId,
    /// Original and optional parsed value.
    pub values: EvidenceList<ResultValue>,
    /// Source control row, when explicitly reported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub control: Option<ControlMaterial>,
    /// Component role when the source defines a fixed interference panel.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interference: Option<InterferenceComponent>,
    /// Raw measurement reported separately from the observation value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_measurement: Option<SourceText>,
    /// Units if present.
    pub units: Option<Units>,
    /// Dilution evidence if present.
    pub dilution: Option<Dilution>,
    /// Additional structured evidence from the selected source layout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<ResultDetails>,
    /// Source flags, unchanged.
    pub flags: EvidenceList<SourceText>,
    /// Source reference range, uninterpreted.
    pub reference_range: Option<SourceText>,
    /// Source comments, unchanged.
    pub comments: EvidenceList<SourceText>,
    /// Source material classification.
    pub qc_class: QcClass,
    /// Original result status code.
    pub source_status: SourceText,
    /// Analyte summary, never replacing the source status.
    pub status: ResultStatus,
    /// Ordered path from source message to result, preserving hierarchy.
    pub source_path: EvidenceList<SourceNode>,
    /// Named source times (collection, analysis, etc.).
    pub times: EvidenceList<NamedSourceTime>,
    /// Host receive instant, independent of machine clock.
    #[schemars(with = "crate::schema::Timestamp")]
    pub received_at: jiff::Timestamp,
}

/// One replacement unit can contain several source segments and repeated values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ResultDocument {
    /// The receipt whose raw bytes this result was read from.
    pub receipt: ReceiptId,
    /// The sample the machine reported; absence requires operator parking.
    pub sample: Option<SampleIdentity>,
    /// Processing mode of the source message; only production may be delivered.
    pub processing: ProcessingMode,
    /// Identity may resolve even if a correction arrives before its original.
    pub identity: IdentityStatus,
    /// Source parts in their original order; no scalar flattening or deduplication.
    pub parts: NonEmptyEvidenceList<ResultPart>,
}
