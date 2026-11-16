//! Retains machine departure provenance separately from source evidence and clinical summaries.
use crate::{EvidenceList, Identifier, SourceText};
use serde::{Deserialize, Serialize};

/// Fixed derived meanings; none claims clinical release or QC acceptance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DepartureMeaning {
    /// A documented omission means a complete final report, not observation status.
    FinalReportFromDocumentation,
    /// A documented omission means a final observation, not report completion.
    FinalObservationFromDocumentation,
    /// Verified inbound frames carried several source records without changing their meaning.
    PackedRecords,
    /// A declared wire compatibility setting was exercised.
    WireCompatibility,
}

/// A bounded source byte range, with an exclusive end; coordinates are named by the entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "std::ops::Range<usize>")]
pub struct DepartureSpan {
    start: usize,
    end: usize,
}
impl TryFrom<std::ops::Range<usize>> for DepartureSpan {
    type Error = &'static str;
    fn try_from(range: std::ops::Range<usize>) -> Result<Self, Self::Error> {
        if range.start > range.end {
            return Err("departure span end precedes start");
        }
        Ok(Self {
            start: range.start,
            end: range.end,
        })
    }
}

/// One actually exercised rule; original source values are never rewritten.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppliedDeparture {
    /// Stable rule tag.
    pub rule: Identifier,
    /// Exact declaration in the existing TOML config format, including scope and evidence level.
    pub declaration: SourceText,
    /// Standard clause whose diagnostic or boundary rule was relaxed.
    pub standard: Identifier,
    /// One-based source record/segment occurrence, zero only for a frame-level accommodation.
    pub record: usize,
    /// Source field, zero for record packing.
    pub field: usize,
    /// Original absence or wire-layout description; never a fabricated source status.
    pub source: SourceText,
    /// Exact derived meaning, distinct from received status.
    pub meaning: DepartureMeaning,
    /// Coordinate system: syntax bytes for HL7, retained receipt bytes for ASTM frames.
    pub coordinates: Identifier,
    /// Exact source spans; a continued record can have several frame-local spans.
    pub spans: EvidenceList<DepartureSpan>,
}

/// Every new interpretation carries enabled declarations and an applied list, including empty lists.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DepartureEvidence {
    /// Exact enabled declaration snapshots, encoded with the existing TOML config format.
    pub enabled: EvidenceList<SourceText>,
    /// Rules actually exercised while interpreting these particular source bytes.
    pub applied: EvidenceList<AppliedDeparture>,
    /// A bounded ledger failure means this applied list is incomplete and delivery must stop.
    #[serde(default)]
    pub incomplete: Option<crate::Reason>,
}
