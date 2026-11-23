//! Holds result content and revision evidence.
use crate::{
    Dilution, Identifier, IdentityStatus, ProcessingMode, ReceiptId, ResultValue, SampleId,
    SourceText, Units,
};
use serde::{Deserialize, Serialize};

/// How a sample identifier reached the analyzer; unknown remains explicit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SampleSource {
    /// Read by the analyzer's scanner.
    ScannedByMachine,
    /// Entered at the analyzer.
    TypedOnMachine,
    /// Supplied in a work list.
    WorkList,
    /// Assigned by the analyzer.
    InstrumentAssigned,
    /// Not established by source evidence.
    Unknown,
}

/// Carrier category; position never substitutes for sample identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CarrierKind {
    /// Rack carrier.
    Rack,
    /// Disk carrier.
    Disk,
    /// Tray carrier.
    Tray,
    /// Another explicitly named carrier.
    Other(Identifier),
}

/// Source-reported carrier and slot, including nonnumeric labels.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SamplePosition {
    /// Carrier category.
    pub kind: CarrierKind,
    /// Carrier identity when reported.
    pub carrier: Option<Identifier>,
    /// Slot as sent, preserving leading zeros.
    pub slot: Identifier,
}

/// Sample identity with provenance and optional physical position.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SampleIdentity {
    /// String ID, never converted to a number.
    pub id: SampleId,
    /// Origin of this ID.
    pub source: SampleSource,
    /// Physical position when reported.
    pub position: Option<SamplePosition>,
}

/// Source classification only; no QC decision is made here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

/// Written time precision; the original text retains fractional digits exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimePrecision {
    /// Year only.
    Year,
    /// Through month.
    Month,
    /// Through day.
    Day,
    /// Through hour.
    Hour,
    /// Through minute.
    Minute,
    /// Through second.
    Second,
    /// Subsecond digits, retained in original text.
    FractionalSecond,
    /// Precision could not be established.
    Unknown,
}

/// Source time evidence without inventing a timezone for a partial/local time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceTime {
    /// Time exactly as reported.
    pub original: SourceText,
    /// Parsed local date/time if available; never supplies an implied offset.
    pub parsed: Option<jiff::civil::DateTime>,
    /// Explicit source offset if supplied.
    pub offset: Option<UtcOffset>,
    /// Written precision.
    pub precision: TimePrecision,
}

/// A bounded sequence, capped at 256 entries including during deserialization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct EvidenceList<T>(Vec<T>);
impl<T> Default for EvidenceList<T> {
    fn default() -> Self {
        Self(Vec::new())
    }
}
/// A source supplied too many evidence entries.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("evidence list exceeds 256 entries")]
pub struct ListError;
impl<T> EvidenceList<T> {
    /// Maximum retained entries, enforced by construction and deserialization.
    pub const MAX_ENTRIES: usize = 256;

    /// Accepts at most 256 entries.
    /// # Errors
    /// Rejects a longer list.
    pub fn new(entries: Vec<T>) -> Result<Self, ListError> {
        if entries.len() > Self::MAX_ENTRIES {
            return Err(ListError);
        }
        Ok(Self(entries))
    }

    /// Borrows entries without permitting unbounded mutation.
    #[must_use]
    pub fn as_slice(&self) -> &[T] {
        &self.0
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for EvidenceList<T> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Visitor<T>(std::marker::PhantomData<T>);
        impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for Visitor<T> {
            type Value = EvidenceList<T>;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("at most 256 evidence entries")
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<Self::Value, A::Error> {
                let mut entries = Vec::new();
                while let Some(entry) = seq.next_element()? {
                    if entries.len() == EvidenceList::<T>::MAX_ENTRIES {
                        return Err(serde::de::Error::custom(ListError));
                    }
                    entries.push(entry);
                }
                Ok(EvidenceList(entries))
            }
        }
        d.deserialize_seq(Visitor(std::marker::PhantomData))
    }
}

/// One source hierarchy node; the flat path avoids recursive untrusted trees.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceNode {
    /// One-based record or segment position in the source message.
    pub occurrence: usize,
    /// Source record/segment category, unchanged.
    pub kind: Identifier,
    /// Source sequence identifier when supplied.
    pub sequence: Option<SourceText>,
    /// Source status at this level when supplied.
    pub status: Option<SourceText>,
}

/// Source-defined fields that retain their exact transmitted text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "field", content = "original", rename_all = "snake_case")]
pub enum ResultAttribute {
    /// Complete label identifying control material.
    ControlLabel(SourceText),
    /// Control name or identifier, where the source distinguishes it from its lot.
    ControlName(SourceText),
    /// Lot of the material or test strip used for a control.
    ControlLot(SourceText),
    /// Source discriminator for known or unknown control material.
    ControlKnowledge(SourceText),
    /// Level of the control used in the measurement.
    ControlLevel(SourceText),
    /// List the analyzer reports the measurement under.
    ReportList(SourceText),
    /// Medium identified by the analyzer.
    SampleMedium(SourceText),
    /// Identifier of the analyzer that produced the result.
    InstrumentId(SourceText),
    /// Instrument serial number, including any embedded spaces.
    InstrumentSerial(SourceText),
    /// Technologist named in the source result.
    Technologist(SourceText),
}

/// A result retains source evidence beside summaries, without clinical decisions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResultPart {
    /// The machine's own test code as sent, in its code system (ADR 0029).
    pub test: crate::NamespacedId,
    /// Original and optional parsed value.
    pub values: EvidenceList<ResultValue>,
    /// Units if present.
    pub units: Option<Units>,
    /// Dilution evidence if present.
    pub dilution: Option<Dilution>,
    /// Additional fields as reported by the analyzer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attributes: Option<EvidenceList<ResultAttribute>>,
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

/// A time with its source-defined role.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamedSourceTime {
    /// Source role rather than a guessed clinical meaning.
    pub role: Identifier,
    /// Source time evidence.
    pub time: SourceTime,
}

/// Source UTC offset in seconds, bounded by Jiff's supported offset range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "i32", into = "i32")]
pub struct UtcOffset(i32);
/// Offset is outside the supported civil-time range.
#[derive(Debug, thiserror::Error)]
#[error("UTC offset is outside -25:59:59 through +25:59:59")]
pub struct OffsetError;
impl TryFrom<i32> for UtcOffset {
    type Error = OffsetError;
    fn try_from(seconds: i32) -> Result<Self, Self::Error> {
        jiff::tz::Offset::from_seconds(seconds).map_err(|_| OffsetError)?;
        Ok(Self(seconds))
    }
}

impl From<UtcOffset> for i32 {
    fn from(offset: UtcOffset) -> Self {
        offset.0
    }
}

/// One replacement unit can contain several source segments and repeated values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResultContent {
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

/// A replacement unit must contain source evidence, even when its value is unsupported.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct NonEmptyEvidenceList<T>(EvidenceList<T>);
/// Replacement evidence cannot be absent.
#[derive(Debug, thiserror::Error)]
#[error("replacement unit has no source parts")]
pub struct EmptyEvidence;
impl<T> NonEmptyEvidenceList<T> {
    /// Requires at least one entry in an already bounded list.
    /// # Errors
    /// Rejects an empty replacement unit.
    pub fn new(entries: EvidenceList<T>) -> Result<Self, EmptyEvidence> {
        if entries.as_slice().is_empty() {
            return Err(EmptyEvidence);
        }
        Ok(Self(entries))
    }

    /// Returns source parts in order, retaining repeated content.
    #[must_use]
    pub fn as_slice(&self) -> &[T] {
        self.0.as_slice()
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for NonEmptyEvidenceList<T> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Self::new(EvidenceList::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}

impl ResultContent {
    /// Unresolved identities, a missing sample or unsupported values require
    /// operator parking. This is a minimum safety check, not clinical acceptance.
    #[must_use]
    pub fn requires_parking(&self) -> bool {
        let unresolved = match &self.identity {
            IdentityStatus::Resolved(_) => false,
            IdentityStatus::Unresolved(_) => true,
        };
        unresolved
            || self.sample.is_none()
            || self.parts.as_slice().iter().any(|part| {
                part.values
                    .as_slice()
                    .iter()
                    .any(|value| value.parsed().is_unsupported())
            })
    }

    /// Whether this result may go to a production delivery: production mode and
    /// nothing that requires parking. Training and debug results never qualify.
    #[must_use]
    pub fn may_deliver_to_production(&self) -> bool {
        self.processing.may_deliver_to_production() && !self.requires_parking()
    }
}
