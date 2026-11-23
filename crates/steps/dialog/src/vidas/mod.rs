//! Runs VIDAS result upload and work download conversations.
//! Host owns byte journaling, storage and cancellation; the dialog owns timers.
//! Timeout/disconnect can leave accepted orders or incomplete durable uploads;
//! never automatically replay either. A new conversation requires reconciliation.
use analyte_framing::vidas::{ACK, Config as FrameConfig, ENQ, EOT, ETX, Mode, NAK, Packet};
use analyte_vidas::MAX_DOWNLOAD_REQUESTS;
use analyte_vidas::syntax::{Kind, Message};
use std::{collections::VecDeque, time::Duration};
pub mod outcome;

/// Required site timing/retry evidence; no ASTM constants or defaults.
#[derive(Debug, Clone)]
pub struct Config {
    /// Maximum peer response wait.
    pub ack: Duration,
    /// Maximum gap between received bytes.
    pub inter_byte: Duration,
    /// Maximum entire conversation duration.
    pub session: Duration,
    /// Maximum host commit wait.
    pub commit: Duration,
    /// Recorded site retry count; never permission to replay an order.
    pub retries: u16,
    /// Recorded site backoff; retries require explicit host reconciliation.
    pub backoff: Duration,
}

/// Names a dialog-owned deadline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Timer {
    /// Peer ack.
    Acknowledgment,
    /// Byte gap.
    Interbyte,
    /// Whole conversation, never reset by progress.
    Session,
    /// Durable store response.
    Commit,
}

/// Host-visible protocol output. Raw input is never logged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Send exact protocol bytes.
    Write(Vec<u8>),
    /// Durably save packet with driver/config identity before reporting Stored.
    Store(Packet),
    /// Durably retain the rejected packet before reporting Stored.
    RejectChecksum(Vec<u8>),
    /// Reports a deadline arm for observers; the dialog schedules it internally.
    TimerArmed(Timer, Duration),
    /// Reports a cancelled deadline; the dialog cancels it internally.
    TimerCancelled(Timer),
    /// Keep receipt/order unresolved for operator reconciliation.
    Park,
    /// Transport session completed; this does not mean clinical acceptance.
    Close,
}

/// Events are supplied only after their stated host operation completes.
#[derive(Debug, Clone)]
pub enum Event {
    /// One byte already retained in the host journal.
    Byte(u8),
    /// Incoming control after configured suffix validation and journaling.
    Control(u8),
    /// Decoder's complete checksum-validated acceptance boundary.
    Packet(Packet),
    /// Decoder reached the configured reply point with a bad checksum.
    BadChecksum(Vec<u8>),
    /// Matching accepted or rejected packet has been durably retained.
    Stored,
    /// Storage unavailable or disk nearly full.
    StoreFailed,
    /// A current named deadline expired.
    Expired(Timer),
    /// Advance deadlines without input.
    Tick,
    /// A journaled byte arrived, including an incomplete control or packet.
    Activity,
    /// Connection lost or host cancelled.
    Disconnected,
}

#[derive(Debug, Clone, Copy)]
enum State {
    UploadReady,
    UploadPacket,
    Commit(CommitKind),
    UploadEtx,
    UploadEot,
    DownloadReady,
    DownloadAck,
    Finished,
}

#[derive(Debug, Clone, Copy)]
enum CommitKind {
    Accepted,
    Rejected,
}

/// Specific conversation construction errors.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Deadlines must all be positive.
    #[error("zero conversation deadline")]
    Configuration,
    /// Connection time moved backwards.
    #[error("dialog time moved backwards")]
    TimeReversed,
    /// A connection deadline cannot be represented.
    #[error("dialog deadline exhausted")]
    Exhausted,
    /// A download must contain an mtmpr message.
    #[error("download requires request")]
    Request,
    /// The batch has no requests or exceeds its packet bound.
    #[error("download requires 1 to {MAX_DOWNLOAD_REQUESTS} requests")]
    RequestCount,
    /// Packet encoding failed.
    #[error(transparent)]
    Framing(#[from] analyte_framing::vidas::Error),
}

/// Deterministic state machine; one transfer identity per instance.
pub struct Dialog {
    decoder: analyte_framing::vidas::StreamDecoder,
    config: FrameConfig,
    policy: Config,
    state: State,
    outbound: VecDeque<Vec<u8>>,
    clock: crate::Clock,
    timers: Deadlines,
    expired: Option<Timer>,
}

impl Dialog {
    /// Prepares an upload receiver; ENQ readiness requires reserved store capacity.
    /// # Errors
    /// Rejects zero deadlines. Caller must reserve storage before creation.
    pub fn upload(config: FrameConfig, policy: Config) -> Result<Self, Error> {
        Self::new(config, policy, State::UploadReady, VecDeque::new())
    }

    /// Prepares one work-list request, with no automatic resend path.
    /// # Errors
    /// Rejects non-request messages, invalid policy or encoding failures.
    pub fn download(config: FrameConfig, policy: Config, request: &Message) -> Result<Self, Error> {
        Self::download_batch(config, policy, std::slice::from_ref(request))
    }

    /// Prepares one bounded session with an ACK for each request packet.
    /// # Errors
    /// Rejects an empty batch, a batch above the request limit, non-requests, or invalid framing or policy.
    pub fn download_batch(
        config: FrameConfig,
        policy: Config,
        requests: &[Message],
    ) -> Result<Self, Error> {
        if requests.is_empty() || requests.len() > MAX_DOWNLOAD_REQUESTS {
            return Err(Error::RequestCount);
        }
        let packets = requests
            .iter()
            .map(|request| {
                if request.kind() != Kind::Request {
                    return Err(Error::Request);
                }
                Ok(config.encode(request.text().as_bytes())?)
            })
            .collect::<Result<_, Error>>()?;
        Self::new(config, policy, State::DownloadReady, packets)
    }

    fn new(
        config: FrameConfig,
        policy: Config,
        state: State,
        outbound: VecDeque<Vec<u8>>,
    ) -> Result<Self, Error> {
        if [policy.ack, policy.inter_byte, policy.session, policy.commit].contains(&Duration::ZERO)
        {
            return Err(Error::Configuration);
        }
        Ok(Self {
            decoder: analyte_framing::vidas::StreamDecoder::new(config.clone()),
            config,
            policy,
            state,
            outbound,
            clock: crate::Clock::default(),
            timers: Deadlines::default(),
            expired: None,
        })
    }

    /// Starts deadlines and emits ENQ for a download. Call exactly once.
    #[must_use]
    pub fn start(&mut self) -> Vec<Action> {
        let mut actions = vec![
            Action::TimerArmed(Timer::Session, self.policy.session),
            Action::TimerArmed(Timer::Interbyte, self.policy.inter_byte),
        ];
        if matches!(self.state, State::DownloadReady) {
            actions.push(Action::Write(self.config.control(ENQ)));
            actions.push(Action::TimerArmed(Timer::Acknowledgment, self.policy.ack));
        }
        self.schedule(&actions);
        actions
    }

    /// Returns the next absolute connection-clock wake time.
    #[must_use]
    pub fn next_deadline(&self) -> Option<Duration> {
        [
            self.timers.session,
            self.timers.commit,
            self.timers.ack,
            self.timers.inter_byte,
        ]
        .into_iter()
        .flatten()
        .min()
    }

    /// Identifies the deadline that ended this transfer, for durable diagnostics.
    #[must_use]
    pub fn expired_timer(&self) -> Option<Timer> {
        self.expired
    }

    /// Applies time before input; received fragments never postpone session or commit deadlines.
    /// # Errors
    /// Rejects reversed or overflowing time without changing the conversation.
    pub fn step(&mut self, now: Duration, event: Event) -> Result<Vec<Action>, Error> {
        let headroom = self
            .policy
            .session
            .max(self.policy.inter_byte)
            .max(self.policy.commit)
            .max(self.policy.ack);
        self.clock
            .advance(now, headroom)
            .map_err(|error| error.into_dialog(Error::TimeReversed, Error::Exhausted))?;
        if !matches!(event, Event::Disconnected) {
            for timer in [
                Timer::Session,
                Timer::Commit,
                Timer::Acknowledgment,
                Timer::Interbyte,
            ] {
                if self
                    .timers
                    .get_mut(timer)
                    .is_some_and(|deadline| now >= deadline)
                {
                    return Ok(self.apply(Event::Expired(timer)));
                }
            }
        }
        if matches!(event, Event::Tick) {
            return Ok(Vec::new());
        }
        if matches!(event, Event::Activity) {
            if self.timers.inter_byte.is_some() {
                self.timers.inter_byte = now.checked_add(self.policy.inter_byte);
            }
            return Ok(Vec::new());
        }
        let event = if let Event::Byte(byte) = event {
            if self.timers.inter_byte.is_some() {
                self.timers.inter_byte = now.checked_add(self.policy.inter_byte);
            }
            match self.decoder.push(byte)? {
                Some(analyte_framing::vidas::Event::Packet(packet)) => Event::Packet(packet),
                Some(analyte_framing::vidas::Event::BadChecksum(raw)) => Event::BadChecksum(raw),
                Some(analyte_framing::vidas::Event::Control(control)) => Event::Control(control),
                None => return Ok(Vec::new()),
            }
        } else {
            event
        };
        Ok(self.apply(event))
    }

    fn schedule(&mut self, actions: &[Action]) {
        for action in actions {
            match action {
                Action::TimerArmed(timer, wait) => {
                    *self.timers.get_mut(*timer) = self.clock.now().checked_add(*wait);
                }
                Action::TimerCancelled(timer) => *self.timers.get_mut(*timer) = None,
                Action::Write(_)
                | Action::Store(_)
                | Action::RejectChecksum(_)
                | Action::Park
                | Action::Close => {}
            }
        }
    }

    /// Advances using host events. Any unexpected event parks and stops.
    #[must_use]
    fn apply(&mut self, event: Event) -> Vec<Action> {
        if matches!(event, Event::Tick | Event::Activity) {
            return Vec::new();
        }
        if let Event::Expired(timer) = event
            && !matches!(self.state, State::Finished)
        {
            self.expired = Some(timer);
        }
        let mut actions = vec![
            Action::TimerCancelled(Timer::Interbyte),
            Action::TimerArmed(Timer::Interbyte, self.policy.inter_byte),
        ];
        match (self.state, event) {
            (State::UploadReady, Event::Control(ENQ)) => {
                self.state = State::UploadPacket;
                actions.push(Action::Write(self.config.control(ACK)));
            }
            (State::UploadPacket, Event::BadChecksum(raw)) => {
                self.state = State::Commit(CommitKind::Rejected);
                actions.push(Action::RejectChecksum(raw));
                actions.push(Action::TimerArmed(Timer::Commit, self.policy.commit));
            }
            (State::UploadPacket, Event::Packet(packet)) => {
                self.state = State::Commit(CommitKind::Accepted);
                actions.push(Action::Store(packet));
                actions.push(Action::TimerArmed(Timer::Commit, self.policy.commit));
            }
            (State::Commit(kind), Event::Stored) => self.reply_to_packet(kind, &mut actions),
            (State::UploadEtx, Event::Control(ETX)) => self.state = State::UploadEot,
            (State::UploadEot, Event::Control(EOT)) => {
                self.state = State::Finished;
                actions.push(Action::Close);
            }
            (State::DownloadReady, Event::Control(ACK)) => {
                actions.push(Action::TimerCancelled(Timer::Acknowledgment));
                self.send_next(&mut actions);
            }
            (State::DownloadAck, Event::Control(ACK)) => {
                actions.push(Action::TimerCancelled(Timer::Acknowledgment));
                // Guide C pp. 3-2 and 3-4: each packet ends at ETX, in either mode.
                if self.config.mode() == Mode::Literal {
                    actions.push(Action::Write(self.config.control(ETX)));
                }
                self.send_next(&mut actions);
            }
            (
                State::Finished,
                Event::Byte(_)
                | Event::Control(_)
                | Event::Packet(_)
                | Event::BadChecksum(_)
                | Event::Stored
                | Event::StoreFailed
                | Event::Expired(_)
                | Event::Disconnected
                | Event::Tick
                | Event::Activity,
            ) => return Vec::new(),
            (
                State::UploadReady
                | State::UploadPacket
                | State::Commit(_)
                | State::UploadEtx
                | State::UploadEot
                | State::DownloadReady
                | State::DownloadAck,
                Event::Byte(_)
                | Event::Control(_)
                | Event::Packet(_)
                | Event::BadChecksum(_)
                | Event::Stored
                | Event::StoreFailed
                | Event::Expired(_)
                | Event::Disconnected
                | Event::Tick
                | Event::Activity,
            ) => {
                self.state = State::Finished;
                actions.push(Action::Park);
            }
        }
        if matches!(self.state, State::Finished) {
            for timer in [
                Timer::Session,
                Timer::Interbyte,
                Timer::Commit,
                Timer::Acknowledgment,
            ] {
                actions.push(Action::TimerCancelled(timer));
            }
        }
        self.schedule(&actions);
        actions
    }

    fn send_next(&mut self, actions: &mut Vec<Action>) {
        if let Some(bytes) = self.outbound.pop_front() {
            self.state = State::DownloadAck;
            actions.push(Action::Write(bytes));
            actions.push(Action::TimerArmed(Timer::Acknowledgment, self.policy.ack));
        } else {
            self.state = State::Finished;
            actions.push(Action::Write(self.config.control(EOT)));
            actions.push(Action::Close);
        }
    }

    fn reply_to_packet(&mut self, kind: CommitKind, actions: &mut Vec<Action>) {
        let (control, next) = match kind {
            CommitKind::Rejected => {
                // The sender owns retransmission. The original session deadline still applies.
                (NAK, State::UploadPacket)
            }
            CommitKind::Accepted => {
                let next = match self.config.mode() {
                    Mode::Literal => State::UploadEtx,
                    Mode::Alternate => State::UploadEot,
                };
                (ACK, next)
            }
        };
        self.state = next;
        actions.push(Action::TimerCancelled(Timer::Commit));
        actions.push(Action::Write(self.config.control(control)));
    }
}

#[derive(Default)]
struct Deadlines {
    ack: Option<Duration>,
    inter_byte: Option<Duration>,
    session: Option<Duration>,
    commit: Option<Duration>,
}
impl Deadlines {
    fn get_mut(&mut self, timer: Timer) -> &mut Option<Duration> {
        match timer {
            Timer::Acknowledgment => &mut self.ack,
            Timer::Interbyte => &mut self.inter_byte,
            Timer::Session => &mut self.session,
            Timer::Commit => &mut self.commit,
        }
    }
}
