//! Defines the version-1 sample identity: the string ID, how it reached the analyzer and its carrier position.
use super::{Identifier, SampleId};
use serde::{Deserialize, Serialize};

/// How a sample identifier reached the analyzer; unknown remains explicit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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
#[serde(rename_all = "snake_case")]
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
