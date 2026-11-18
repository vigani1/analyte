//! Reports LAW parse failures and prepares located negative acknowledgments.
use super::{
    AckCode, AckError, Action, Dialog, Error, ExchangeId, Link, Plan, State, header_error,
};

impl Dialog {
    pub(super) fn parse_failed(
        &mut self,
        id: ExchangeId,
        payload: &[u8],
        error: analyte_hl7::Error,
        out: &mut Vec<Action>,
    ) -> Result<(), Error> {
        let error = std::sync::Arc::new(error);
        out.push(Action::Parked(id, super::Failure::Syntax(error.clone())));
        if self.link == Link::WorkResponse {
            return Err(Error::Syntax(error));
        }
        let (code, location) = match error.as_ref() {
            analyte_hl7::Error::Control {
                segment: "MSH",
                field,
            } => (
                AckCode::Ar,
                u16::try_from(*field)
                    .ok()
                    .map(|field| header_error(field, b"102", b"LAW-control")),
            ),
            analyte_hl7::Error::UnsupportedCharset
            | analyte_hl7::Error::CharsetMismatch
            | analyte_hl7::Error::UndeclaredCharset => {
                (AckCode::Ar, Some(header_error(18, b"102", b"LAW-control")))
            }
            analyte_hl7::Error::EditionRules(_)
            | analyte_hl7::Error::WithdrawnQuery { .. }
            | analyte_hl7::Error::Limit { .. }
            | analyte_hl7::Error::Encoding
            | analyte_hl7::Error::Truncated { .. }
            | analyte_hl7::Error::Truncation
            | analyte_hl7::Error::Terminator
            | analyte_hl7::Error::Segment(_)
            | analyte_hl7::Error::Escape { .. }
            | analyte_hl7::Error::CharacterEncoding
            | analyte_hl7::Error::Control { .. }
            | analyte_hl7::Error::NotStored
            | analyte_hl7::Error::AckMode => (AckCode::Ae, None),
        };
        self.state = State::Respond(
            id,
            Plan::Reject(
                payload.to_vec(),
                code,
                location
                    .unwrap_or(AckError::unlocated(b"102", b"LAW-syntax"))
                    .into(),
            ),
        );
        out.push(Action::ResponseNeeded(id));
        Ok(())
    }
}
