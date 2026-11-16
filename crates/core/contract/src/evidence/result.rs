//! Defines version-1 result evidence types and validates their wire values.
use super::{
    Dilution, EvidenceList, Identifier, IdentityStatus, NamedSourceTime, NamespacedId,
    NonEmptyEvidenceList, ProcessingMode, ReceiptId, ResultValue, SampleIdentity, SourceText,
    Units,
};
use serde::{Deserialize, Serialize};
/// Source classification only; no QC decision is made here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

/// A result retains source evidence beside summaries, without clinical decisions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResultPart {
    /// Test code with assigning system.
    pub test: NamespacedId,
    /// Original and optional parsed value.
    pub values: EvidenceList<ResultValue>,
    /// Units if present.
    pub units: Option<Units>,
    /// Dilution evidence if present.
    pub dilution: Option<Dilution>,
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
    pub received_at: jiff::Timestamp,
}

/// One replacement unit can contain several source segments and repeated values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
