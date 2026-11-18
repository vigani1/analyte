//! Owns HL7 acknowledgment phases and evidence retained after disconnect.
use super::{Error, ExchangeId, Pending};

// Input framing and order waits can overlap every open acknowledgment phase.
#[derive(Debug)]
pub(super) enum State {
    Idle,
    Awaiting(Pending),
    Malformed(ExchangeId, Vec<u8>, Error),
    Closed {
        pending: Option<Pending>,
        malformed: Option<(ExchangeId, Vec<u8>, Error)>,
    },
}
impl State {
    pub(super) fn pending(&self) -> Option<&Pending> {
        match self {
            Self::Awaiting(pending) => Some(pending),
            Self::Closed { pending, .. } => pending.as_ref(),
            Self::Idle | Self::Malformed(..) => None,
        }
    }
    pub(super) fn pending_mut(&mut self) -> Option<&mut Pending> {
        match self {
            Self::Awaiting(pending) => Some(pending),
            Self::Closed { pending, .. } => pending.as_mut(),
            Self::Idle | Self::Malformed(..) => None,
        }
    }
    pub(super) fn malformed_id(&self) -> Option<ExchangeId> {
        match self {
            Self::Malformed(id, ..) => Some(*id),
            Self::Closed { malformed, .. } => malformed.as_ref().map(|(id, ..)| *id),
            Self::Idle | Self::Awaiting(_) => None,
        }
    }
    pub(super) fn take_malformed(&mut self) -> Option<(ExchangeId, Vec<u8>, Error)> {
        if let Self::Closed { malformed, .. } = self {
            return malformed.take();
        }
        if matches!(self, Self::Malformed(..)) {
            let Self::Malformed(id, raw, error) = std::mem::replace(self, Self::Idle) else {
                unreachable!("the state was checked before replacement");
            };
            Some((id, raw, error))
        } else {
            None
        }
    }
    pub(super) fn close(&mut self) {
        if matches!(self, Self::Closed { .. }) {
            return;
        }
        let previous = std::mem::replace(self, Self::Idle);
        let (pending, malformed) = match previous {
            Self::Awaiting(pending) => (Some(pending), None),
            Self::Malformed(id, raw, error) => (None, Some((id, raw, error))),
            Self::Idle => (None, None),
            Self::Closed { .. } => unreachable!("closed state returned before replacement"),
        };
        *self = Self::Closed { pending, malformed };
    }
}
