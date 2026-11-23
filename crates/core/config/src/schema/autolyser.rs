//! Holds raw Autolyser TOML policy settings.
use super::{Deserialize, Serialize};

/// Poll cadence supplied by the definition or required from the lab.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PollInterval {
    /// Milliseconds fixed by an existing definition.
    Milliseconds(u32),
    /// The lab must provide the interval, in whole seconds.
    Site(PollSource),
}

/// The only supported source for a site-entered poll interval.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PollSource {
    /// The site's `autolyser_poll_seconds` setting.
    Site,
}

/// Autolyser connection timing and installed report layout.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Autolyser {
    /// Interval between operations.
    pub poll_ms: PollInterval,
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
