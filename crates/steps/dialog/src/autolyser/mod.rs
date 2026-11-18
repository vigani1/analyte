//! Runs host-driven Autolyser conversations with host-owned I/O and dialog-owned deadlines.
use analyte_autolyser::syntax::{self as records, Report, Worklist};
use analyte_framing::autolyser::{ACK, Decoder, EOT, Frame, MAX_ENVELOPE, Mode, NAK, STX};
use std::time::Duration;
use thiserror::Error;
pub mod outcome;

/// Recovery policy must be chosen explicitly by the deployment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recovery {
    /// Stop for operator-led last-report recovery.
    ManualLast,
    /// Permit a last-report request on the next configured poll interval.
    LastOnPoll,
}

/// Required site policy; no manufacturer timing defaults are known.
#[derive(Debug, Clone, Copy)]
pub struct Config {
    /// Interval before another operation may start.
    pub poll: Duration,
    /// STX acknowledgment deadline.
    pub ack: Duration,
    /// Total response deadline, independent of inter-byte progress.
    pub response: Duration,
    /// Maximum silence during a response.
    pub inter_byte: Duration,
    /// Durable-store deadline.
    pub storage: Duration,
    /// Maximum raw response bytes, including EOT.
    pub max_bytes: usize,
    /// Explicit controls layout selection.
    pub controls: bool,
    /// Corrupt/incomplete report policy.
    pub recovery: Recovery,
}

/// Poll operation; first-report replay is always explicit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Request {
    /// R: next report.
    Next,
    /// L: last report, for recovery.
    Last,
    /// A: explicit archive replay/reconciliation.
    First,
}

impl Request {
    fn byte(self) -> u8 {
        match self {
            Self::Next => b'R',
            Self::Last => b'L',
            Self::First => b'A',
        }
    }
}

/// Specific dialog refusal; never contains patient content.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Error {
    /// Required bound or deadline is invalid.
    #[error("invalid configuration")]
    Configuration,
    /// Operation conflicts with the current phase.
    #[error("connection busy or recovery required")]
    Busy,
    /// Connection time moved backwards.
    #[error("dialog time moved backwards")]
    TimeReversed,
    /// A deadline or storage token cannot be represented.
    #[error("dialog deadline or token exhausted")]
    Exhausted,
    /// Response did not complete within its bound.
    #[error("response incomplete or overlong")]
    Incomplete,
    /// Durable storage failed or expired.
    #[error("durable storage unavailable")]
    Storage,
    /// Host completion does not identify the pending response.
    #[error("stale storage completion")]
    Stale,
    /// Unexpected initialization reply.
    #[error("unexpected acknowledgment")]
    Ack,
    /// Checksum envelope defect.
    #[error(transparent)]
    Framing(#[from] analyte_framing::autolyser::Error),
    /// Parsed record defect.
    #[error(transparent)]
    Record(#[from] records::Error),
}

/// Connection-scoped storage request ordinal; never a clinical result identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoreId(u64);

impl StoreId {
    /// Returns the ordinal for host evidence within this connection incarnation.
    #[must_use]
    pub const fn ordinal(self) -> u64 {
        self.0
    }
}

/// Host input, processed without consulting a clock or performing I/O.
#[derive(Clone, Copy)]
pub enum Event<'a> {
    /// Start an explicit poll/replay operation.
    Poll(Request),
    /// Start one insertion; never queued or retried automatically.
    Download(&'a Worklist),
    /// Next received byte, independent of transport read boundaries.
    Byte(u8),
    /// Durable commit completion for the emitted token.
    Stored(StoreId),
    /// Commit failed; preserve emitted evidence and close the connection.
    StoreFailed(StoreId),
    /// Evaluate deadlines; may initiate configured L recovery only.
    Tick,
    /// End the connection, retaining any partial response for storage.
    Disconnected,
}

/// Ordered effects; byte-bearing actions belong only in protected storage.
#[derive(Debug)]
pub enum Action {
    /// Write exactly these bytes.
    Write(Vec<u8>),
    /// Durably store token and exact raw bytes with driver/config versions.
    Store(StoreId, Vec<u8>),
    /// Stored response is valid; identity remains unresolved until mapping qualifies it.
    Report(Report),
    /// Stored response is corrupt or incomplete and needs reconciliation.
    Park(Error),
    /// Standalone NAK; no report available, not a busy or insertion error.
    Empty,
    /// Worklist outcome after durable storage of the reply.
    Insertion(Reception),
    /// Outcome may have happened; reconcile, never blindly resend an insertion.
    Uncertain,
    /// Caller input refused without changing the active operation.
    Refused(Error),
    /// Stop reading/writing this connection; host journals any subsequent bytes.
    Close,
    /// Next absolute monotonic wake-up time.
    Deadline(Duration),
}

/// Single-byte insertion failure (Rev18 p.176), preserving unrecognized values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertionError {
    /// Analyzer rejection category: `Checksum`.
    Checksum,
    /// Analyzer rejection category: `UnknownCommand`.
    UnknownCommand,
    /// Analyzer rejection category: `List`.
    List,
    /// Analyzer rejection category: `Sample`.
    Sample,
    /// Analyzer rejection category: `Clone`.
    Clone,
    /// Analyzer rejection category: `Position`.
    Position,
    /// Analyzer rejection category: `CountField`.
    CountField,
    /// Analyzer rejection category: `CountMismatch`.
    CountMismatch,
    /// Analyzer rejection category: `Executing`.
    Executing,
    /// Analyzer rejection category: `CannotClone`.
    CannotClone,
    /// Analyzer rejection category: `Duplicate`.
    Duplicate,
    /// Analyzer rejection category: `UnknownAnalysis`.
    UnknownAnalysis,
    /// Analyzer rejection category: `AnalysisOffTray`.
    AnalysisOffTray,
    /// Analyzer rejection category: `TooManyAnalyses`.
    TooManyAnalyses,
    /// Analyzer rejection category: `Constant`.
    Constant,
    /// Analyzer rejection category: `Variable`.
    Variable,
    /// Analyzer rejection category: `AnalysisVariable`.
    AnalysisVariable,
    /// Analyzer rejection category: `NoRepeatPatient`.
    NoRepeatPatient,
    /// Analyzer rejection category: `RepeatSampleMismatch`.
    RepeatSampleMismatch,
    /// Analyzer rejection category: `RepeatListFull`.
    RepeatListFull,
    /// Analyzer rejection category: `RepeatListMismatch`.
    RepeatListMismatch,
    /// Analyzer rejection category: `PositionOccupied`.
    PositionOccupied,
    /// Analyzer rejection category: `ExtraListPatient`.
    ExtraListPatient,
    /// Analyzer rejection category: `RepeatDisabled`.
    RepeatDisabled,
    /// Unrecognized wire byte retained exactly.
    Other(u8),
}

impl From<u8> for InsertionError {
    fn from(b: u8) -> Self {
        match b {
            1 => Self::Checksum,
            2 => Self::UnknownCommand,
            3 => Self::List,
            4 => Self::Sample,
            5 => Self::Clone,
            6 => Self::Position,
            7 => Self::CountField,
            8 => Self::CountMismatch,
            9 => Self::Executing,
            10 => Self::CannotClone,
            11 => Self::Duplicate,
            12 => Self::UnknownAnalysis,
            13 => Self::AnalysisOffTray,
            14 => Self::TooManyAnalyses,
            15 => Self::Constant,
            16 => Self::Variable,
            17 => Self::AnalysisVariable,
            18 => Self::NoRepeatPatient,
            19 => Self::RepeatSampleMismatch,
            20 => Self::RepeatListFull,
            21 => Self::RepeatListMismatch,
            22 => Self::PositionOccupied,
            23 => Self::ExtraListPatient,
            24 => Self::RepeatDisabled,
            other => Self::Other(other),
        }
    }
}

/// Two-byte insertion reply; success position is binary, not two decimal digits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reception {
    /// Analyzer accepted at the exact returned position byte.
    Accepted(u8),
    /// Analyzer refused with a specific error.
    Rejected(InsertionError),
    /// Unknown prefix and detail retained for operator investigation.
    Other(u8, u8),
}

#[derive(Debug)]
enum Operation {
    Poll(Request),
    Download(Vec<u8>),
}

#[derive(Debug)]
enum State {
    Idle,
    Ack(Operation),
    Response {
        insertion: bool,
        decoder: Decoder,
        total: Duration,
    },
    Store {
        token: StoreId,
        frame: Frame,
        insertion: bool,
    },
    Closed,
}

/// One serialized connection. Cancellation/disconnect can leave an uncertain
/// insertion or advanced result cursor; host must reconcile before a new dialog.
#[derive(Debug)]
pub struct Dialog {
    config: Config,
    state: State,
    clock: crate::Clock,
    deadline: Duration,
    next_token: u64,
    recovery: bool,
    transport_ended: bool,
}

impl Dialog {
    /// Validates bounds and starts idle. Tokens are connection-scoped: the host
    /// must journal a unique connection incarnation alongside each token.
    /// # Errors
    /// Rejects zero deadlines and limits outside 2 to [`MAX_ENVELOPE`] bytes.
    pub fn new(config: Config) -> Result<Self, Error> {
        if [
            config.poll,
            config.ack,
            config.response,
            config.inter_byte,
            config.storage,
        ]
        .iter()
        .any(Duration::is_zero)
            || !(2..=MAX_ENVELOPE).contains(&config.max_bytes)
        {
            return Err(Error::Configuration);
        }
        Ok(Self {
            config,
            state: State::Idle,
            clock: crate::Clock::default(),
            deadline: Duration::ZERO,
            next_token: 0,
            recovery: false,
            transport_ended: false,
        })
    }

    /// Returns the next absolute wake time; manual recovery waits for an explicit event.
    #[must_use]
    pub fn next_deadline(&self) -> Option<Duration> {
        if matches!(self.state, State::Closed)
            || (matches!(self.state, State::Idle) && self.deadline.is_zero())
            || (matches!(self.state, State::Idle)
                && self.recovery
                && self.config.recovery == Recovery::ManualLast)
        {
            None
        } else {
            Some(self.deadline)
        }
    }

    fn after(&self, wait: Duration) -> Result<Duration, Error> {
        self.clock
            .after(wait)
            .map_err(|error| error.into_dialog(Error::TimeReversed, Error::Exhausted))
    }

    fn begin(&mut self, operation: Operation) -> Result<Vec<Action>, Error> {
        if self.transport_ended
            || !matches!(self.state, State::Idle)
            || self.clock.now() < self.deadline
        {
            return Err(Error::Busy);
        }
        if self.recovery && !matches!(operation, Operation::Poll(Request::Last)) {
            return Err(Error::Busy);
        }
        self.deadline = self.after(self.config.ack)?;
        self.state = State::Ack(operation);
        Ok(vec![
            Action::Write(vec![STX]),
            Action::Deadline(self.deadline),
        ])
    }

    fn store(&mut self, frame: Frame, insertion: bool) -> Result<Vec<Action>, Error> {
        let ordinal = self.next_token.checked_add(1).ok_or(Error::Exhausted)?;
        self.next_token = ordinal;
        let token = StoreId(ordinal);
        self.deadline = self.after(self.config.storage)?;
        let action = Action::Store(token, frame.raw().to_vec());
        self.state = State::Store {
            token,
            frame,
            insertion,
        };
        Ok(vec![action, Action::Deadline(self.deadline)])
    }

    /// Applies one event at a host-supplied monotonic elapsed time.
    /// The host journals transport input before feeding it, including unexpected
    /// bytes and bytes arriving at expired deadlines. Storage always precedes
    /// interpretation. On Close the host stops feeding
    /// this dialog and retains unread transport bytes in its protected journal.
    /// # Errors
    /// Rejects reversed time before mutating the conversation.
    pub fn step(&mut self, now: Duration, event: Event<'_>) -> Result<Vec<Action>, Error> {
        self.clock
            .advance(now, Duration::ZERO)
            .map_err(|error| error.into_dialog(Error::TimeReversed, Error::Exhausted))?;
        Ok(match self.apply(event) {
            Ok(actions) => actions,
            Err(error) => vec![Action::Refused(error)],
        })
    }

    fn apply(&mut self, event: Event<'_>) -> Result<Vec<Action>, Error> {
        if !matches!(self.state, State::Idle | State::Closed) && self.clock.now() >= self.deadline {
            return self.interrupt();
        }
        match event {
            Event::Poll(request) => self.begin(Operation::Poll(request)),
            Event::Download(item) => self.begin(Operation::Download(
                analyte_framing::autolyser::encode(item.encode()),
            )),
            Event::Byte(byte) => self.byte(byte),
            Event::Stored(token) => self.stored(token),
            Event::StoreFailed(token) => {
                if !matches!(self.state, State::Store { token: pending, .. } if token == pending) {
                    return Err(Error::Stale);
                }
                self.state = State::Closed;
                Ok(vec![
                    Action::Park(Error::Storage),
                    Action::Uncertain,
                    Action::Close,
                ])
            }
            Event::Disconnected => self.interrupt(),
            Event::Tick => {
                if self.recovery
                    && self.config.recovery == Recovery::LastOnPoll
                    && matches!(self.state, State::Idle)
                    && self.clock.now() >= self.deadline
                {
                    self.begin(Operation::Poll(Request::Last))
                } else {
                    Ok(Vec::new())
                }
            }
        }
    }

    fn interrupt(&mut self) -> Result<Vec<Action>, Error> {
        self.transport_ended = true;
        let state = std::mem::replace(&mut self.state, State::Closed);
        match state {
            State::Response {
                insertion,
                mut decoder,
                ..
            } => {
                let mut actions = self.store(decoder.finish(), insertion)?;
                actions.push(Action::Close);
                Ok(actions)
            }
            State::Store {
                token,
                frame,
                insertion,
            } => {
                if self.clock.now() >= self.deadline {
                    return Ok(vec![
                        Action::Park(Error::Storage),
                        Action::Uncertain,
                        Action::Close,
                    ]);
                }
                self.state = State::Store {
                    token,
                    frame,
                    insertion,
                };
                Ok(vec![Action::Park(Error::Storage), Action::Close])
            }
            State::Ack(_) => Ok(vec![Action::Uncertain, Action::Close]),
            State::Idle | State::Closed => Ok(vec![Action::Close]),
        }
    }

    fn byte(&mut self, byte: u8) -> Result<Vec<Action>, Error> {
        match &mut self.state {
            State::Ack(_) => {
                if byte != ACK {
                    self.state = State::Closed;
                    return Ok(vec![Action::Park(Error::Ack), Action::Close]);
                }
                let total = self.after(self.config.response)?;
                self.deadline = total.min(self.after(self.config.inter_byte)?);
                let operation = std::mem::replace(&mut self.state, State::Closed);
                let State::Ack(operation) = operation else {
                    return Err(Error::Busy);
                };
                let (bytes, insertion) = match operation {
                    Operation::Poll(request) => (vec![request.byte(), EOT], false),
                    Operation::Download(bytes) => (bytes, true),
                };
                self.state = State::Response {
                    insertion,
                    decoder: Decoder::new(
                        if insertion {
                            Mode::Insertion
                        } else {
                            Mode::Report
                        },
                        self.config.max_bytes,
                    )?,
                    total,
                };
                Ok(vec![Action::Write(bytes), Action::Deadline(self.deadline)])
            }
            State::Response {
                insertion,
                decoder,
                total,
            } => {
                if let Some(frame) = decoder.push(byte)? {
                    let insertion = *insertion;
                    let complete = frame.complete();
                    let mut actions = self.store(frame, insertion)?;
                    if !complete {
                        actions.push(Action::Close);
                    }
                    Ok(actions)
                } else {
                    self.deadline = (*total).min(
                        self.clock
                            .now()
                            .checked_add(self.config.inter_byte)
                            .ok_or(Error::Exhausted)?,
                    );
                    Ok(vec![Action::Deadline(self.deadline)])
                }
            }
            State::Idle | State::Store { .. } | State::Closed => Err(Error::Busy),
        }
    }

    fn stored(&mut self, token: StoreId) -> Result<Vec<Action>, Error> {
        if !matches!(self.state, State::Store { token: pending, .. } if pending == token) {
            return Err(Error::Stale);
        }
        let deadline = self.after(self.config.poll)?;
        let state = std::mem::replace(&mut self.state, State::Closed);
        let State::Store {
            frame, insertion, ..
        } = state
        else {
            return Err(Error::Stale);
        };
        if !frame.complete() {
            self.recovery = true;
            return Ok(vec![
                Action::Park(Error::Incomplete),
                Action::Uncertain,
                Action::Close,
            ]);
        }
        self.state = State::Idle;
        self.deadline = deadline;
        let action =
            if insertion {
                let mut bytes = frame.raw().iter().copied();
                let prefix = bytes.next().ok_or(Error::Incomplete)?;
                let detail = bytes.next().ok_or(Error::Incomplete)?;
                Action::Insertion(match prefix {
                    b'Y' => Reception::Accepted(detail),
                    b'N' => Reception::Rejected(detail.into()),
                    other => Reception::Other(other, detail),
                })
            } else if frame.raw() == [NAK] {
                // An empty L does not resolve a previously corrupt report.
                Action::Empty
            } else {
                match frame.body().map_err(Error::Framing).and_then(|body| {
                    Report::parse(body, self.config.controls).map_err(Error::Record)
                }) {
                    Ok(report) => {
                        self.recovery = false;
                        Action::Report(report)
                    }
                    Err(error) => {
                        self.recovery = true;
                        Action::Park(error)
                    }
                }
            };
        Ok(vec![action, Action::Deadline(self.deadline)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ADR 0001
    #[test]
    fn exhausted_token_differs_from_reversed_time() {
        let config = Config {
            poll: Duration::from_secs(1),
            ack: Duration::from_secs(1),
            response: Duration::from_secs(1),
            inter_byte: Duration::from_secs(1),
            storage: Duration::from_secs(1),
            max_bytes: 11038,
            controls: false,
            recovery: Recovery::ManualLast,
        };
        let mut dialog = Dialog::new(config).unwrap();
        dialog.next_token = u64::MAX;
        let mut decoder = Decoder::new(Mode::Report, 11038).unwrap();
        assert!(matches!(
            dialog.store(decoder.finish(), false),
            Err(Error::Exhausted)
        ));
    }
}
