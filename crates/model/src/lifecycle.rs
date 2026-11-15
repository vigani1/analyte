use serde::{Deserialize, Serialize};

/// Mode is explicit so training and debugging cannot silently become production.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProcessingMode {
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
/// An attempted state change is not supported by lifecycle evidence.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("illegal order transition from {from:?} to {to:?}")]
pub struct TransitionError {
    /// Current persisted state.
    pub from: OrderState,
    /// Attempted next state.
    pub to: OrderState,
}
impl OrderState {
    /// Checks a state change without performing I/O or resending an order.
    /// Terminal states have no outgoing transitions. Uncertain outcomes can only
    /// be reconciled with machine evidence or a cancellation request.
    /// # Errors
    /// Returns both states when the change is illegal.
    pub fn transition(self, to: Self) -> Result<Self, TransitionError> {
        let valid = match self {
            Self::Requested => matches!(to, Self::Sent | Self::CancelRequested | Self::Rejected),
            Self::Sent => matches!(
                to,
                Self::Accepted | Self::Rejected | Self::CancelRequested | Self::Uncertain
            ),
            Self::Accepted => matches!(
                to,
                Self::Done | Self::Rejected | Self::CancelRequested | Self::Uncertain
            ),
            Self::CancelRequested => matches!(
                to,
                Self::CancelConfirmed | Self::Done | Self::Rejected | Self::Uncertain
            ),
            Self::Uncertain => matches!(
                to,
                Self::Accepted
                    | Self::Done
                    | Self::Rejected
                    | Self::CancelRequested
                    | Self::CancelConfirmed
            ),
            Self::Done | Self::Rejected | Self::CancelConfirmed => false,
        };
        if valid {
            Ok(to)
        } else {
            Err(TransitionError { from: self, to })
        }
    }
}
/// A lifecycle event keeps its source and time rather than only a summary state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderEvent {
    /// State reached using the transition function.
    pub state: OrderState,
    /// Reporting system.
    pub source: crate::Identifier,
    /// Unmodified source status code.
    pub source_code: crate::SourceText,
    /// Time the host recorded the event.
    pub recorded_at: jiff::Timestamp,
}
