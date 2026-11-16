//! Defines version-1 lifecycle evidence types and validates their wire values.
use serde::{Deserialize, Serialize};
/// Mode is explicit so training and debugging cannot silently become production.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessingMode {
    /// The machine did not state a processing mode; never production-eligible.
    Unknown,
    /// Eligible for production delivery.
    Production,
    /// Training evidence only.
    Training,
    /// Debugging evidence only.
    Debug,
    /// Quality control evidence only.
    QualityControl,
}

impl ProcessingMode {
    /// Only production mode is eligible for production delivery.
    #[must_use]
    pub fn may_deliver_to_production(self) -> bool {
        self == Self::Production
    }
}

/// Order progress; persistence and machine commands remain the host's responsibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderState {
    /// Requested by the app, not sent.
    Requested,
    /// Sent, awaiting machine acceptance.
    Sent,
    /// Machine explicitly accepted.
    Accepted,
    /// Machine reported completion.
    Done,
    /// Explicitly rejected.
    Rejected,
    /// Cancellation requested, not yet confirmed.
    CancelRequested,
    /// Cancellation explicitly confirmed.
    CancelConfirmed,
    /// Send outcome unknown; reconciliation required, never automatic resend.
    Uncertain,
}
