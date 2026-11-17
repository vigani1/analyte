//! Runs deterministic LAW exchanges and deadlines (TF-2b 3.27-3.29, B.6/B.7).
//! Kept whole: The LAW conversation state transitions and their deadline decisions are reviewed together.
/// Operation whose deadline elapsed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeadlineKind {
    /// Deadline for LAW independent transaction deadline.
    IndependentTransaction,
    /// Deadline for LAW receive deadline expired.
    Receive,
    /// Deadline for LAW response send deadline expired.
    ResponseSend,
    /// Deadline for LAW processing deadline expired; no acceptance promised.
    Processing,
    /// Deadline for LAW exchange closed.
    Closed,
    /// Deadline for LAW outbound work expired before issuance.
    OutboundWork,
}
impl DeadlineKind {
    /// Stable operation text for display and retained evidence.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::IndependentTransaction => "LAW independent transaction deadline",
            Self::Receive => "LAW receive deadline expired",
            Self::ResponseSend => "LAW response send deadline expired",
            Self::Processing => "LAW processing deadline expired; no acceptance promised",
            Self::Closed => "LAW exchange closed",
            Self::OutboundWork => "LAW outbound work expired before issuance",
        }
    }
}
impl std::fmt::Display for DeadlineKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

use analyte_framing::hl7 as mllp;
use analyte_hl7::OwnedAckError as Detail;
use analyte_hl7::{AckCode, AckError, Message, Options, Response, Responsibility, law};
use std::{collections::VecDeque, time::Duration};
use thiserror::Error;

mod failure;
pub use crate::ExchangeId;
/// Link role; independent outbound links accept only the correlated ORL transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Link {
    /// Persistent link or an independent inbound analyzer connection.
    Inbound,
    /// One manager-initiated independent LAB-28 connection.
    WorkResponse,
}
/// A query response and its mandatory followup remain one protocol decision.
#[derive(Debug, Clone)]
pub enum Reply {
    /// Response without work on this connection.
    Response(Message),
    /// Availability followed by work, in this order.
    Followup {
        /// Availability response.
        response: Message,
        /// Durably reserved work.
        work: Message,
    },
}
/// Source location supplied by the host's interpretation, without choosing an ACK.
#[derive(Debug, Clone, Copy)]
pub struct Location<'a> {
    /// Source segment identifier.
    pub segment: &'a [u8],
    /// One-based occurrence.
    pub occurrence: u16,
    /// One-based field.
    pub field: u16,
}
impl Location<'_> {
    fn detail<'a>(&'a self, code: &'a [u8], application: &'a [u8]) -> AckError<'a> {
        AckError {
            segment: self.segment,
            occurrence: self.occurrence,
            field: self.field,
            code,
            application_code: application,
        }
    }
}
/// Durable interpretation outcome, independent of acknowledgment policy.
#[derive(Debug, Clone, Copy)]
pub enum Interpretation<'a> {
    /// Stored interpretation accepted the batch.
    Accepted,
    /// Retained work contradicts the received result.
    WorkConflict(Location<'a>),
    /// Identity/revision conflict needs operator review.
    Conflict,
    /// Stored interpretation could not map the batch.
    Failed(Option<Location<'a>>),
}
/// Host and wire input. Cancellation never grants permission to replay an order.
#[derive(Debug, Clone, Copy)]
pub enum Event<'a> {
    /// One received byte.
    Byte(u8),
    /// Advance deadlines before accepting another event.
    Tick,
    /// Exact raw receipt is durably stored.
    Stored(ExchangeId),
    /// Durable custody failed; no positive reply is permitted.
    StoreFailed(ExchangeId),
    /// Host completed a transaction that does not need a response.
    Complete(ExchangeId),
    /// Host finished durable interpretation.
    Interpreted(ExchangeId, Interpretation<'a>),
    /// Fresh host-generated response metadata.
    Respond(ExchangeId, Response<'a>),
    /// Query service supplied its response and a reservation deadline.
    Reply(ExchangeId, &'a Reply, Option<Duration>),
    /// Explicit manager-initiated work and its admission deadline.
    Send(&'a Message, Duration),
    /// Installs the retained work exchange's post-issuance deadline on an independent link.
    WorkDeadline(Duration),
    /// Transport queued the requested write; not proof of peer acceptance.
    Written,
    /// Connection ended; preserve all pending evidence.
    Disconnected,
}
/// Ordered effects. Payloads belong only in the protected store or transport.
#[derive(Debug)]
pub enum Action {
    /// Commit exact framed bytes before sending `Stored`.
    Store(ExchangeId, Vec<u8>),
    /// Query cache lookup may now run, using the retained first-byte time.
    Query(ExchangeId, Message, Duration),
    /// Reconcile this durable ORL against retained work; never acknowledge a response.
    WorkResponse(ExchangeId, Message),
    /// Interpret the durable receipt before selecting a result ACK.
    Interpret(ExchangeId),
    /// Retain a specific interpretation/syntax failure before any reply.
    Parked(ExchangeId, Failure),
    /// Supply metadata for the dialog-selected ACK.
    ResponseNeeded(ExchangeId),
    /// Exact framed bytes and absolute connection-clock deadline.
    Write(crate::Write),
    /// All work bytes were issued; the host must retain its reservation until ORL.
    WorkIssued,
    /// The current received transaction is finished.
    Done,
    /// Rejected or incomplete wire evidence.
    Rejected(mllp::Error, Vec<u8>),
    /// A complete block whose commit confirmation was not driven before interruption.
    Uncommitted(ExchangeId, Vec<u8>),
    /// Deadline expiry preceded this byte; retain it with the unread transport tail.
    Unconsumed(u8),
    /// Fatal refusal; earlier effects must still be consumed.
    Refused(Error),
    /// Next absolute connection-clock deadline.
    Deadline(Option<Duration>),
}
/// Specific protocol/control failures without payloads in their diagnostics.
#[derive(Debug, Error)]
pub enum Error {
    /// Timer bounds are unusable.
    #[error("invalid LAW timer configuration")]
    Configuration,
    /// The caller clock moved backwards.
    #[error("LAW time reversed")]
    TimeReversed,
    /// A deadline or ordinal cannot be represented.
    #[error("LAW time or identity exhausted")]
    Exhausted,
    /// Host event does not identify the active phase.
    #[error("stale LAW host response")]
    Stale,
    /// An exchange is already active.
    #[error("LAW exchange busy")]
    Busy,
    /// Named exchange deadline expired.
    #[error("{0}")]
    Deadline(DeadlineKind),
    /// Storage refused custody.
    #[error("LAW durable commit refused")]
    NotStored,
    /// An independent outbound link received another transaction.
    #[error("LAW outbound link received a non-ORL message")]
    NotOrl,
    /// Invalid framing with exact evidence emitted separately.
    #[error(transparent)]
    Framing(#[from] mllp::Error),
    /// HL7 wire syntax failure.
    #[error(transparent)]
    Syntax(std::sync::Arc<analyte_hl7::Error>),
    /// LAW profile validation failure.
    #[error(transparent)]
    Profile(#[from] law::Error),
    /// A rejected trigger cannot be acknowledged safely.
    #[error("LAW failure has no ACK outcome")]
    NoOutcome,
}
#[derive(Debug)]
enum Plan {
    Reject(Vec<u8>, AckCode, Detail),
    Result(Message, AckCode, Responsibility, Option<Detail>),
}
#[derive(Debug)]
enum Completion {
    Reply(bool),
    Work,
}
#[derive(Debug)]
enum State {
    Idle,
    Store(ExchangeId, Vec<u8>),
    Query(ExchangeId),
    WorkResponse(ExchangeId),
    Interpret(ExchangeId, Message),
    Respond(ExchangeId, Plan),
    Writing {
        remaining: VecDeque<Message>,
        until: Duration,
        send_until: Duration,
        completion: Completion,
    },
    AwaitDeadline,
    Closed,
}
/// One connection's bounded state machine. Dropping it leaves durable/sent work
/// owned by the host; no timeout or disconnect authorizes automatic resend.
#[derive(Debug)]
pub struct Dialog {
    options: Options,
    limit: mllp::Limit,
    receive: Duration,
    send: Duration,
    link: Link,
    decoder: mllp::Decoder,
    state: State,
    clock: crate::Clock,
    started: Option<Duration>,
    until: Option<Duration>,
    next: u64,
}
impl Dialog {
    /// Starts an idle link with explicit first-byte and send budgets.
    /// # Errors
    /// Rejects zero timers or an invalid framing bound.
    pub fn new(
        options: Options,
        receive: Duration,
        send: Duration,
        link: Link,
    ) -> Result<Self, Error> {
        if receive.is_zero() || send.is_zero() {
            return Err(Error::Configuration);
        }
        let limit = mllp::Limit::new(options.limits.message_bytes)?;
        Ok(Self {
            options,
            limit,
            receive,
            send,
            link,
            decoder: mllp::Decoder::new(limit),
            state: State::Idle,
            clock: crate::Clock::default(),
            started: None,
            until: None,
            next: 0,
        })
    }
    /// Current deadline, including an outstanding transport issuance.
    #[must_use]
    pub fn deadline(&self) -> Option<Duration> {
        if let State::Writing { send_until, .. } = self.state {
            Some(send_until)
        } else {
            self.until
        }
    }
    /// Returns the next absolute connection-clock wake time, including issuance.
    #[must_use]
    pub fn next_deadline(&self) -> Option<Duration> {
        self.deadline()
    }

    /// Whether a partial received block prevents initiating new work.
    #[must_use]
    pub fn receiving(&self) -> bool {
        !self.decoder.pending().is_empty()
    }
    /// Advances deadlines before input and returns ordered effects.
    /// # Errors
    /// Rejects reversed time before mutation. Fatal protocol failures are actions.
    pub fn step(&mut self, now: Duration, event: Event<'_>) -> Result<Vec<Action>, Error> {
        let headroom = self
            .receive
            .checked_add(self.send)
            .ok_or(Error::Exhausted)?;
        self.clock
            .advance(now, headroom)
            .map_err(|error| error.into_dialog(Error::TimeReversed, Error::Exhausted))?;
        let mut out = Vec::new();
        if self.deadline().is_some_and(|until| now >= until)
            && !matches!(event, Event::Disconnected)
        {
            let reason = match self.state {
                State::Idle => {
                    if self.link == Link::WorkResponse {
                        DeadlineKind::IndependentTransaction
                    } else {
                        DeadlineKind::Receive
                    }
                }
                State::Writing { .. } => DeadlineKind::ResponseSend,
                State::Store(..)
                | State::Query(_)
                | State::WorkResponse(_)
                | State::Interpret(..)
                | State::Respond(..)
                | State::AwaitDeadline => DeadlineKind::Processing,
                State::Closed => DeadlineKind::Closed,
            };
            if let Event::Byte(byte) = event {
                out.push(Action::Unconsumed(byte));
            }
            self.finish_wire(&mut out);
            self.state = State::Closed;
            self.until = None;
            out.push(Action::Refused(Error::Deadline(reason)));
        } else if let Err(error) = self.input(event, &mut out) {
            out.push(Action::Refused(error));
        }
        out.push(Action::Deadline(self.deadline()));
        Ok(out)
    }
    fn receive_byte(&mut self, byte: u8, out: &mut Vec<Action>) -> Result<(), Error> {
        if !matches!(self.state, State::Idle) {
            return Err(Error::Busy);
        }
        self.started.get_or_insert(self.clock.now());
        if self.until.is_none() {
            self.until = Some(
                self.clock
                    .now()
                    .checked_add(self.receive)
                    .ok_or(Error::Exhausted)?,
            );
        }
        if let Some(event) = self.decoder.push(byte) {
            match event {
                mllp::Event::Rejected { raw, error } => {
                    out.push(Action::Rejected(error.clone(), raw));
                    return Err(error.into());
                }
                mllp::Event::Block(raw) => {
                    let id = ExchangeId(0, self.next);
                    let Some(next) = self.next.checked_add(1) else {
                        out.push(Action::Uncommitted(id, raw));
                        return Err(Error::Exhausted);
                    };
                    self.next = next;
                    self.state = State::Store(id, raw.clone());
                    out.push(Action::Store(id, raw));
                }
            }
        }
        Ok(())
    }

    fn reply(
        &mut self,
        id: ExchangeId,
        reply: &Reply,
        reserved_until: Option<Duration>,
        out: &mut Vec<Action>,
    ) -> Result<(), Error> {
        if !matches!(self.state, State::Query(expected) if expected == id) {
            return Err(Error::Stale);
        }
        let (messages, followup) = match reply {
            Reply::Response(message) => (vec![message], false),
            Reply::Followup { response, work } => (vec![response, work], true),
        };
        if messages
            .iter()
            .any(|message| message.raw().len() > self.options.limits.message_bytes)
        {
            return Err(mllp::Error::TooLong.into());
        }
        let messages: VecDeque<_> = messages.into_iter().cloned().collect();
        let until = self.until.ok_or(Error::Stale)?;
        let until = reserved_until.map_or(until, |due| due.min(until));
        self.write(messages, until, Completion::Reply(followup), out)?;
        Ok(())
    }

    fn input(&mut self, event: Event<'_>, out: &mut Vec<Action>) -> Result<(), Error> {
        match event {
            Event::Tick => {}
            Event::Disconnected => {
                self.finish_wire(out);
                self.state = State::Closed;
                self.until = None;
            }
            Event::Byte(byte) => {
                self.receive_byte(byte, out)?;
            }
            Event::Stored(id) => self.committed(id, out)?,
            Event::StoreFailed(id) => {
                if !matches!(self.state, State::Store(expected, _) if expected == id) {
                    return Err(Error::Stale);
                }
                self.state = State::Closed;
                self.until = None;
                return Err(Error::NotStored);
            }
            Event::Complete(id) => {
                if !matches!(self.state, State::WorkResponse(expected) if expected == id) {
                    return Err(Error::Stale);
                }
                self.done(out);
            }
            Event::Interpreted(id, outcome) => self.interpreted(id, outcome, out)?,
            Event::Respond(id, response) => self.respond(id, response, out)?,
            Event::Reply(id, reply, reserved_until) => {
                self.reply(id, reply, reserved_until, out)?;
            }
            Event::Send(message, until) => {
                if self.link == Link::WorkResponse && self.clock.now() >= until {
                    return Err(Error::Deadline(DeadlineKind::OutboundWork));
                }
                if !matches!(self.state, State::Idle) || self.receiving() {
                    return Err(Error::Busy);
                }
                if message.raw().len() > self.options.limits.message_bytes {
                    return Err(mllp::Error::TooLong.into());
                }
                self.until = Some(until);
                self.write(vec![message.clone()].into(), until, Completion::Work, out)?;
            }
            Event::WorkDeadline(until) => {
                if self.link != Link::WorkResponse || !matches!(self.state, State::AwaitDeadline) {
                    return Err(Error::Stale);
                }
                self.state = State::Idle;
                self.until = Some(until);
            }
            Event::Written => {
                let state = std::mem::replace(&mut self.state, State::Idle);
                if let State::Writing {
                    remaining,
                    until,
                    completion,
                    ..
                } = state
                {
                    self.write(remaining, until, completion, out)?;
                } else {
                    self.state = state;
                    return Err(Error::Stale);
                }
            }
        }
        Ok(())
    }
    fn done(&mut self, out: &mut Vec<Action>) {
        self.state = State::Idle;
        if self.link == Link::Inbound {
            self.until = None;
        }
        self.started = None;
        out.push(Action::Done);
    }
    fn finish_wire(&mut self, out: &mut Vec<Action>) {
        if let State::Store(id, raw) = &self.state {
            out.push(Action::Uncommitted(*id, raw.clone()));
        }
        if let Some(mllp::Event::Rejected { raw, error }) = self.decoder.finish() {
            out.push(Action::Rejected(error, raw));
        }
    }
    fn write(
        &mut self,
        mut remaining: VecDeque<Message>,
        until: Duration,
        completion: Completion,
        out: &mut Vec<Action>,
    ) -> Result<(), Error> {
        if let Some(message) = remaining.pop_front() {
            let send_until = match completion {
                Completion::Work => until,
                Completion::Reply(_) => until.min(
                    self.clock
                        .now()
                        .checked_add(self.send)
                        .ok_or(Error::Exhausted)?,
                ),
            };
            out.push(Action::Write(crate::Write::encode(
                message.raw(),
                self.limit,
                Some(send_until),
            )?));
            self.state = State::Writing {
                remaining,
                until,
                send_until,
                completion,
            };
        } else {
            match completion {
                Completion::Reply(followup) => {
                    self.done(out);
                    if followup {
                        out.push(Action::WorkIssued);
                    }
                }
                Completion::Work => {
                    self.state = if self.link == Link::WorkResponse {
                        State::AwaitDeadline
                    } else {
                        State::Idle
                    };
                    self.until = None;
                    out.push(Action::WorkIssued);
                }
            }
        }
        Ok(())
    }
    fn committed(&mut self, id: ExchangeId, out: &mut Vec<Action>) -> Result<(), Error> {
        let state = std::mem::replace(&mut self.state, State::Idle);
        let State::Store(expected, raw) = state else {
            self.state = state;
            return Err(Error::Stale);
        };
        if id != expected {
            self.state = State::Store(expected, raw);
            return Err(Error::Stale);
        }
        let payload = mllp::payload(&raw).ok_or(mllp::Error::Incomplete)?;
        let message = match law::parse_message(payload, self.options) {
            Ok(message) => message,
            Err(error) => {
                return self.parse_failed(id, payload, error, out);
            }
        };
        let kind: Vec<_> = law::message_type(&message).take(4).collect();
        if self.link == Link::WorkResponse && kind != [b"ORL".as_slice(), b"O34", b"ORL_O42"] {
            out.push(Action::Parked(id, Failure::NotOrl));
            return Err(Error::NotOrl);
        }
        if kind == [b"QBP".as_slice(), b"Q11", b"QBP_Q11"] {
            self.state = State::Query(id);
            out.push(Action::Query(
                id,
                message,
                self.started.ok_or(Error::Stale)?,
            ));
        } else if kind == [b"ORL".as_slice(), b"O34", b"ORL_O42"] {
            self.state = State::WorkResponse(id);
            out.push(Action::WorkResponse(id, message));
        } else if kind != [b"OUL".as_slice(), b"R22", b"OUL_R22"] {
            out.push(Action::Parked(id, Failure::DisabledTransaction));
            self.state = State::Respond(
                id,
                Plan::Reject(
                    payload.to_vec(),
                    AckCode::Ar,
                    header_error(9, b"200", b"LAW-unsupported-trigger").into(),
                ),
            );
            out.push(Action::ResponseNeeded(id));
        } else {
            match law::receive_results(&message) {
                Ok(_) => {
                    self.state = State::Interpret(id, message);
                    out.push(Action::Interpret(id));
                }
                Err(failure) => {
                    let code = failure.acknowledgment_code().ok_or(Error::NoOutcome)?;
                    let plan = if law::validate_control(&message, law::ControlDirection::Receive)
                        .is_err()
                    {
                        Plan::Reject(
                            payload.to_vec(),
                            code,
                            failure
                                .acknowledgment_error(&message, b"102", b"LAW-control")
                                .into(),
                        )
                    } else {
                        let detail = failure
                            .acknowledgment_error(&message, b"102", b"LAW-invalid-results")
                            .into();
                        Plan::Result(message, code, Responsibility::Stored, Some(detail))
                    };
                    out.push(Action::Parked(id, Failure::Profile(failure)));
                    self.state = State::Respond(id, plan);
                    out.push(Action::ResponseNeeded(id));
                }
            }
        }
        Ok(())
    }
    fn interpreted(
        &mut self,
        id: ExchangeId,
        outcome: Interpretation<'_>,
        out: &mut Vec<Action>,
    ) -> Result<(), Error> {
        let location = match outcome {
            Interpretation::WorkConflict(location) | Interpretation::Failed(Some(location)) => {
                Some(location)
            }
            Interpretation::Accepted | Interpretation::Conflict | Interpretation::Failed(None) => {
                None
            }
        };
        if location.is_some_and(|location| location.segment.len() > 3) {
            return Err(Error::Configuration);
        }
        let state = std::mem::replace(&mut self.state, State::Idle);
        let State::Interpret(expected, message) = state else {
            self.state = state;
            return Err(Error::Stale);
        };
        if expected != id {
            self.state = State::Interpret(expected, message);
            return Err(Error::Stale);
        }
        let (code, responsibility, detail) = match outcome {
            Interpretation::Accepted => (AckCode::Aa, Responsibility::StoredAndInterpreted, None),
            Interpretation::WorkConflict(location) => (
                AckCode::Ar,
                Responsibility::Stored,
                Some(location.detail(b"207", b"LAW-inconsistent-work").into()),
            ),
            Interpretation::Conflict => (
                AckCode::Ar,
                Responsibility::Stored,
                Some(AckError::unlocated(b"207", b"operator-review").into()),
            ),
            Interpretation::Failed(location) => (
                AckCode::Ae,
                Responsibility::Stored,
                Some(
                    location
                        .as_ref()
                        .map_or(
                            AckError::unlocated(b"207", b"operator-review"),
                            |location| location.detail(b"207", b"operator-review"),
                        )
                        .into(),
                ),
            ),
        };
        self.state = State::Respond(id, Plan::Result(message, code, responsibility, detail));
        out.push(Action::ResponseNeeded(id));
        Ok(())
    }
    fn respond(
        &mut self,
        id: ExchangeId,
        response: Response<'_>,
        out: &mut Vec<Action>,
    ) -> Result<(), Error> {
        let state = std::mem::replace(&mut self.state, State::Idle);
        let State::Respond(expected, plan) = state else {
            self.state = state;
            return Err(Error::Stale);
        };
        if expected != id {
            self.state = State::Respond(expected, plan);
            return Err(Error::Stale);
        }
        let message = match plan {
            Plan::Reject(raw, code, detail) => {
                law::reject_trigger(&raw, response, code, detail.view(), self.options)?
            }
            Plan::Result(message, code, responsibility, detail) => law::result_acknowledgment(
                &message,
                response,
                code,
                responsibility,
                detail.as_ref().map(Detail::view),
                self.options,
            )?,
        };
        if let Some(message) = message {
            self.write(
                vec![message].into(),
                self.until.ok_or(Error::Stale)?,
                Completion::Reply(false),
                out,
            )?;
        } else {
            self.done(out);
        }
        Ok(())
    }
}
fn header_error<'a>(field: u16, code: &'a [u8], application_code: &'a [u8]) -> AckError<'a> {
    AckError {
        segment: b"MSH",
        occurrence: 1,
        field,
        code,
        application_code,
    }
}

#[derive(Debug)]
enum WorkPhase {
    Prepared(Duration),
    Awaiting(Duration),
}
/// Retained work and its admission/ORL deadlines, transferable between link owners.
#[derive(Debug)]
pub struct WorkExchange {
    sent: Message,
    phase: WorkPhase,
}
impl WorkExchange {
    /// Retains the exact outgoing command with its already-budgeted admission deadline.
    #[must_use]
    pub fn new(sent: Message, until: Duration) -> Self {
        Self {
            sent,
            phase: WorkPhase::Prepared(until),
        }
    }
    /// Current absolute deadline in the reservation owner's clock.
    #[must_use]
    pub fn deadline(&self) -> Duration {
        match self.phase {
            WorkPhase::Prepared(until) | WorkPhase::Awaiting(until) => until,
        }
    }
    /// Whether the host must reconcile this exchange as uncertain.
    #[must_use]
    pub fn expired(&self, now: Duration) -> bool {
        now >= self.deadline()
    }
    /// Starts the response budget only after actual work issuance.
    /// # Errors
    /// Rejects repeated issuance, zero waits or unrepresentable deadlines.
    pub fn issued(&mut self, now: Duration, wait: Duration) -> Result<(), Error> {
        if !matches!(self.phase, WorkPhase::Prepared(_)) {
            return Err(Error::Stale);
        }
        if wait.is_zero() {
            return Err(Error::Configuration);
        }
        self.phase = WorkPhase::Awaiting(now.checked_add(wait).ok_or(Error::Exhausted)?);
        Ok(())
    }
    /// Validates a reply against the retained command without changing its identities.
    /// # Errors
    /// Preserves the LAW response correlation and per-AWOS validation failure.
    pub fn response<'a>(
        &'a self,
        message: &'a Message,
    ) -> Result<law::WorkResponse<'a>, law::Failure> {
        law::receive_work_response(message, &self.sent)
    }
}

/// Specific retained failure from a complete LAW exchange.
#[derive(Debug, Error)]
pub enum Failure {
    /// Syntax could not be interpreted.
    #[error(transparent)]
    Syntax(std::sync::Arc<analyte_hl7::Error>),
    /// The LAW transaction violated its profile.
    #[error(transparent)]
    Profile(#[from] law::Failure),
    /// An outbound link accepts only its correlated response.
    #[error("LAW outbound link received a non-ORL message")]
    NotOrl,
    /// Unsolicited work is not enabled on this link.
    #[error("LAW transaction disabled: unsolicited work/response traffic is not enabled")]
    DisabledTransaction,
}

impl From<analyte_hl7::Error> for Error {
    fn from(error: analyte_hl7::Error) -> Self {
        Self::Syntax(std::sync::Arc::new(error))
    }
}
