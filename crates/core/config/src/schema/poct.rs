//! Holds raw POCT TOML policy settings.
use super::{Deserialize, Serialize};

/// POCT DML reviewer resource and time budgets.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Poct {
    /// Explicit trusted POCT source identity policy; absence keeps results unresolved.
    pub identity: Option<PoctIdentity>,
    /// Maximum pending custody requests.
    pub pending_messages: usize,
    /// Maximum XML nesting depth.
    pub depth: usize,
    /// Maximum XML elements.
    pub elements: usize,
    /// Maximum encoded attribute bytes.
    pub attribute_bytes: usize,
    /// Device response deadline.
    pub response_ms: u32,
    /// ACK budget when DCP omits it.
    pub acknowledgment_ms: u32,
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
