//! Keeps AWOS execution evidence separate from command acceptance and result revisions.
use serde::{Deserialize, Serialize};

/// Analyzer execution facts; these never imply a command outcome or a result status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AwosExecution {
    /// The analyzer scheduled the retained AWOS.
    Scheduled,
    /// The analyzer reports processing the retained AWOS.
    InProgress,
    /// The analyzer independently cancelled the retained AWOS.
    AnalyzerCancelled,
}

/// A notification contradicts retained execution evidence and requires reconciliation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("AWOS execution conflict from {previous} to {incoming}")]
pub struct AwosExecutionConflict {
    /// Retained execution fact.
    pub previous: AwosExecution,
    /// Contradictory incoming fact.
    pub incoming: AwosExecution,
}

impl AwosExecution {
    /// Applies the forward-only notification table in `PaLM` TF-2b C.8 Figure C.8-1.
    /// Callers must additionally check command and result history before accepting evidence.
    /// # Errors
    /// Refuses backwards progress, cancellation after processing, and reopening cancellation.
    pub fn advance(self, incoming: Self) -> Result<Self, AwosExecutionConflict> {
        let allowed = match self {
            Self::Scheduled => true,
            Self::InProgress => incoming == Self::InProgress,
            Self::AnalyzerCancelled => incoming == Self::AnalyzerCancelled,
        };
        if allowed {
            Ok(incoming)
        } else {
            Err(AwosExecutionConflict {
                previous: self,
                incoming,
            })
        }
    }
}

/// Projection points at immutable interpretation evidence rather than copying source fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AwosExecutionProjection {
    /// Latest consistent execution fact.
    pub state: AwosExecution,
    /// Physical receipt that established this projection.
    pub receipt: crate::ReceiptId,
    /// Append-only interpretation version within that receipt.
    pub interpretation: std::num::NonZeroU32,
    /// Host commit time, distinct from supplied analyzer times.
    pub recorded_at: i64,
    /// Whether a manager cancel request was outstanding when this evidence committed.
    pub cancel_outstanding: bool,
}
