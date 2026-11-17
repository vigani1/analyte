//! Borrows validated status-only AWOS evidence without manufacturing observations.
use super::{super::OrderStatus, Segment};

/// The LAB-29 execution states that permit an absent RESULT group (TF-2b §3.29.4.1.2.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionStatus {
    /// Scheduled work, before processing.
    Scheduled,
    /// Processing has begun, without observations in this group.
    InProgress,
    /// Independent analyzer cancellation, never a CR response.
    AnalyzerCancelled,
}

impl ExecutionStatus {
    pub(super) fn from_order(status: Option<OrderStatus>) -> Option<Self> {
        match status {
            Some(OrderStatus::Scheduled) => Some(Self::Scheduled),
            Some(OrderStatus::InProgress) => Some(Self::InProgress),
            Some(OrderStatus::Cancelled) => Some(Self::AnalyzerCancelled),
            Some(OrderStatus::Complete) | None => None,
        }
    }
}

/// Original segments for one empty AWOS group; validation has checked the entire LAB-29.
#[derive(Debug, Clone, Copy)]
pub struct StatusNotice<'a> {
    pub(super) order_index: usize,
    pub(super) status: ExecutionStatus,
    pub(super) order: Segment<'a>,
    pub(super) control: Segment<'a>,
    pub(super) specimen: Segment<'a>,
    pub(super) container: Segment<'a>,
}

impl<'a> StatusNotice<'a> {
    /// Returns the zero-based OBR occurrence in the original message.
    #[must_use]
    pub fn order_index(self) -> usize {
        self.order_index
    }

    /// Execution evidence, independent of command and observation statuses.
    #[must_use]
    pub fn status(self) -> ExecutionStatus {
        self.status
    }
    /// Original OBR, including manager AWOS and service identity.
    #[must_use]
    pub fn order(self) -> Segment<'a> {
        self.order
    }
    /// Original ORC, including status, source time and supplied estimate.
    #[must_use]
    pub fn control(self) -> Segment<'a> {
        self.control
    }
    /// Original specimen context; no sample identity is guessed.
    #[must_use]
    pub fn specimen(self) -> Segment<'a> {
        self.specimen
    }
    /// Original container context.
    #[must_use]
    pub fn container(self) -> Segment<'a> {
        self.container
    }
}
