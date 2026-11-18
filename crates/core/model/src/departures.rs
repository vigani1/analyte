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
    /// Deliberate tolerance: a literal control character in text was kept byte for byte.
    ControlCharacterKept,
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
    /// Exact declaration in the existing TOML config format, including scope and evidence
    /// level. A deliberate tolerance has no declaration: it names the setting it used, or
    /// says that it is always on.
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

impl std::fmt::Display for DepartureSpan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "DepartureSpan {{ start: {}, end: {} }}",
            self.start, self.end
        )
    }
}
impl std::fmt::Display for EvidenceList<DepartureSpan> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("EvidenceList([")?;
        for (index, span) in self.as_slice().iter().enumerate() {
            if index != 0 {
                f.write_str(", ")?;
            }
            write!(f, "{span}")?;
        }
        f.write_str("])")
    }
}
#[test]
fn departure_span_spellings_are_compatible() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        EvidenceList::<DepartureSpan>::default().to_string(),
        "EvidenceList([])"
    );
    let spans = EvidenceList::new(vec![
        DepartureSpan::try_from(1..3)?,
        DepartureSpan::try_from(4..4)?,
    ])?;
    assert_eq!(
        spans.to_string(),
        "EvidenceList([DepartureSpan { start: 1, end: 3 }, DepartureSpan { start: 4, end: 4 }])"
    );
    Ok(())
}
