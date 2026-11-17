//! Runs ASTM stop-and-wait connections over serial or TCP (LIS01-A2 6/8).
use analyte_framing::astm::{self, ACK, Decoder, ENQ, EOT, Frame, FrameNumber, NAK};
use std::{collections::VecDeque, time::Duration};

mod control;
pub use control::{Action, CommitId, Config, End, Epoch, Error, Event, Interrupt, Role, Timers};

#[derive(Debug)]
struct Receive {
    expected: FrameNumber,
    previous: Option<Frame>,
    record: Vec<u8>,
    count: u64,
    records: usize,
}

#[derive(Debug)]
enum State {
    Neutral,
    Establish(VecDeque<Vec<u8>>),
    Busy(VecDeque<Vec<u8>>),
    Contention,
    Priority(VecDeque<Vec<u8>>),
    Sending {
        current: Vec<u8>,
        rest: VecDeque<Vec<u8>>,
        failures: u8,
    },
    Receiving(Receive),
    Commit {
        receive: Receive,
        frame: Frame,
        id: CommitId,
    },
    Holdoff,
}

/// One connection's bounded dialog. Dropping it never implies a commit failed.
/// The host owns protected journaling, replay/reconciliation and ordered writes.
#[derive(Debug)]
pub struct Dialog {
    config: Config,
    epoch: Epoch,
    transfer: u64,
    request: u64,
    state: State,
    decoder: Decoder,
    clock: crate::Clock,
    deadline: Option<Duration>,
    capacity: bool,
    interrupt: bool,
    closed: bool,
}

impl Dialog {
    /// Creates a neutral connection with capacity disabled.
    #[must_use]
    pub fn new(config: Config, epoch: Epoch) -> Self {
        let decoder = Decoder::new(config.limit, config.checksum_hex);
        Self {
            config,
            epoch,
            transfer: 0,
            request: 0,
            state: State::Neutral,
            decoder,
            clock: crate::Clock::default(),
            deadline: None,
            capacity: false,
            interrupt: false,
            closed: false,
        }
    }

    /// Advances time, expires deadlines before input, and returns ordered effects.
    /// Writes are considered issued at `now`; the host must budget actual transport
    /// latency inside configured reply budgets. Never retry an order implicitly.
    /// # Errors
    /// Rejects reversed or unrepresentable time before mutation. Other refused events
    /// produce `Action::Refused`, preserving any preceding timeout effects.
    pub fn step(&mut self, now: Duration, event: Event) -> Result<Vec<Action>, Error> {
        let headroom = [
            self.config.timers.establish,
            self.config.timers.contention,
            self.config.timers.frame,
            self.config.timers.receive,
            self.config.timers.commit,
            Duration::from_secs(15),
        ]
        .into_iter()
        .max()
        .unwrap_or(Duration::ZERO);
        self.clock
            .advance(now, headroom)
            .map_err(|error| error.into_dialog(Error::TimeReversed, Error::Exhausted))?;
        let mut out = Vec::new();
        if matches!(event, Event::Disconnected) {
            self.disconnect(&mut out);
            out.push(Action::Deadline(None));
            return Ok(out);
        }
        if self.closed {
            return Ok(vec![Action::Refused(Error::Busy), Action::Deadline(None)]);
        }
        if self.deadline.is_some_and(|d| now >= d) {
            self.expire(&mut out);
        }
        let result = (|| {
            match event {
                Event::Tick | Event::Disconnected => {}
                Event::Capacity(value) => self.capacity = value,
                Event::Interrupt(value) => self.interrupt = value,
                Event::Byte(byte) => {
                    if let Some(event) = self.decoder.push(byte) {
                        self.wire(event, &mut out)?;
                    }
                    // Unsolicited partial frames still belong to the dialog and
                    // must be bounded and returned as evidence on timeout/EOF.
                    if matches!(self.state, State::Neutral) {
                        if self.decoder.pending().is_empty() {
                            self.deadline = None;
                        } else if self.deadline.is_none() {
                            self.set_timer(self.config.timers.receive);
                        }
                    }
                }
                Event::Stored(id) => self.commit(id, true, &mut out)?,
                Event::StoreFailed(id) => self.commit(id, false, &mut out)?,
                Event::Send(records) => {
                    if !matches!(self.state, State::Neutral) {
                        return Err(Error::Busy);
                    }
                    let frames = self.encode(&records)?;
                    self.state = State::Establish(frames);
                    out.push(Action::Write(vec![ENQ]));
                    self.set_timer(self.config.timers.establish);
                }
            }
            Ok::<(), Error>(())
        })();
        if let Err(error) = result {
            out.push(Action::Refused(error));
        }
        out.push(Action::Deadline(self.deadline));
        Ok(out)
    }

    /// Returns the next absolute connection-clock wake time, or none when idle/closed.
    #[must_use]
    pub fn next_deadline(&self) -> Option<Duration> {
        self.deadline
    }

    fn disconnect(&mut self, out: &mut Vec<Action>) {
        if self.closed {
            return;
        }
        self.closed = true;
        self.deadline = None;
        if let Some(astm::Event::Rejected { error, raw }) = self.decoder.finish() {
            out.push(Action::Rejected(error, raw));
        }
        let state = std::mem::replace(&mut self.state, State::Neutral);
        match state {
            State::Establish(_) | State::Busy(_) | State::Priority(_) | State::Sending { .. } => {
                out.push(Action::Sent(End::Incomplete));
                out.push(Action::Ended(End::Incomplete));
            }
            State::Receiving(_) | State::Commit { .. } | State::Contention => {
                out.push(Action::Ended(End::Incomplete));
            }
            State::Neutral | State::Holdoff => {}
        }
    }

    fn set_timer(&mut self, budget: Duration) {
        self.deadline = self.clock.now().checked_add(budget);
    }

    fn encode(&self, records: &[Vec<u8>]) -> Result<VecDeque<Vec<u8>>, Error> {
        let mut frames = VecDeque::new();
        let mut bytes = 0usize;
        let mut number = FrameNumber::FIRST;
        if records.is_empty() || records.len() > self.config.frames {
            return Err(Error::Limit);
        }
        for record in records {
            if !astm::RecordBoundaries::Separate.accepts(record, true) {
                return Err(astm::Error::RecordBoundary.into());
            }
            bytes = bytes.checked_add(record.len()).ok_or(Error::Limit)?;
            if bytes > self.config.bytes {
                return Err(Error::Limit);
            }
            for frame in astm::encode(record, number, self.config.limit)? {
                if frames.len() >= self.config.frames {
                    return Err(Error::Limit);
                }
                frames.push_back(frame);
                number = number.next();
            }
        }
        Ok(frames)
    }

    fn start_frame(&mut self, mut frames: VecDeque<Vec<u8>>, out: &mut Vec<Action>) {
        if let Some(current) = frames.pop_front() {
            out.push(Action::Write(current.clone()));
            self.state = State::Sending {
                current,
                rest: frames,
                failures: 0,
            };
            self.set_timer(self.config.timers.frame);
        } else {
            self.end_send(End::Normal, out);
        }
    }

    fn end_send(&mut self, reason: End, out: &mut Vec<Action>) {
        self.state = State::Neutral;
        self.deadline = None;
        out.extend([
            Action::Write(vec![EOT]),
            Action::Sent(reason),
            Action::Ended(reason),
        ]);
    }

    fn expire(&mut self, out: &mut Vec<Action>) {
        let state = std::mem::replace(&mut self.state, State::Neutral);
        self.deadline = None;
        match state {
            State::Busy(frames) | State::Priority(frames) => {
                self.state = State::Establish(frames);
                out.push(Action::Write(vec![ENQ]));
                self.set_timer(self.config.timers.establish);
            }
            State::Establish(_) | State::Sending { .. } => self.end_send(End::Timeout, out),
            State::Commit { receive, id, .. } => {
                // Storage did not answer in time: refuse the frame but stay in the
                // transfer, so the sender's retransmission can be committed again
                // (LIS01-A2 §6.5.1.2 bounds the retries on the sender's side).
                out.extend([Action::Write(vec![NAK]), Action::StoreFailed(id)]);
                self.state = State::Receiving(receive);
                self.set_timer(self.config.timers.receive);
            }
            State::Receiving(_) | State::Contention => out.push(Action::Ended(End::Timeout)),
            State::Neutral | State::Holdoff => {}
        }
        if let Some(astm::Event::Rejected { error, raw }) = self.decoder.finish() {
            out.push(Action::Rejected(error, raw));
        }
    }

    fn ready(&mut self, out: &mut Vec<Action>) -> Result<(), Error> {
        if self.capacity {
            self.transfer = self.transfer.checked_add(1).ok_or(Error::Exhausted)?;
            self.state = State::Receiving(Receive {
                expected: FrameNumber::FIRST,
                previous: None,
                record: Vec::new(),
                records: 0,
                count: 0,
            });
            out.push(Action::Write(vec![ACK]));
            self.set_timer(self.config.timers.receive);
        } else {
            out.push(Action::Write(vec![NAK]));
        }
        Ok(())
    }

    fn wire(&mut self, event: astm::Event, out: &mut Vec<Action>) -> Result<(), Error> {
        match event {
            astm::Event::Outside(byte) => self.control(byte, out)?,
            astm::Event::Frame(frame) => self.frame(frame, out)?,
            astm::Event::Rejected { error, raw } => {
                let control = match error {
                    astm::Error::LinkControlInFrame(byte) => Some(byte),
                    astm::Error::InvalidLimit(_)
                    | astm::Error::BadFrameNumber(_)
                    | astm::Error::TooLong
                    | astm::Error::ControlInText(_)
                    | astm::Error::LowercaseChecksum
                    | astm::Error::NonHexChecksum(_)
                    | astm::Error::BadChecksum { .. }
                    | astm::Error::MissingTrailer(_)
                    | astm::Error::Interrupted
                    | astm::Error::Incomplete
                    | astm::Error::RecordBoundary => None,
                };
                out.push(Action::Rejected(error, raw));
                if matches!(self.state, State::Receiving(_)) {
                    out.push(Action::Write(vec![NAK]));
                    self.set_timer(self.config.timers.receive);
                }
                if let Some(byte) = control {
                    self.control(byte, out)?;
                }
            }
        }
        Ok(())
    }

    fn frame(&mut self, frame: Frame, out: &mut Vec<Action>) -> Result<(), Error> {
        let state = std::mem::replace(&mut self.state, State::Neutral);
        match state {
            State::Receiving(receive) => {
                if receive
                    .previous
                    .as_ref()
                    .is_some_and(|p| p.number() == frame.number())
                {
                    if receive.previous.as_ref() == Some(&frame) {
                        out.push(Action::Write(vec![if self.interrupt { EOT } else { ACK }]));
                        self.set_timer(self.config.timers.receive);
                    } else {
                        out.extend([
                            Action::Conflict(frame.raw().to_vec()),
                            Action::Write(vec![NAK]),
                        ]);
                        self.set_timer(self.config.timers.receive);
                    }
                    self.state = State::Receiving(receive);
                } else if frame.number() != receive.expected {
                    out.push(Action::UnexpectedFrame(frame.raw().to_vec()));
                    out.push(Action::Write(vec![NAK]));
                    self.set_timer(self.config.timers.receive);
                    self.state = State::Receiving(receive);
                } else if !self
                    .config
                    .record_boundaries
                    .accepts(frame.text(), frame.is_final())
                {
                    out.extend([
                        Action::Rejected(astm::Error::RecordBoundary, frame.raw().to_vec()),
                        Action::Write(vec![NAK]),
                    ]);
                    self.state = State::Receiving(receive);
                    self.set_timer(self.config.timers.receive);
                } else if (self.config.record_boundaries == astm::RecordBoundaries::Packed
                    && receive
                        .records
                        .checked_add(frame.text().iter().filter(|b| **b == astm::CR).count())
                        .is_none_or(|n| n > self.config.frames))
                    || receive.count
                        >= u64::try_from(self.config.frames).map_err(|_| Error::Exhausted)?
                    || receive
                        .record
                        .len()
                        .checked_add(frame.text().len())
                        .is_none_or(|n| n > self.config.bytes)
                {
                    out.extend([
                        Action::Write(vec![NAK]),
                        Action::UnexpectedFrame(frame.raw().to_vec()),
                        Action::LimitReached,
                        Action::Ended(End::Abort),
                    ]);
                    self.deadline = None;
                } else {
                    let id = CommitId {
                        epoch: self.epoch,
                        transfer: self.transfer,
                        frame: self.request,
                    };
                    self.request = self.request.checked_add(1).ok_or(Error::Exhausted)?;
                    out.push(Action::Store(id, frame.number(), frame.raw().to_vec()));
                    self.state = State::Commit { receive, frame, id };
                    self.set_timer(self.config.timers.commit);
                }
            }
            other @ (State::Neutral
            | State::Establish(_)
            | State::Busy(_)
            | State::Contention
            | State::Priority(_)
            | State::Sending { .. }
            | State::Commit { .. }
            | State::Holdoff) => {
                out.push(Action::UnexpectedFrame(frame.raw().to_vec()));
                self.state = other;
                if matches!(self.state, State::Sending { .. }) {
                    self.control(NAK, out)?;
                }
            }
        }
        Ok(())
    }

    fn commit(
        &mut self,
        reply: CommitId,
        success: bool,
        out: &mut Vec<Action>,
    ) -> Result<(), Error> {
        if !matches!(&self.state, State::Commit { id, .. } if *id == reply) {
            return Err(Error::StaleCommit);
        }
        let state = std::mem::replace(&mut self.state, State::Neutral);
        if let State::Commit {
            mut receive,
            frame,
            id,
        } = state
        {
            if success {
                receive.count = receive.count.checked_add(1).ok_or(Error::Exhausted)?;
                receive.record.extend_from_slice(frame.text());
                match self.config.record_boundaries {
                    astm::RecordBoundaries::Separate => {
                        if frame.is_final() {
                            out.push(Action::Record(std::mem::take(&mut receive.record)));
                        }
                    }
                    astm::RecordBoundaries::Packed => {
                        receive.records = receive
                            .records
                            .checked_add(frame.text().iter().filter(|b| **b == astm::CR).count())
                            .ok_or(Error::Exhausted)?;
                        let complete = receive
                            .record
                            .iter()
                            .rposition(|b| *b == astm::CR)
                            .map_or(0, |i| i.saturating_add(1));
                        let tail = receive.record.split_off(complete);
                        for record in receive.record.split_inclusive(|b| *b == astm::CR) {
                            out.push(Action::Record(record.to_vec()));
                        }
                        receive.record = tail;
                    }
                }
                receive.expected = frame.number().next();
                receive.previous = Some(frame);
                out.push(Action::Write(vec![if self.interrupt { EOT } else { ACK }]));
            } else {
                out.extend([Action::Write(vec![NAK]), Action::StoreFailed(id)]);
            }
            self.state = State::Receiving(receive);
            self.set_timer(self.config.timers.receive);
        }
        Ok(())
    }

    fn sending_control(
        &mut self,
        byte: u8,
        current: Vec<u8>,
        rest: VecDeque<Vec<u8>>,
        failures: u8,
        out: &mut Vec<Action>,
    ) {
        if byte == ACK || byte == EOT {
            if byte == EOT && self.config.honor_interrupt == Interrupt::Honor {
                self.end_send(End::Interrupt, out);
                self.state = State::Holdoff;
                self.set_timer(Duration::from_secs(15));
            } else {
                self.start_frame(rest, out);
            }
        } else {
            let failures = failures.saturating_add(1);
            if failures >= 6 {
                self.end_send(End::Abort, out);
            } else {
                out.push(Action::Write(current.clone()));
                self.state = State::Sending {
                    current,
                    rest,
                    failures,
                };
                self.set_timer(self.config.timers.frame);
            }
        }
    }

    fn control(&mut self, byte: u8, out: &mut Vec<Action>) -> Result<(), Error> {
        let state = std::mem::replace(&mut self.state, State::Neutral);
        match state {
            State::Neutral => {
                if byte == ENQ {
                    self.ready(out)?;
                }
            }
            State::Contention => {
                self.state = State::Contention;
                if byte == ENQ {
                    self.state = State::Neutral;
                    self.deadline = None;
                    self.ready(out)?;
                }
            }
            State::Establish(frames) => match byte {
                ACK => self.start_frame(frames, out),
                NAK => {
                    self.state = State::Busy(frames);
                    self.set_timer(Duration::from_secs(10));
                }
                ENQ => match self.config.role {
                    Role::Computer => {
                        out.push(Action::Sent(End::Interrupt));
                        self.state = State::Contention;
                        self.set_timer(self.config.timers.contention);
                    }
                    Role::Instrument => {
                        self.state = State::Priority(frames);
                        self.set_timer(Duration::from_secs(1));
                    }
                },
                _ => self.state = State::Establish(frames),
            },
            State::Sending {
                current,
                rest,
                failures,
            } => {
                self.sending_control(byte, current, rest, failures, out);
            }
            State::Receiving(receive) => {
                if byte == EOT {
                    self.deadline = None;
                    let end = if receive.record.is_empty() {
                        End::Normal
                    } else {
                        End::Incomplete
                    };
                    out.push(Action::Ended(end));
                } else {
                    self.state = State::Receiving(receive);
                }
            }
            State::Commit { receive, frame, id } => {
                if byte == EOT {
                    self.deadline = None;
                    out.extend([Action::StoreFailed(id), Action::Ended(End::Incomplete)]);
                } else {
                    self.state = State::Commit { receive, frame, id };
                }
            }
            State::Holdoff => {
                self.state = State::Holdoff;
                if byte == ENQ {
                    self.ready(out)?;
                }
            }
            State::Busy(frames) => {
                if byte == ENQ {
                    out.push(Action::Sent(End::Interrupt));
                    self.deadline = None;
                    self.ready(out)?;
                } else {
                    self.state = State::Busy(frames);
                }
            }
            State::Priority(frames) => self.state = State::Priority(frames),
        }
        Ok(())
    }
}
