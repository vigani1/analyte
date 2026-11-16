//! Recovery-oriented error classes from TF-2b B.7; no error owns patient data.
use super::{Error, Query, Results, WorkResponse, content, raw};
use crate::{AckCode, Message, Msa};
use thiserror::Error as ThisError;

/// Location available from validation, without guessing a segment occurrence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ErrorLocation {
    /// Segment identifier.
    pub segment: &'static str,
    /// One-based field; zero identifies structure/cardinality rather than a field.
    pub field: usize,
    /// Actual segment occurrence when retained by validation; never inferred from its name.
    pub occurrence: Option<usize>,
}

/// Host recovery classes; a failed response is never acknowledged in an ACK loop.
#[derive(Debug, ThisError)]
pub enum Failure {
    /// Malformed triggered content needs AE, retaining the original.
    #[error("malformed LAW trigger: {0}")]
    MalformedTrigger(#[source] Error),

    /// Invalid control content or inconsistent results needs AR.
    #[error("LAW content rejected: {0}")]
    ContentReject(#[source] Error),

    /// An invalid acknowledgment fails the exchange; do not ACK it.
    #[error("invalid LAW acknowledgment: {0}")]
    InvalidAcknowledgment(#[source] Error),

    /// An inconsistent ORL fails the exchange, leaving prior order acceptance uncertain.
    #[error("inconsistent LAW work response: {0}")]
    InconsistentOrl(#[source] Error),

    /// Host query processing failed; QAK reports the error, never no work.
    #[error("LAW query processing rejected at {location:?}")]
    QueryRejected {
        /// Error location supplied by the host.
        location: ErrorLocation,
    },
}

impl Failure {
    /// Returns the strongest available source location; none when syntax cannot identify it.
    #[must_use]
    pub fn location(&self) -> Option<ErrorLocation> {
        let error = match self {
            Self::MalformedTrigger(error)
            | Self::ContentReject(error)
            | Self::InvalidAcknowledgment(error)
            | Self::InconsistentOrl(error) => error,
            Self::QueryRejected { location } => return Some(*location),
        };
        let occurrence = error.occurrence();
        match error.base() {
            Error::DilutionDefinition { segment, field, .. }
            | Error::Subcomponent { segment, field, .. }
            | Error::Authority { segment, field, .. }
            | Error::Component { segment, field, .. }
            | Error::Content { segment, field }
            | Error::Inconsistent { segment, field }
            | Error::Syntax(crate::Error::Control { segment, field }) => Some(ErrorLocation {
                segment,
                field: *field,
                occurrence,
            }),
            Error::AtSegment { .. }
            | Error::Syntax(
                crate::Error::Limit { .. }
                | crate::Error::Encoding
                | crate::Error::Truncation
                | crate::Error::Terminator
                | crate::Error::Segment(_)
                | crate::Error::Escape { .. }
                | crate::Error::UnsupportedCharset
                | crate::Error::CharacterEncoding
                | crate::Error::CharsetMismatch
                | crate::Error::NotStored
                | crate::Error::AckMode,
            ) => None,
        }
    }

    /// Renders the retained field location, omitting only locations whose occurrence is unknown.
    /// A single segment in the original proves occurrence one; repeated segments need evidence.
    #[must_use]
    pub fn acknowledgment_error<'a>(
        &self,
        message: &Message,
        code: &'a [u8],
        application_code: &'a [u8],
    ) -> crate::AckError<'a> {
        let located = self
            .location()
            .filter(|l| l.field > 0)
            .and_then(|location| {
                let field = u16::try_from(location.field).ok()?;
                let occurrence = location.occurrence.or_else(|| {
                    (message
                        .segments()
                        .filter(|s| s.id() == location.segment.as_bytes())
                        .count()
                        == 1)
                        .then_some(1)
                })?;
                Some(crate::AckError {
                    segment: location.segment.as_bytes(),
                    occurrence: u16::try_from(occurrence).ok()?,
                    field,
                    code,
                    application_code,
                })
            });
        located.unwrap_or(crate::AckError::unlocated(code, application_code))
    }

    /// Returns the triggered-message MSA outcome, or none for failed responses/query processing.
    /// `QueryRejected` must use QAK; invalid acknowledgments must not start an ACK loop.
    #[must_use]
    pub fn acknowledgment_code(&self) -> Option<AckCode> {
        match self {
            Self::MalformedTrigger(_) => Some(AckCode::Ae),
            Self::ContentReject(_) => Some(AckCode::Ar),
            Self::InvalidAcknowledgment(_)
            | Self::InconsistentOrl(_)
            | Self::QueryRejected { .. } => None,
        }
    }
}

fn triggered(error: Error) -> Failure {
    let reject = match error.base() {
        Error::Component { segment: "MSH", .. }
        | Error::Inconsistent { .. }
        | Error::Content {
            segment: "MSH",
            field: 9 | 12 | 15 | 16 | 18 | 21,
        }
        | Error::Syntax(crate::Error::Control {
            segment: "MSH",
            field: 9 | 12 | 15 | 16 | 18 | 21,
        }) => true,
        Error::AtSegment { .. }
        | Error::DilutionDefinition { .. }
        | Error::Subcomponent { .. }
        | Error::Authority { .. }
        | Error::Component { .. }
        | Error::Content { .. }
        | Error::Syntax(_) => false,
    };
    if reject {
        Failure::ContentReject(error)
    } else {
        Failure::MalformedTrigger(error)
    }
}

/// Validates a query and classifies trigger errors for the host's recovery policy.
/// # Errors
/// Returns control-content AR versus malformed-trigger AE with source locations.
pub fn receive_query(message: &Message) -> Result<Query<'_>, Failure> {
    Query::new(message).map_err(triggered)
}

/// Validates results atomically and distinguishes inconsistent content from malformed input.
/// # Errors
/// Returns AR for a TCD/OBX inconsistency; no partial observations are returned.
pub fn receive_results(message: &Message) -> Result<Results<'_>, Failure> {
    Results::new(message).map_err(triggered)
}

/// Receives an ORL; any inconsistency fails this exchange without altering prior acceptance.
/// # Errors
/// Returns `InconsistentOrl` with the original validation error and its location.
pub fn receive_work_response<'a>(
    message: &'a Message,
    sent: &'a Message,
) -> Result<WorkResponse<'a>, Failure> {
    WorkResponse::new(message, sent).map_err(Failure::InconsistentOrl)
}

/// Checks a generic ACK response, including source message correlation and response grammar.
/// # Errors
/// Invalid acknowledgments are exchange failures, never new triggered messages to ACK.
pub fn receive_acknowledgment(message: &Message, sent: &Message) -> Result<AckCode, Failure> {
    let validate = || -> Result<AckCode, Error> {
        super::validate_control(message, super::ControlDirection::Receive)?;
        let incoming = super::msh(message)?;
        let outgoing = super::msh(sent)?;
        let d = message.delimiters();
        let trigger = outgoing.message_type().nth(1).ok_or(content("MSH", 9))?;
        if !raw(incoming.segment(), 9)
            .split(|b| *b == d.component)
            .take(3)
            .eq([b"ACK".as_slice(), trigger, b"ACK"])
        {
            return Err(content("MSH", 9));
        }
        let mut segments = message.segments().skip(1);
        let msa_segment = segments.next().ok_or(content("MSA", 0))?;
        let msa = Msa::from_law(msa_segment)?;
        if msa.control_id() != outgoing.control_id() {
            return Err(content("MSA", 2));
        }
        super::envelope::received_segment(msa_segment)?;
        let mut errors = 0usize;
        for segment in segments {
            if segment.id() != b"ERR" {
                return Err(content("ACK", 0));
            }
            super::envelope::received_segment(segment)?;
            errors = errors.checked_add(1).ok_or(content("ERR", 0))?;
        }
        if (msa.code() == AckCode::Aa) == (errors != 0) {
            return Err(content("ERR", 0));
        }
        Ok(msa.code())
    };
    validate().map_err(Failure::InvalidAcknowledgment)
}
