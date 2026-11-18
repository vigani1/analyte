//! Runs HL7 acknowledgment conversations over MLLP with documented evidence (HL7 2.5.1 2.9.2/2.9.3).
//! Kept whole: The HL7 conversation state transitions share connection ownership and deadline invariants.
use analyte_framing::hl7 as mllp;
use analyte_hl7::OwnedAckError as OwnedError;
use analyte_hl7::{
    AckCode, AckError, AckMode, Condition, Message, Msa, Msh, Options, Response, Responsibility,
};
use std::{collections::VecDeque, time::Duration};
use thiserror::Error;
mod orders;
use orders::Orders;
pub use orders::{DisplayReply, Expect, ReplyPart};
mod evidence;
mod state;
use state::State;

pub use crate::{Epoch, ExchangeId};

/// Validated resource and deadline configuration.
#[derive(Debug, Clone, Copy)]
pub struct Config {
    limit: mllp::Limit,
    options: Options,
    wait: Duration,
    queued: usize,
}
impl Config {
    /// Creates bounded configuration; zero queue capacity explicitly refuses pipelining.
    /// # Errors
    /// Rejects zero waits or invalid framing bounds.
    pub fn new(options: Options, wait: Duration, queued: usize) -> Result<Self, Error> {
        if wait.is_zero() {
            return Err(Error::Configuration);
        }
        Ok(Self {
            limit: mllp::Limit::new(options.limits.message_bytes)?,
            options,
            wait,
            queued,
        })
    }
}

/// Specific refusal without payload in its display.
#[derive(Debug, Error)]
pub enum Error {
    /// The selected edition requires an application exchange on another channel.
    #[error(
        "HL7 2.9.1 enhanced application acknowledgment requires an unsupported separate channel"
    )]
    SeparateApplicationChannel,

    /// Invalid deadline configuration.
    #[error("invalid configuration")]
    Configuration,
    /// Time went backwards.
    #[error("time reversed")]
    TimeReversed,
    /// Deadline or identity cannot be represented.
    #[error("identity or time exhausted")]
    Exhausted,
    /// Host response does not match the pending phase and identity.
    #[error("stale host response")]
    Stale,
    /// Another conversation is active.
    #[error("connection busy")]
    Busy,
    /// A correlated response belongs to another acknowledgment phase.
    #[error("unexpected acknowledgment {received} while awaiting {expected}")]
    AckPhase {
        /// Negotiated pending phase.
        expected: AckPhase,
        /// Received acknowledgment code.
        received: AckCode,
    },
    /// The code reports an outcome that the negotiated condition suppresses.
    #[error("acknowledgment {received} violates condition {condition}")]
    AckCondition {
        /// Negotiated outcome condition.
        condition: Condition,
        /// Received acknowledgment code.
        received: AckCode,
    },
    /// Framing defect.
    #[error(transparent)]
    Framing(#[from] mllp::Error),
    /// Syntax or acknowledgment construction defect.
    #[error(transparent)]
    Syntax(#[from] analyte_hl7::Error),
}

/// Host input; each byte is processed independently of transport read boundaries.
#[derive(Debug, Clone, Copy)]
pub enum Event<'a> {
    /// Advance deadlines without input.
    Tick,
    /// Next received byte.
    Byte(u8),
    /// Durable commit succeeded.
    Stored(ExchangeId),
    /// Durable commit failed; responsibility was not accepted.
    StoreFailed(ExchangeId),
    /// Interpretation succeeded under the agreed profile.
    Interpreted(ExchangeId),
    /// Interpretation failed, with structured location and a rejection/content-error choice.
    NotInterpretable(ExchangeId, AckError<'a>, AckChoice),
    /// Fresh response metadata for the requested response phase.
    Respond(ExchangeId, Response<'a>),
    /// Explicit outgoing message and the reply it waits for; never retried automatically.
    Send(&'a [u8], Expect),
    /// Reply prepared only after lookup and any required durable reservations.
    QueryReply(&'a DisplayReply),
    /// The requested reply part was queued by the transport within its budget.
    QueryWritten(ReplyPart),
    /// Issuance failed or became uncertain; never retry implicitly.
    QueryWriteFailed(ReplyPart),
    /// Connection ended. Drop this dialog after consuming returned evidence.
    Disconnected,
}

/// Ordered host effects. Raw bytes belong only in protected storage.
#[derive(Debug)]
pub enum Action {
    /// Commit exact MLLP block, including framing, with driver/config versions.
    Store(ExchangeId, Vec<u8>),
    /// Interpret exact payload only after durable commit.
    Interpret(ExchangeId, Vec<u8>),
    /// Supply fresh response metadata; code identifies the exchange phase.
    ResponseNeeded(ExchangeId, AckCode),
    /// Issue these framed bytes promptly.
    Write(crate::Write),
    /// Framing evidence to park.
    Rejected(mllp::Error, Vec<u8>),
    /// Unparseable or refused complete block to park.
    Parked(Vec<u8>, Error),
    /// Queue overflow; preserve this exact block and close/backpressure the peer.
    QueueFull(Vec<u8>),
    /// Host event refused without losing earlier effects.
    Refused(Error),
    /// Host phase expired; late replies must be reconciled by the host.
    TimedOut(ExchangeId),
    /// Peer explicitly committed the message; application processing is still pending.
    CommitAccepted(Vec<u8>),
    /// Sender outcome, correlated to its original MSH-10.
    Sent(Vec<u8>, Outcome),
    /// Unsolicited or mismatched acknowledgment retained for investigation.
    Unmatched(Vec<u8>),
    /// A late routed receipt was retained after its exchange had already timed out.
    ReceiptRetained(ExchangeId),
    /// Durable query ready for host cache lookup; `started` is the first-byte time.
    Query {
        /// Committed receipt correlation.
        receipt: ExchangeId,
        /// Parsed message preserving original bytes.
        message: Message,
        /// First-byte time relative to this connection clock.
        started: Duration,
        /// Whether the query matches the configured edition.
        edition_matches: bool,
    },
    /// Durable unsolicited-order response ready for retained-work reconciliation.
    OrderResponse {
        /// Committed receipt correlation.
        receipt: ExchangeId,
        /// Parsed message preserving original bytes.
        message: Message,
    },
    /// The unsolicited-order response deadline expired; never resend implicitly.
    OrderResponseTimeout,
    /// DSR application ACK decision, made only after its raw receipt was committed.
    DisplayOutcome {
        /// Committed ACK receipt, absent for timeout.
        receipt: Option<ExchangeId>,
        /// True only for a correlated application AA.
        accepted: bool,
        /// Stable nonclinical outcome reason.
        code: &'static str,
        /// Validation diagnostic without raw payload.
        detail: Option<String>,
    },
    /// Issue one reply part; feed back completion before issuing another part.
    QueryWrite {
        /// Exact framed bytes, for transport only.
        bytes: Vec<u8>,
        /// Absolute deadline relative to the connection clock.
        until: Duration,
        /// Identifies the issuance awaiting completion.
        part: ReplyPart,
    },
    /// Next absolute wake-up time.
    Deadline(Option<Duration>),
}

/// Transport issuance is not proof of acceptance; uncertainty never causes a resend.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Application or durable acceptance, retaining the distinction.
    Accepted(AckCode),
    /// Explicit negative response.
    Rejected(AckCode),
    /// Issued with neither acceptance nor application acknowledgment requested.
    /// This is not evidence that the peer accepted or processed it.
    NotRequested,
    /// Reply lost, connection ended or deadline expired after issuance.
    Uncertain,
}

/// Distinct acknowledgment phases retained in sender diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AckPhase {
    /// Safe-storage acceptance response.
    Accept,
    /// Application processing response.
    Application,
}

#[derive(Debug)]
enum Waiting {
    Accept {
        condition: Condition,
        application: Option<Condition>,
    },
    Application(Condition),
}
#[derive(Debug)]
struct Sending {
    id: Vec<u8>,
    waiting: Waiting,
    // Set by `step` from the current time; separate from the inbound deadline.
    until: Option<Duration>,
}
impl Sending {
    fn acknowledge(&mut self, code: AckCode) -> Result<Option<Outcome>, Error> {
        let accept_code = matches!(code, AckCode::Ca | AckCode::Ce | AckCode::Cr);
        let success = matches!(code, AckCode::Ca | AckCode::Aa);
        let (condition, application) = match self.waiting {
            Waiting::Accept {
                condition,
                application,
            } if accept_code => (condition, application),
            // ER suppresses CA: a later application response may be the first reply.
            Waiting::Accept {
                condition: Condition::Error,
                application: Some(application),
            } if !accept_code => (application, None),
            Waiting::Accept { .. } => {
                return Err(Error::AckPhase {
                    expected: AckPhase::Accept,
                    received: code,
                });
            }
            Waiting::Application(condition) if !accept_code => (condition, None),
            Waiting::Application(_) => {
                return Err(Error::AckPhase {
                    expected: AckPhase::Application,
                    received: code,
                });
            }
        };
        if !condition.requests(success) {
            return Err(Error::AckCondition {
                condition,
                received: code,
            });
        }
        if code == AckCode::Ca
            && let Some(application) = application
        {
            self.waiting = Waiting::Application(application);
            return Ok(None);
        }
        Ok(Some(match code {
            AckCode::Aa | AckCode::Ca => Outcome::Accepted(code),
            AckCode::Ae | AckCode::Ar | AckCode::Ce | AckCode::Cr => Outcome::Rejected(code),
        }))
    }
}

#[derive(Debug)]
enum Phase {
    Commit,
    Interpret,
    Respond,
}
#[derive(Debug)]
struct Pending {
    id: ExchangeId,
    message: Message,
    phase: Phase,
    responsibility: Responsibility,
    codes: VecDeque<AckCode>,
    error: Option<OwnedError>,
}

/// One deterministic connection. Cancellation may leave a committed receipt or sent order;
/// the host journal owns reconciliation. No action authorizes deleting evidence.
#[derive(Debug)]
pub struct Dialog {
    config: Config,
    epoch: Epoch,
    next: u64,
    decoder: mllp::Decoder,
    state: State,
    // HL7 2.5.1 §2.9: our outbound exchange runs beside inbound ones on the same link.
    sending: Option<Sending>,
    queue: VecDeque<(Vec<u8>, Duration)>,
    clock: crate::Clock,
    deadline: Option<Duration>,
    orders: Option<Orders>,
    frame_started: Option<Duration>,
}
impl Dialog {
    /// Starts an idle connection with a host-selected unique incarnation.
    #[must_use]
    pub fn new(config: Config, epoch: Epoch) -> Self {
        Self {
            config,
            epoch,
            next: 0,
            state: State::Idle,
            sending: None,
            decoder: mllp::Decoder::new(config.limit),
            queue: VecDeque::new(),
            clock: crate::Clock::default(),
            deadline: None,
            orders: None,
            frame_started: None,
        }
    }

    /// Expires deadlines before processing input and returns ordered effects.
    /// # Errors
    /// Reversed or overflowing time is rejected before mutation; other errors are actions.
    pub fn step(&mut self, now: Duration, event: Event<'_>) -> Result<Vec<Action>, Error> {
        self.clock
            .advance(now, self.config.wait)
            .map_err(|error| error.into_dialog(Error::TimeReversed, Error::Exhausted))?;
        let end = now.checked_add(self.config.wait).ok_or(Error::Exhausted)?;
        let mut out = Vec::new();
        if let Some(orders) = &mut self.orders {
            orders.expire(now, &mut out);
        }
        if self.deadline.is_some_and(|d| now >= d) {
            self.expire(&mut out);
        }
        if let Some(sending) = self.sending.take_if(|s| s.until.is_some_and(|d| now >= d)) {
            out.push(Action::Sent(sending.id, Outcome::Uncertain));
        }
        if let Err(error) = self.input(event, &mut out) {
            out.push(Action::Refused(error));
        }
        if self.state.pending().is_none()
            && self.state.malformed_id().is_none()
            && !self.orders.as_ref().is_some_and(Orders::committing)
            && !matches!(self.state, State::Closed { .. })
            && let Some((raw, started)) = self.queue.pop_front()
        {
            self.frame_started = Some(started);
            self.receive(raw, &mut out);
            self.frame_started = None;
        }
        if self.deadline.is_none()
            && ((self.state.pending().is_some() || self.state.malformed_id().is_some())
                || !self.decoder.pending().is_empty())
        {
            self.deadline = Some(end);
        }
        if let Some(sending) = &mut self.sending {
            sending.until.get_or_insert(end);
        }
        out.push(Action::Deadline(self.next_deadline()));
        Ok(out)
    }

    fn send(&mut self, raw: &[u8], out: &mut Vec<Action>) -> Result<(), Error> {
        if (self.state.pending().is_some() || self.state.malformed_id().is_some())
            || self.sending.is_some()
            || !self.decoder.pending().is_empty()
        {
            return Err(Error::Busy);
        }
        let message = Message::parse(raw, self.config.options)?;
        let msh = Msh::new(&message)?;
        let id = msh.control_id().to_vec();
        let waiting = match msh.mode() {
            AckMode::Original => Some(Waiting::Application(Condition::Always)),
            AckMode::Enhanced {
                accept,
                application,
            } => {
                let application = application.filter(|c| *c != Condition::Never);
                if let Some(condition) = accept.filter(|c| *c != Condition::Never) {
                    Some(Waiting::Accept {
                        condition,
                        application,
                    })
                } else {
                    application.map(Waiting::Application)
                }
            }
        };
        let wire = crate::Write::encode(raw, self.config.limit, None)?;
        out.push(Action::Write(wire));
        if let Some(waiting) = waiting {
            self.sending = Some(Sending {
                id,
                waiting,
                until: None,
            });
        } else {
            out.push(Action::Sent(id, Outcome::NotRequested));
        }
        Ok(())
    }

    fn committed(&mut self, id: ExchangeId, out: &mut Vec<Action>) -> Result<(), Error> {
        if self.complete_order_commit(id, out)? {
            return Ok(());
        }
        if self.complete_malformed(id, out)? {
            return Ok(());
        }
        let pending = self.state.pending_mut().ok_or(Error::Stale)?;
        if pending.id != id || !matches!(pending.phase, Phase::Commit) {
            return Err(Error::Stale);
        }
        pending.responsibility = Responsibility::Stored;
        pending.phase = Phase::Interpret;
        if matches!(Msh::new(&pending.message)?.mode(), AckMode::Enhanced { .. }) {
            pending.codes.push_back(AckCode::Ca);
            pending.phase = Phase::Respond;
            out.push(Action::ResponseNeeded(id, AckCode::Ca));
        } else {
            out.push(Action::Interpret(id, pending.message.raw().to_vec()));
        }
        self.deadline = None;
        Ok(())
    }

    fn commit_failed(&mut self, id: ExchangeId, out: &mut Vec<Action>) -> Result<(), Error> {
        if self
            .orders
            .as_mut()
            .is_some_and(|orders| orders.failed_commit(id))
        {
            out.push(Action::Refused(Error::Syntax(
                analyte_hl7::Error::NotStored,
            )));
            return Ok(());
        }
        if !self.complete_malformed(id, out)? {
            if !self
                .state
                .pending()
                .is_some_and(|p| p.id == id && matches!(p.phase, Phase::Commit))
            {
                return Err(Error::Stale);
            }
            self.finish_host(id, false, true, None, out)?;
        }
        Ok(())
    }

    fn respond(
        &mut self,
        id: ExchangeId,
        response: Response<'_>,
        out: &mut Vec<Action>,
    ) -> Result<(), Error> {
        let pending = self.state.pending_mut().ok_or(Error::Stale)?;
        if pending.id != id || !matches!(pending.phase, Phase::Respond) {
            return Err(Error::Stale);
        }
        let code = pending.codes.front().copied().ok_or(Error::Stale)?;
        let error = pending.error.as_ref().map(OwnedError::view);
        if let Some(message) = analyte_hl7::acknowledgment(
            &pending.message,
            response,
            code,
            pending.responsibility,
            error,
            self.config.options,
        )? {
            out.push(Action::Write(crate::Write::encode(
                message.raw(),
                self.config.limit,
                None,
            )?));
        }
        pending.codes.pop_front();
        if code == AckCode::Ca {
            pending.phase = Phase::Interpret;
            out.push(Action::Interpret(id, pending.message.raw().to_vec()));
        } else {
            self.state = State::Idle;
        }
        self.deadline = None;
        Ok(())
    }

    fn input(&mut self, event: Event<'_>, out: &mut Vec<Action>) -> Result<(), Error> {
        if matches!(self.state, State::Closed { .. }) {
            return Err(Error::Busy);
        }
        match event {
            Event::Tick => {}
            Event::QueryReply(reply) => self.prepare_reply(reply, out)?,
            Event::QueryWritten(part) => self.reply_written(part, out)?,
            Event::QueryWriteFailed(part) => self.reply_failed(part, out)?,
            Event::Byte(byte) => {
                self.frame_started.get_or_insert(self.clock.now());
                if let Some(event) = self.decoder.push(byte) {
                    self.wire(event, out);
                    self.frame_started = None;
                }
            }
            Event::Send(raw, Expect::Acknowledgment) => {
                self.send(raw, out)?;
            }
            Event::Send(raw, Expect::OrderResponse(wait)) => {
                self.send_order(raw, wait, out)?;
            }
            Event::Stored(id) => {
                self.committed(id, out)?;
            }
            Event::StoreFailed(id) => {
                self.commit_failed(id, out)?;
            }
            Event::Interpreted(id) => self.finish_host(id, true, false, None, out)?,
            Event::NotInterpretable(id, error, reject) => {
                if !self
                    .state
                    .pending()
                    .is_some_and(|p| p.id == id && matches!(p.phase, Phase::Interpret))
                {
                    return Err(Error::Stale);
                }
                self.finish_host(
                    id,
                    false,
                    matches!(reject, AckChoice::Reject),
                    Some(error),
                    out,
                )?;
            }
            Event::Respond(id, response) => {
                self.respond(id, response, out)?;
            }
            Event::Disconnected => {
                if let Some(sending) = self.sending.take() {
                    out.push(Action::Sent(sending.id, Outcome::Uncertain));
                }
                if let Some(event) = self.decoder.finish() {
                    self.wire(event, out);
                }
                self.state.close();
                self.deadline = None;
            }
        }
        Ok(())
    }

    fn finish_host(
        &mut self,
        id: ExchangeId,
        success: bool,
        reject: bool,
        error: Option<AckError<'_>>,
        out: &mut Vec<Action>,
    ) -> Result<(), Error> {
        let pending = self.state.pending_mut().ok_or(Error::Stale)?;
        if pending.id != id
            || !matches!(pending.phase, Phase::Commit | Phase::Interpret)
            || (success && !matches!(pending.phase, Phase::Interpret))
        {
            return Err(Error::Stale);
        }
        let code = if matches!(pending.phase, Phase::Commit) {
            // A storage failure is unrelated to the message content: AR in
            // original mode, a commit error (CE) in enhanced mode. CR is reserved
            // for unacceptable message type, version or processing ID
            // (HL7 2.5.1 §2.9.3).
            match Msh::new(&pending.message)?.mode() {
                AckMode::Original => AckCode::Ar,
                AckMode::Enhanced { .. } => AckCode::Ce,
            }
        } else if success {
            pending.responsibility = Responsibility::StoredAndInterpreted;
            AckCode::Aa
        } else if reject {
            AckCode::Ar
        } else {
            AckCode::Ae
        };
        if error.is_some_and(|e| {
            e.segment.len() > 3 || e.code.len() > 3 || e.application_code.len() > 64
        }) {
            return Err(Error::Configuration);
        }
        pending.error = error.map(OwnedError::from);
        pending.codes.push_back(code);
        pending.phase = Phase::Respond;
        self.deadline = None;
        out.push(Action::ResponseNeeded(id, code));
        Ok(())
    }

    fn wire(&mut self, event: mllp::Event, out: &mut Vec<Action>) {
        match event {
            mllp::Event::Rejected { error, raw } => out.push(Action::Rejected(error, raw)),
            mllp::Event::Block(raw) if self.ack(&raw, out) => {}
            mllp::Event::Block(raw) => {
                if self.state.pending().is_some()
                    || self.state.malformed_id().is_some()
                    || self.orders.as_ref().is_some_and(Orders::committing)
                {
                    if self.queue.len() < self.config.queued {
                        self.queue
                            .push_back((raw, self.frame_started.unwrap_or(self.clock.now())));
                    } else {
                        out.push(Action::QueueFull(raw));
                    }
                } else {
                    self.receive(raw, out);
                }
            }
        }
    }

    fn receive(&mut self, raw: Vec<u8>, out: &mut Vec<Action>) {
        // Preserve first: even malformed syntax must pass through the host's protected journal.
        let result = mllp::payload(&raw)
            .ok_or(Error::Framing(mllp::Error::Incomplete))
            .and_then(|bytes| Message::parse(bytes, self.config.options).map_err(Error::from));
        match result.and_then(|message| {
            let msh = Msh::new(&message)?;
            if msh.version().paired_application_ack()
                && matches!(msh.mode(), AckMode::Enhanced { application: Some(condition), .. } if condition != Condition::Never) {
                return Err(Error::SeparateApplicationChannel);
            }
            Ok(message)
        }) {
            Ok(message) => {
                if self.route_order(&raw, &message, out) {
                    return;
                }
                if message.segments().any(|s| s.id() == b"MSA") {
                    out.push(Action::Unmatched(raw));
                    return;
                }
                if let Some(next) = self.next.checked_add(1) {
                    let id = ExchangeId(self.epoch.0, self.next);
                    self.next = next;
                    self.state = State::Awaiting(Pending {
                        id,
                        message,
                        phase: Phase::Commit,
                        responsibility: Responsibility::NotStored,
                        codes: VecDeque::new(),
                        error: None,
                    });
                    self.deadline = None;
                    out.push(Action::Store(id, raw));
                } else {
                    out.push(Action::Parked(raw, Error::Exhausted));
                }
            }
            Err(error) => {
                if let Some(next) = self.next.checked_add(1) {
                    let id = ExchangeId(self.epoch.0, self.next);
                    self.next = next;
                    out.push(Action::Store(id, raw.clone()));
                    self.state = State::Malformed(id, raw, error);
                    self.deadline = None;
                } else {
                    out.push(Action::Parked(raw, Error::Exhausted));
                }
            }
        }
    }

    // Consumes only acknowledgments while our message waits; other blocks take the inbound path.
    fn ack(&mut self, raw: &[u8], out: &mut Vec<Action>) -> bool {
        let Some(sending) = &mut self.sending else {
            return false;
        };
        let Some(message) =
            mllp::payload(raw).and_then(|p| Message::parse(p, self.config.options).ok())
        else {
            return false;
        };
        let Some(segment) = message.segments().find(|s| s.id() == b"MSA") else {
            return false;
        };
        let result = Msh::new(&message)
            .and_then(|msh| Msa::for_version(segment, msh.version()))
            .map(|msa| (msa.control_id().to_vec(), msa.code()));
        match result {
            Ok((id, code)) if sending.id == id => match sending.acknowledge(code) {
                Ok(Some(outcome)) => {
                    self.sending = None;
                    out.push(Action::Sent(id, outcome));
                }
                Ok(None) => {
                    // The application phase gets its own bounded wait.
                    sending.until = None;
                    out.push(Action::CommitAccepted(id));
                }
                Err(error) => out.push(Action::Parked(raw.to_vec(), error)),
            },
            _ => out.push(Action::Unmatched(raw.to_vec())),
        }
        true
    }

    fn complete_malformed(&mut self, id: ExchangeId, out: &mut Vec<Action>) -> Result<bool, Error> {
        if self
            .state
            .malformed_id()
            .is_some_and(|expected| expected != id)
        {
            return Err(Error::Stale);
        }
        if let Some((_, raw, error)) = self.state.take_malformed() {
            out.push(Action::Parked(raw, error));
            self.deadline = None;
            return Ok(true);
        }
        Ok(false)
    }
    fn expire(&mut self, out: &mut Vec<Action>) {
        if let Some((id, raw, error)) = self.state.take_malformed() {
            out.push(Action::TimedOut(id));
            out.push(Action::Parked(raw, error));
        }
        self.deadline = None;
        if let Some(pending) = self.state.pending() {
            let id = pending.id;
            out.push(Action::TimedOut(id));
            match pending.phase {
                Phase::Commit | Phase::Interpret => {
                    if let Err(error) = self.finish_host(id, false, true, None, out) {
                        out.push(Action::Refused(error));
                    }
                }
                Phase::Respond => {
                    self.state.close();
                }
            }
        }
        if let Some(event) = self.decoder.finish() {
            self.wire(event, out);
        }
        self.frame_started = None;
    }
}

/// Selects the negative application acknowledgment without an unnamed flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AckChoice {
    /// Refuse the transaction.
    Reject,
    /// Report a content error after custody.
    ContentError,
}
