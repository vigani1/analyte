//! Raw Autolyser policy at the TOML boundary.
use super::*;

/// Autolyser connection timing and installed report layout.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Autolyser {
    /// Interval between operations.
    pub poll_ms: u32,
    /// Initialization ACK deadline.
    pub acknowledgment_ms: u32,
    /// Total response deadline.
    pub response_ms: u32,
    /// Maximum silence within response.
    pub interbyte_ms: u32,
    /// Durable storage deadline.
    pub storage_ms: u32,
    /// Controls-enabled report layout.
    pub controls: bool,
    /// Request L automatically after corruption at the poll interval.
    pub recover_last_on_poll: bool,
    /// Permit explicit worklist downloads.
    pub download: bool,
}
