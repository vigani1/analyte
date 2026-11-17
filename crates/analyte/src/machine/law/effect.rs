//! Separates LAW host effects from bytes returned to the input queue.
use super::{Action, Duration, protocol};

pub(super) enum Effect {
    Store(protocol::ExchangeId, Vec<u8>),
    Query(protocol::ExchangeId, analyte_hl7::Message, Duration),
    WorkResponse(protocol::ExchangeId, analyte_hl7::Message),
    Interpret(protocol::ExchangeId),
    Parked(protocol::ExchangeId, protocol::Failure),
    ResponseNeeded(protocol::ExchangeId),
    Write(analyte_dialog::Write),
    WorkIssued,
    Done,
    Rejected(analyte_framing::hl7::Error, Vec<u8>),
    Uncommitted(Vec<u8>),
    Refused(protocol::Error),
    Deadline,
}
impl TryFrom<Action> for Effect {
    type Error = u8;
    fn try_from(action: Action) -> Result<Self, u8> {
        Ok(match action {
            Action::Unconsumed(byte) => return Err(byte),
            Action::Store(a, b) => Self::Store(a, b),
            Action::Query(a, b, c) => Self::Query(a, b, c),
            Action::WorkResponse(a, b) => Self::WorkResponse(a, b),
            Action::Interpret(a) => Self::Interpret(a),
            Action::Parked(a, b) => Self::Parked(a, b),
            Action::ResponseNeeded(a) => Self::ResponseNeeded(a),
            Action::Write(a) => Self::Write(a),
            Action::WorkIssued => Self::WorkIssued,
            Action::Done => Self::Done,
            Action::Rejected(a, b) => Self::Rejected(a, b),
            Action::Uncommitted(_, raw) => Self::Uncommitted(raw),
            Action::Refused(a) => Self::Refused(a),
            Action::Deadline(_) => Self::Deadline,
        })
    }
}
