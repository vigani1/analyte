//! HL7 2.5.1 sections 2.9.2/2.9.3 over MLLP R1. Evidence: documented.
use analyte_framing::mllp;
use analyte_hl7::{
    AckCode, AckError, AckMode, Message, Msa, Msh, Options, Response, Responsibility,
};
use std::{collections::VecDeque, time::Duration};
use thiserror::Error;

/// Connection-scoped host request identity. The host must supply a unique epoch after restart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReceiptId(u64, u64);
impl ReceiptId {
    /// Returns the host-selected connection incarnation for journaling.
    #[must_use]
    pub fn epoch(self) -> u64 {
        self.0
    }
    /// Returns the unique request ordinal within this incarnation.
    #[must_use]
    pub fn ordinal(self) -> u64 {
        self.1
    }
}

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
    Committed(ReceiptId),
    /// Durable commit failed; responsibility was not accepted.
    CommitFailed(ReceiptId),
    /// Interpretation succeeded under the agreed profile.
    Interpreted(ReceiptId),
    /// Interpretation failed, with structured location and a rejection/content-error choice.
    NotInterpretable(ReceiptId, AckError<'a>, bool),
    /// Fresh response metadata for the requested response phase.
    Respond(ReceiptId, Response<'a>),
    /// Explicit outgoing message; never retried automatically.
    Send(&'a [u8]),
    /// Connection ended. Drop this dialog after consuming returned evidence.
    Disconnect,
}

/// Ordered host effects. Raw bytes belong only in protected storage.
#[derive(Debug)]
pub enum Action {
    /// Commit exact MLLP block, including framing, with driver/config versions.
    Commit(ReceiptId, Vec<u8>),
    /// Interpret exact payload only after durable commit.
    Interpret(ReceiptId, Vec<u8>),
    /// Supply fresh response metadata; code identifies the exchange phase.
    ResponseNeeded(ReceiptId, AckCode),
    /// Issue these framed bytes promptly.
    Write(Vec<u8>),
    /// Framing evidence to park.
    Rejected(mllp::Error, Vec<u8>),
    /// Unparseable or refused complete block to park.
    Parked(Vec<u8>, Error),
    /// Queue overflow; preserve this exact block and close/backpressure the peer.
    QueueFull(Vec<u8>),
    /// Host event refused without losing earlier effects.
    Refused(Error),
    /// Host phase expired; late replies must be reconciled by the host.
    TimedOut(ReceiptId),
    /// Sender outcome, correlated to its original MSH-10.
    Sent(Vec<u8>, Outcome),
    /// Unsolicited or mismatched acknowledgment retained for investigation.
    Unmatched(Vec<u8>),
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
    /// Reply lost, connection ended or deadline expired after issuance.
    Uncertain,
}

#[derive(Debug)]
enum Phase {
    Commit,
    Interpret,
    Respond,
}
#[derive(Debug)]
struct Pending {
    id: ReceiptId,
    message: Message,
    phase: Phase,
    responsibility: Responsibility,
    codes: VecDeque<AckCode>,
}

/// One deterministic connection. Cancellation may leave a committed receipt or sent order;
/// the host journal owns reconciliation. No action authorizes deleting evidence.
#[derive(Debug)]
pub struct Dialog {
    config: Config,
    epoch: u64,
    next: u64,
    decoder: mllp::Decoder,
    pending: Option<Pending>,
    malformed: Option<(ReceiptId, Vec<u8>, Error)>,
    queue: VecDeque<Vec<u8>>,
    sending: Option<Vec<u8>>,
    error: Option<OwnedError>,
    now: Duration,
    deadline: Option<Duration>,
    closed: bool,
}
impl Dialog {
    /// Starts an idle connection with a host-selected unique incarnation.
    #[must_use]
    pub fn new(config: Config, epoch: u64) -> Self {
        Self {
            config,
            epoch,
            next: 0,
            decoder: mllp::Decoder::new(config.limit),
            pending: None,
            malformed: None,
            queue: VecDeque::new(),
            sending: None,
            error: None,
            now: Duration::ZERO,
            deadline: None,
            closed: false,
        }
    }

    /// Expires deadlines before processing input and returns ordered effects.
    /// # Errors
    /// Reversed or overflowing time is rejected before mutation; other errors are actions.
    pub fn step(&mut self, now: Duration, event: Event<'_>) -> Result<Vec<Action>, Error> {
        if now < self.now {
            return Err(Error::TimeReversed);
        }
        let end = now.checked_add(self.config.wait).ok_or(Error::Exhausted)?;
        self.now = now;
        let mut out = Vec::new();
        if self.deadline.is_some_and(|d| now >= d) {
            self.expire(&mut out);
        }
        if let Err(error) = self.input(event, &mut out) {
            out.push(Action::Refused(error));
        }
        if self.pending.is_none()
            && self.malformed.is_none()
            && self.sending.is_none()
            && !self.closed
            && let Some(raw) = self.queue.pop_front()
        {
            self.receive(raw, &mut out);
        }
        if self.deadline.is_none()
            && ((self.pending.is_some() || self.malformed.is_some())
                || self.sending.is_some()
                || !self.decoder.pending().is_empty())
        {
            self.deadline = Some(end);
        }
        out.push(Action::Deadline(self.deadline));
        Ok(out)
    }

    fn input(&mut self, event: Event<'_>, out: &mut Vec<Action>) -> Result<(), Error> {
        if self.closed {
            return Err(Error::Busy);
        }
        match event {
            Event::Tick => {}
            Event::Byte(byte) => {
                if let Some(event) = self.decoder.push(byte) {
                    self.wire(event, out);
                }
            }
            Event::Send(raw) => {
                if (self.pending.is_some() || self.malformed.is_some())
                    || self.sending.is_some()
                    || !self.decoder.pending().is_empty()
                {
                    return Err(Error::Busy);
                }
                let message = Message::parse(raw, self.config.options)?;
                let id = Msh::new(&message)?.control_id().to_vec();
                let wire = mllp::encode(raw, self.config.limit)?;
                self.sending = Some(id);
                out.push(Action::Write(wire));
            }
            Event::Committed(id) => {
                if self.complete_malformed(id, out)? {
                    return Ok(());
                }
                let pending = self.pending.as_mut().ok_or(Error::Stale)?;
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
            }
            Event::CommitFailed(id) => {
                if !self.complete_malformed(id, out)? {
                    if !self
                        .pending
                        .as_ref()
                        .is_some_and(|p| p.id == id && matches!(p.phase, Phase::Commit))
                    {
                        return Err(Error::Stale);
                    }
                    self.finish_host(id, false, true, None, out)?;
                }
            }
            Event::Interpreted(id) => self.finish_host(id, true, false, None, out)?,
            Event::NotInterpretable(id, error, reject) => {
                if !self
                    .pending
                    .as_ref()
                    .is_some_and(|p| p.id == id && matches!(p.phase, Phase::Interpret))
                {
                    return Err(Error::Stale);
                }
                self.finish_host(id, false, reject, Some(error), out)?;
            }
            Event::Respond(id, response) => {
                let pending = self.pending.as_mut().ok_or(Error::Stale)?;
                if pending.id != id || !matches!(pending.phase, Phase::Respond) {
                    return Err(Error::Stale);
                }
                let code = pending.codes.front().copied().ok_or(Error::Stale)?;
                let error = self.error.as_ref().map(OwnedError::view);
                if let Some(message) = analyte_hl7::acknowledgment(
                    &pending.message,
                    response,
                    code,
                    pending.responsibility,
                    error,
                    self.config.options,
                )? {
                    out.push(Action::Write(mllp::encode(
                        message.raw(),
                        self.config.limit,
                    )?));
                }
                pending.codes.pop_front();
                if code == AckCode::Ca {
                    pending.phase = Phase::Interpret;
                    out.push(Action::Interpret(id, pending.message.raw().to_vec()));
                } else {
                    self.pending = None;
                    self.error = None;
                }
                self.deadline = None;
            }
            Event::Disconnect => {
                if let Some(id) = self.sending.take() {
                    out.push(Action::Sent(id, Outcome::Uncertain));
                }
                if let Some(event) = self.decoder.finish() {
                    self.wire(event, out);
                }
                self.closed = true;
                self.deadline = None;
            }
        }
        Ok(())
    }

    fn finish_host(
        &mut self,
        id: ReceiptId,
        success: bool,
        reject: bool,
        error: Option<AckError<'_>>,
        out: &mut Vec<Action>,
    ) -> Result<(), Error> {
        let pending = self.pending.as_mut().ok_or(Error::Stale)?;
        if pending.id != id
            || !matches!(pending.phase, Phase::Commit | Phase::Interpret)
            || (success && !matches!(pending.phase, Phase::Interpret))
        {
            return Err(Error::Stale);
        }
        let code = if matches!(pending.phase, Phase::Commit) {
            match Msh::new(&pending.message)?.mode() {
                AckMode::Original => AckCode::Ar,
                AckMode::Enhanced { .. } => AckCode::Cr,
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
        self.error = error.map(OwnedError::from);
        pending.codes.push_back(code);
        pending.phase = Phase::Respond;
        self.deadline = None;
        out.push(Action::ResponseNeeded(id, code));
        Ok(())
    }

    fn wire(&mut self, event: mllp::Event, out: &mut Vec<Action>) {
        match event {
            mllp::Event::Rejected { error, raw } => out.push(Action::Rejected(error, raw)),
            mllp::Event::Block(raw) => {
                if self.sending.is_some() {
                    self.ack(&raw, out);
                } else if self.pending.is_some() || self.malformed.is_some() {
                    if self.queue.len() < self.config.queued {
                        self.queue.push_back(raw);
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
        let result = payload(&raw)
            .and_then(|bytes| Message::parse(bytes, self.config.options).map_err(Error::from));
        match result.and_then(|message| {
            Msh::new(&message)?;
            Ok(message)
        }) {
            Ok(message) => {
                if message.segments().any(|s| s.id() == b"MSA") {
                    out.push(Action::Unmatched(raw));
                    return;
                }
                if let Some(next) = self.next.checked_add(1) {
                    let id = ReceiptId(self.epoch, self.next);
                    self.next = next;
                    self.pending = Some(Pending {
                        id,
                        message,
                        phase: Phase::Commit,
                        responsibility: Responsibility::NotStored,
                        codes: VecDeque::new(),
                    });
                    self.deadline = None;
                    out.push(Action::Commit(id, raw));
                } else {
                    out.push(Action::Parked(raw, Error::Exhausted));
                }
            }
            Err(error) => {
                if let Some(next) = self.next.checked_add(1) {
                    let id = ReceiptId(self.epoch, self.next);
                    self.next = next;
                    out.push(Action::Commit(id, raw.clone()));
                    self.malformed = Some((id, raw, error));
                    self.deadline = None;
                } else {
                    out.push(Action::Parked(raw, Error::Exhausted));
                }
            }
        }
    }

    fn ack(&mut self, raw: &[u8], out: &mut Vec<Action>) {
        let parsed =
            payload(raw).and_then(|p| Message::parse(p, self.config.options).map_err(Error::from));
        let result = parsed.and_then(|message| {
            Msh::new(&message)?;
            let segment = message
                .segments()
                .find(|s| s.id() == b"MSA")
                .ok_or(Error::Stale)?;
            let msa = Msa::new(segment)?;
            Ok((msa.control_id().to_vec(), msa.code()))
        });
        if let Ok((id, code)) = result
            && self.sending.as_ref() == Some(&id)
        {
            let outcome = match code {
                AckCode::Aa | AckCode::Ca => Outcome::Accepted(code),
                AckCode::Ae | AckCode::Ar | AckCode::Ce | AckCode::Cr => Outcome::Rejected(code),
            };
            self.sending = None;
            self.deadline = None;
            out.push(Action::Sent(id, outcome));
            return;
        }
        out.push(Action::Unmatched(raw.to_vec()));
    }

    fn complete_malformed(&mut self, id: ReceiptId, out: &mut Vec<Action>) -> Result<bool, Error> {
        if self
            .malformed
            .as_ref()
            .is_some_and(|(expected, _, _)| *expected != id)
        {
            return Err(Error::Stale);
        }
        if let Some((_, raw, error)) = self.malformed.take() {
            out.push(Action::Parked(raw, error));
            self.deadline = None;
            return Ok(true);
        }
        Ok(false)
    }
    fn expire(&mut self, out: &mut Vec<Action>) {
        if let Some((id, raw, error)) = self.malformed.take() {
            out.push(Action::TimedOut(id));
            out.push(Action::Parked(raw, error));
        }
        self.deadline = None;
        if let Some(id) = self.sending.take() {
            out.push(Action::Sent(id, Outcome::Uncertain));
        }
        if let Some(pending) = self.pending.as_ref() {
            let id = pending.id;
            out.push(Action::TimedOut(id));
            match pending.phase {
                Phase::Commit | Phase::Interpret => {
                    if let Err(error) = self.finish_host(id, false, true, None, out) {
                        out.push(Action::Refused(error));
                    }
                }
                Phase::Respond => {
                    self.closed = true;
                }
            }
        }
        if let Some(event) = self.decoder.finish() {
            self.wire(event, out);
        }
    }
}

fn payload(raw: &[u8]) -> Result<&[u8], Error> {
    raw.len()
        .checked_sub(2)
        .and_then(|end| raw.get(1..end))
        .ok_or(Error::Framing(mllp::Error::Incomplete))
}

#[derive(Debug)]
struct OwnedError {
    segment: Vec<u8>,
    occurrence: u16,
    field: u16,
    code: Vec<u8>,
    application: Vec<u8>,
}
impl From<AckError<'_>> for OwnedError {
    fn from(e: AckError<'_>) -> Self {
        Self {
            segment: e.segment.to_vec(),
            occurrence: e.occurrence,
            field: e.field,
            code: e.code.to_vec(),
            application: e.application_code.to_vec(),
        }
    }
}
impl OwnedError {
    fn view(&self) -> AckError<'_> {
        AckError {
            segment: &self.segment,
            occurrence: self.occurrence,
            field: self.field,
            code: &self.code,
            application_code: &self.application,
        }
    }
}
