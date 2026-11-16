//! Correlates LAW work responses and retains analyzer AWOS decisions.
use super::{Error, Identity, WorkAction, content};
use crate::law;
use crate::{Message, Msa};
use std::collections::{BTreeMap, BTreeSet};
/// Analyzer's per-request AWOS decision; never a message acknowledgment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AwosOutcome {
    /// New work accepted.
    Accepted,
    /// This request rejected, including a duplicate of previously accepted work.
    UnableToAccept,
    /// Cancellation confirmed.
    CancelAccepted,
    /// Cancellation refused; work may still complete.
    UnableToCancel,
}

/// A scoped decision from an ORL; the prior work lifecycle is not mutated.
#[derive(Debug, Clone, Copy)]
pub struct AwosDecision<'a> {
    /// AWOS identity, including its namespace.
    pub id: Identity<'a>,
    /// Request-specific outcome.
    pub outcome: AwosOutcome,
    /// Typed analyzer work status; original bytes remain in the message.
    pub status: law::OrderStatus,
}

/// Validated ORL response with message acceptance separate from AWOS decisions.
#[derive(Debug)]
pub struct WorkResponse<'a> {
    message: &'a Message,
    acceptance: crate::AckCode,
    decisions: Vec<AwosDecision<'a>>,
}

impl<'a> WorkResponse<'a> {
    /// Validates correlation and one response for every requested AWOS.
    /// # Errors
    /// Rejects inconsistent/missing/duplicate/unknown AWOS or mismatched control/status pairs.
    pub fn new(message: &'a Message, sent: &'a Message) -> Result<Self, Error> {
        Self::with_profile(message, sent, law::Profile::receive())
    }

    /// Validates a work response using negotiated options and trusted authority bindings.
    /// # Errors
    /// Returns located profile failures or an inconsistent work-response error.
    pub fn with_profile(
        message: &'a Message,
        sent: &'a Message,
        mut profile: law::Profile<'_>,
    ) -> Result<Self, Error> {
        profile.context = law::profile::MessageContext::Work;
        profile.validate_message(message, law::ControlDirection::Receive)?;
        law::profile::header(message, b"LAB-28", false)?;
        let msh = law::msh(message)?;
        let d = message.delimiters();
        if !law::raw(msh.segment(), 9)
            .split(|b| *b == d.component)
            .take(3)
            .eq([b"ORL".as_slice(), b"O34", b"ORL_O42"])
        {
            return Err(content("MSH", 9));
        }
        let mut segments = message.segments().skip(1);
        let msa_segment = segments
            .next()
            .filter(|s| s.id() == b"MSA")
            .ok_or(content("MSA", 0))?;
        let msa = Msa::from_law(msa_segment)?;
        if msa.control_id() != law::msh(sent)?.control_id() {
            return Err(content("MSA", 2));
        }
        let expected: Vec<_> = sent
            .segments()
            .filter(|s| s.id() == b"OBR")
            .map(|s| law::raw(s, 2))
            .collect();
        let requests = sent_requests(sent)?;
        let mut decisions = Vec::new();
        let mut has_specimen = false;
        let mut response_specimen = b"".as_slice();
        let mut response_container = b"".as_slice();
        let mut error_count = 0usize;
        let mut seen = BTreeSet::new();
        profile.validate_segment(msa_segment, law::ControlDirection::Receive)?;
        for s in segments {
            profile.validate_segment(s, law::ControlDirection::Receive)?;
            match s.id() {
                b"ERR" => {
                    if has_specimen {
                        return Err(content("ERR", 0));
                    }
                    error_count = error_count.checked_add(1).ok_or(content("ERR", 0))?;
                    continue;
                }
                b"PID" => {
                    if has_specimen || !seen.is_empty() {
                        return Err(content("PID", 0));
                    }
                    continue;
                }
                b"SPM" => {
                    has_specimen = true;
                    response_specimen = law::raw(s, 2);
                    response_container = b"";
                    continue;
                }
                b"SAC" => {
                    if !has_specimen {
                        return Err(content("SAC", 0));
                    }
                    response_container = law::raw(s, 3);
                    continue;
                }
                b"ORC" => {
                    if !has_specimen || error_count != 0 {
                        return Err(content("ORC", 0));
                    }
                }
                _ => return Err(content("ORL", 0)),
            }

            decisions.push(parse_decision(
                s,
                &expected,
                &mut seen,
                &requests,
                response_specimen,
                response_container,
            )?);
        }
        if (msa.code() == crate::AckCode::Aa) == (error_count != 0) {
            return Err(content("ERR", 0));
        }
        if msa.code() == crate::AckCode::Aa && seen.len() != expected.len() {
            return Err(content("ORC", 0));
        }
        if msa.code() != crate::AckCode::Aa && !decisions.is_empty() {
            return Err(content("ORC", 0));
        }
        Ok(Self {
            message,
            acceptance: msa.code(),
            decisions,
        })
    }

    /// Returns message-level acceptance, independently of AWOS outcomes.
    #[must_use]
    pub fn acceptance(&self) -> crate::AckCode {
        self.acceptance
    }

    /// Returns request-specific decisions; an unable-to-accept never retracts prior acceptance.
    #[must_use]
    pub fn decisions(&self) -> &[AwosDecision<'a>] {
        &self.decisions
    }

    /// Returns the untouched input for durable retention and reconciliation.
    #[must_use]
    pub fn message(&self) -> &'a Message {
        self.message
    }
}

fn response_outcome(
    control: law::OrderControl,
    status: law::OrderStatus,
) -> Result<AwosOutcome, Error> {
    use law::{OrderControl, OrderStatus};
    match control {
        OrderControl::Accepted => match status {
            OrderStatus::Scheduled | OrderStatus::InProgress | OrderStatus::Complete => {
                Ok(AwosOutcome::Accepted)
            }
            OrderStatus::Cancelled => Err(content("ORC", 5)),
        },
        OrderControl::UnableToAccept => match status {
            OrderStatus::Cancelled => Ok(AwosOutcome::UnableToAccept),
            OrderStatus::Scheduled | OrderStatus::InProgress | OrderStatus::Complete => {
                Err(content("ORC", 5))
            }
        },
        OrderControl::Cancelled => match status {
            OrderStatus::Cancelled => Ok(AwosOutcome::CancelAccepted),
            OrderStatus::Scheduled | OrderStatus::InProgress | OrderStatus::Complete => {
                Err(content("ORC", 5))
            }
        },
        OrderControl::UnableToCancel => match status {
            OrderStatus::InProgress | OrderStatus::Complete => Ok(AwosOutcome::UnableToCancel),
            OrderStatus::Scheduled | OrderStatus::Cancelled => Err(content("ORC", 5)),
        },
        OrderControl::New
        | OrderControl::Cancel
        | OrderControl::Discontinue
        | OrderControl::StatusChange => Err(content("ORC", 1)),
    }
}

type Requests<'a> = BTreeMap<&'a [u8], (WorkAction, &'a [u8], &'a [u8])>;
fn sent_requests(sent: &Message) -> Result<Requests<'_>, Error> {
    let mut requests = BTreeMap::new();
    let mut action = None;
    let mut source_specimen = b"".as_slice();
    let mut source_container = b"".as_slice();
    for segment in sent.segments() {
        match segment.id() {
            b"SPM" => {
                source_specimen = law::raw(segment, 2);
                source_container = b"";
                action = None;
            }
            b"SAC" => {
                source_container = law::raw(segment, 3);
            }
            b"ORC" => {
                action = match law::OrderControl::parse(law::raw(segment, 1))? {
                    law::OrderControl::New => Some(WorkAction::New),
                    law::OrderControl::Cancel => Some(WorkAction::Cancel),
                    law::OrderControl::Discontinue => None,
                    law::OrderControl::Accepted
                    | law::OrderControl::UnableToAccept
                    | law::OrderControl::Cancelled
                    | law::OrderControl::UnableToCancel
                    | law::OrderControl::StatusChange => return Err(content("ORC", 1)),
                };
            }
            b"OBR" => {
                let action = action.ok_or(content("ORC", 1))?;
                if requests
                    .insert(
                        law::raw(segment, 2),
                        (action, source_specimen, source_container),
                    )
                    .is_some()
                {
                    return Err(content("OBR", 2));
                }
            }
            _ => {}
        }
    }
    Ok(requests)
}

fn parse_decision<'a>(
    s: crate::Segment<'a>,
    expected: &[&[u8]],
    seen: &mut BTreeSet<&'a [u8]>,
    requests: &Requests<'_>,
    response_specimen: &[u8],
    response_container: &[u8],
) -> Result<AwosDecision<'a>, Error> {
    let id = law::raw(s, 2);
    if !expected.contains(&id) || !seen.insert(id) {
        return Err(content("ORC", 2));
    }
    let (_, expected_specimen, expected_container) = requests.get(id).ok_or(content("ORC", 2))?;
    if !response_specimen.is_empty() && response_specimen != *expected_specimen {
        return Err(Error::Inconsistent {
            segment: "SPM",
            field: 2,
        });
    }
    if !response_container.is_empty() && response_container != *expected_container {
        return Err(Error::Inconsistent {
            segment: "SAC",
            field: 3,
        });
    }
    let status = law::OrderStatus::parse(law::raw(s, 5))?;
    let outcome = response_outcome(law::OrderControl::parse(law::raw(s, 1))?, status)?;
    match (requests.get(id).map(|request| &request.0), outcome) {
        (Some(WorkAction::New), AwosOutcome::Accepted | AwosOutcome::UnableToAccept)
        | (Some(WorkAction::Cancel), AwosOutcome::CancelAccepted | AwosOutcome::UnableToCancel) => {
        }
        (Some(WorkAction::New), AwosOutcome::CancelAccepted | AwosOutcome::UnableToCancel)
        | (Some(WorkAction::Cancel), AwosOutcome::Accepted | AwosOutcome::UnableToAccept)
        | (
            None,
            AwosOutcome::Accepted
            | AwosOutcome::UnableToAccept
            | AwosOutcome::CancelAccepted
            | AwosOutcome::UnableToCancel,
        ) => return Err(content("ORC", 1)),
    }
    Ok(AwosDecision {
        id: Identity(id),
        outcome,
        status,
    })
}
