//! Holds validated ASTM dialog settings, host events, dialog actions and errors.
use analyte_framing::astm::{self, Limit};
use std::time::Duration;
use thiserror::Error;
/// Local actor determines contention priority (section 6.2.7.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Yield to the instrument.
    Computer,
    /// Repeat ENQ after one second.
    Instrument,
}

/// Named timer values; zero durations are rejected by configuration validation.
#[derive(Debug, Clone, Copy)]
pub struct Timers {
    /// Establishment response budget.
    pub establish: Duration,
    /// Computer waiting for the instrument's next ENQ.
    pub contention: Duration,
    /// Frame reply budget.
    pub frame: Duration,
    /// Receiver silence budget.
    pub receive: Duration,
    /// Host durable commit response budget.
    pub commit: Duration,
}

impl Default for Timers {
    fn default() -> Self {
        Self {
            establish: Duration::from_secs(15),
            contention: Duration::from_secs(20),
            frame: Duration::from_secs(15),
            receive: Duration::from_secs(30),
            commit: Duration::from_secs(15),
        }
    }
}

/// Validated per-connection settings. Deviations remain explicit values.
#[derive(Debug, Clone)]
pub struct Config {
    pub(super) limit: Limit,
    pub(super) timers: Timers,
    pub(super) role: Role,
    pub(super) bytes: usize,
    pub(super) frames: usize,
    pub(super) honor_interrupt: bool,
}

impl Config {
    /// Validates timers and resource ceilings before a connection starts.
    /// # Errors
    /// Returns `InvalidConfig` for zero budgets or ceilings.
    pub fn new(
        limit: Limit,
        timers: Timers,
        role: Role,
        bytes: usize,
        frames: usize,
        honor_interrupt: bool,
    ) -> Result<Self, Error> {
        if bytes == 0
            || frames == 0
            || [
                timers.establish,
                timers.contention,
                timers.frame,
                timers.receive,
                timers.commit,
            ]
            .contains(&Duration::ZERO)
        {
            return Err(Error::InvalidConfig);
        }
        Ok(Self {
            limit,
            timers,
            role,
            bytes,
            frames,
            honor_interrupt,
        })
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            limit: Limit::default(),
            timers: Timers::default(),
            role: Role::Computer,
            bytes: 16_777_216,
            frames: 65_536,
            honor_interrupt: true,
        }
    }
}

/// Host-supplied identity namespace; use a fresh value after every restart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Epoch(pub u128);

/// Correlates a durable write with exactly one frame request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommitId {
    pub(super) epoch: Epoch,
    pub(super) transfer: u64,
    pub(super) frame: u64,
}

/// Completion reason; committed evidence is never removed by the dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum End {
    /// EOT or all outgoing frames accepted.
    Normal,
    /// Six negative replies.
    Abort,
    /// A named deadline expired.
    Timeout,
    /// Receiver requested termination.
    Interrupt,
    /// EOT arrived before the final (ETX) frame of a record, or while a frame
    /// was still being committed. Committed frames remain evidence; the host must
    /// treat the transfer as incomplete, never as a complete message.
    Incomplete,
}

/// Deterministic effects; raw data must go only to protected storage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Write bytes in order before supplying another event.
    Write(Vec<u8>),
    /// Store exact bytes and identity durably before replying `Committed`.
    Commit(CommitId, astm::FrameNumber, Vec<u8>),
    /// One complete ETX-terminated record, assembled only from committed frames.
    Record(Vec<u8>),
    /// Receive or send transfer ended; incomplete assembly is discarded.
    Ended(End),
    /// Send attempt finished; failure leaves repetition to the host's explicit decision.
    Sent(End),
    /// Commit failed or exceeded its budget; never positively acknowledged.
    CommitFailed(CommitId),
    /// Same previous number carried different bytes.
    Conflict(Vec<u8>),
    /// Rejected wire bytes and specific framing cause.
    Rejected(astm::Error, Vec<u8>),
    /// Resource ceiling reached; no frame ACK is emitted.
    LimitReached,
    /// Next wakeup, replacing any previously returned deadline.
    Deadline(Option<Duration>),
    /// Host event refused; earlier timeout effects in this batch still apply.
    Refused(Error),
    /// Valid but unsolicited frame retained for host evidence; never acknowledged.
    UnexpectedFrame(Vec<u8>),
}

/// One host event. Bytes are fed individually, so output batches stay bounded.
#[derive(Debug)]
pub enum Event {
    /// Next wire byte; TCP packet boundaries have no meaning.
    Byte(u8),
    /// Wakeup without input.
    Tick,
    /// Host has reserved capacity for accepting ENQ.
    Capacity(bool),
    /// Durable response; stale identities are rejected.
    Committed(CommitId),
    /// Failed durable response.
    CommitFailed(CommitId),
    /// Explicit new outgoing message; each vector is a separate record.
    Send(Vec<Vec<u8>>),
    /// Request EOT instead of ACK for subsequent committed frames.
    Interrupt(bool),
}

/// Boundary failures contain no clinical data.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Error {
    /// Configuration has an unusable bound.
    #[error("zero timer or resource bound")]
    InvalidConfig,
    /// Host time must never go backwards.
    #[error("nonmonotonic time")]
    TimeReversed,
    /// A future deadline cannot be represented.
    #[error("time or identity exhausted")]
    Exhausted,
    /// Only one outgoing attempt can be active.
    #[error("link busy")]
    Busy,
    /// Outgoing message exceeds agreed bounds.
    #[error("outgoing limit exceeded")]
    Limit,
    /// Codec rejected outgoing data.
    #[error("outgoing framing: {0}")]
    Framing(#[from] astm::Error),
    /// Commit reply does not identify the outstanding request.
    #[error("stale commit reply")]
    StaleCommit,
}

impl CommitId {
    /// Restart-safe namespace supplied by the host.
    #[must_use]
    pub const fn epoch(self) -> Epoch {
        self.epoch
    }

    /// Connection-local transfer ordinal (first accepted ENQ is one).
    #[must_use]
    pub const fn transfer(self) -> u64 {
        self.transfer
    }

    /// Connection-local durable request ordinal; failed requests also consume one.
    #[must_use]
    pub const fn frame(self) -> u64 {
        self.frame
    }
}
