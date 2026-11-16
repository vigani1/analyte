//! Holds installed VIDAS acknowledgment and checksum choices.
use super::VidasSampleLimit;
use serde::{Deserialize, Serialize};

/// Installed point at which VIDAS expects acknowledgment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "bool", into = "bool")]
pub enum AckPoint {
    /// Acknowledge after the checksum.
    Checksum,
    /// Acknowledge after ETX.
    Etx,
}

impl From<bool> for AckPoint {
    fn from(selected: bool) -> Self {
        if selected { Self::Etx } else { Self::Checksum }
    }
}

impl From<AckPoint> for bool {
    fn from(value: AckPoint) -> Self {
        match value {
            AckPoint::Checksum => false,
            AckPoint::Etx => true,
        }
    }
}

/// Whether a boundary byte participates in the checksum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "bool", into = "bool")]
pub enum ChecksumInclusion {
    /// Exclude the configured boundary bytes.
    Excluded,
    /// Include the configured boundary bytes.
    Included,
}

impl From<bool> for ChecksumInclusion {
    fn from(selected: bool) -> Self {
        if selected {
            Self::Included
        } else {
            Self::Excluded
        }
    }
}

impl From<ChecksumInclusion> for bool {
    fn from(value: ChecksumInclusion) -> Self {
        match value {
            ChecksumInclusion::Excluded => false,
            ChecksumInclusion::Included => true,
        }
    }
}

/// Case used when emitting checksum hex digits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "bool", into = "bool")]
pub enum HexCase {
    /// Emit lowercase hex.
    Lower,
    /// Emit uppercase hex.
    Upper,
}

impl From<bool> for HexCase {
    fn from(selected: bool) -> Self {
        if selected { Self::Upper } else { Self::Lower }
    }
}

impl From<HexCase> for bool {
    fn from(value: HexCase) -> Self {
        match value {
            HexCase::Lower => false,
            HexCase::Upper => true,
        }
    }
}

/// Cases accepted for incoming checksum hex digits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "bool", into = "bool")]
pub enum ChecksumAcceptance {
    /// Accept only the configured emitted case.
    EmittedCaseOnly,
    /// Accept either hex case.
    EitherCase,
}

impl From<bool> for ChecksumAcceptance {
    fn from(selected: bool) -> Self {
        if selected {
            Self::EitherCase
        } else {
            Self::EmittedCaseOnly
        }
    }
}

impl From<ChecksumAcceptance> for bool {
    fn from(value: ChecksumAcceptance) -> Self {
        match value {
            ChecksumAcceptance::EmittedCaseOnly => false,
            ChecksumAcceptance::EitherCase => true,
        }
    }
}

/// Explicit installed VIDAS Literal settings; no defaults.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    /// ACK after ETX rather than checksum.
    #[serde(rename = "alternate")]
    pub ack_point: AckPoint,
    /// Maximum RS chunk bytes.
    pub chunk: usize,
    /// Uniform control suffix bytes.
    pub suffix: Vec<u8>,
    /// Include STX in checksum.
    pub include_stx: ChecksumInclusion,
    /// Include suffix in checksum.
    pub include_suffix: ChecksumInclusion,
    /// Emit uppercase hex.
    #[serde(rename = "uppercase")]
    pub hex_case: HexCase,
    /// Accept either checksum case.
    #[serde(rename = "accept_either_case")]
    pub checksum_acceptance: ChecksumAcceptance,
    /// Installed sample ID ceiling, twelve or seventeen.
    pub sample_limit: VidasSampleLimit,
    /// Peer ACK deadline.
    pub acknowledgment_ms: u32,
    /// Interbyte deadline.
    pub interbyte_ms: u32,
    /// Entire conversation deadline.
    pub session_ms: u32,
    /// Durable storage deadline.
    pub commit_ms: u32,
    /// Recorded retry setting, never automatic order replay.
    pub retries: u16,
    /// Recorded retry backoff.
    pub backoff_ms: u32,
    /// Permit explicit worklist downloads.
    pub download: bool,
}
