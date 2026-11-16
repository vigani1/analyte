//! Runs VIDAS result upload and work download conversations.
//! Host owns byte journaling, storage and cancellation; the dialog owns timers.
//! Timeout/disconnect can leave accepted orders or incomplete durable uploads;
//! never automatically replay either. A new conversation requires reconciliation.
use analyte_framing::vidas::{Config, Mode, Packet};
use analyte_vidas::syntax::{Kind, Message};
use std::time::Duration;

/// Required site timing/retry evidence; no ASTM constants or defaults.
#[derive(Debug, Clone)]
pub struct Policy {
    /// Maximum peer response wait.
    pub acknowledgment: Duration,
    /// Maximum gap between received bytes.
    pub interbyte: Duration,
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
    /// Peer acknowledgment.
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
    Send(Vec<u8>),
    /// Durably save packet with driver/config identity before reporting Stored.
    Store(Packet),
    /// Reports a deadline arm for observers; the dialog schedules it internally.
    Arm(Timer, Duration),
    /// Reports a cancelled deadline; the dialog cancels it internally.
    Cancel(Timer),
    /// Keep receipt/order unresolved for operator reconciliation.
    Park,
    /// Transport session completed; this does not mean clinical acceptance.
    Complete,
}

/// Events are supplied only after their stated host operation completes.
#[derive(Debug, Clone)]
pub enum Event {
    /// Incoming control after configured suffix validation and journaling.
    Control(u8),
    /// Decoder's complete checksum-validated acceptance boundary.
    Packet(Packet),
    /// Matching outstanding packet has durably committed.
    Stored,
    /// Storage unavailable or disk nearly full.
    StorageFailed,
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
    Commit,
    UploadEtx,
    UploadEot,
    DownloadReady,
    DownloadAck,
    Finished,
}

/// Specific conversation construction errors.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Deadlines must all be positive.
    #[error("zero conversation deadline")]
    Policy,
    /// Connection time moved backwards.
    #[error("dialog time moved backwards")]
    TimeReversed,
    /// A connection deadline cannot be represented.
    #[error("dialog deadline exhausted")]
    Exhausted,
    /// A download must contain an mtmpr message.
    #[error("download requires request")]
    Request,
    /// Packet encoding failed.
    #[error(transparent)]
    Framing(#[from] analyte_framing::vidas::Error),
}

/// Deterministic state machine; one transfer identity per instance.
pub struct Dialog {
    config: Config,
    policy: Policy,
    state: State,
    outbound: Option<Vec<u8>>,
    clock: crate::Clock,
    timers: [Option<Duration>; 4],
}

impl Dialog {
    /// Prepares an upload receiver; ENQ readiness requires reserved store capacity.
    /// # Errors
    /// Rejects zero deadlines. Caller must reserve storage before creation.
    pub fn upload(config: Config, policy: Policy) -> Result<Self, Error> {
        Self::new(config, policy, State::UploadReady, None)
    }

    /// Prepares one work-list request, with no automatic resend path.
    /// # Errors
    /// Rejects non-request messages, invalid policy or encoding failures.
    pub fn download(config: Config, policy: Policy, request: &Message) -> Result<Self, Error> {
        if request.kind() != Kind::Request {
            return Err(Error::Request);
        }
        let bytes = config.encode(request.text().as_bytes())?;
        Self::new(config, policy, State::DownloadReady, Some(bytes))
    }

    fn new(
        config: Config,
        policy: Policy,
        state: State,
        outbound: Option<Vec<u8>>,
    ) -> Result<Self, Error> {
        if [
            policy.acknowledgment,
            policy.interbyte,
            policy.session,
            policy.commit,
        ]
        .contains(&Duration::ZERO)
        {
            return Err(Error::Policy);
        }
        Ok(Self {
            config,
            policy,
            state,
            outbound,
            clock: crate::Clock::default(),
            timers: [None; 4],
        })
    }

    /// Starts deadlines and emits ENQ for a download. Call exactly once.
    #[must_use]
    pub fn start(&mut self) -> Vec<Action> {
        let mut actions = vec![
            Action::Arm(Timer::Session, self.policy.session),
            Action::Arm(Timer::Interbyte, self.policy.interbyte),
        ];
        if matches!(self.state, State::DownloadReady) {
            actions.push(Action::Send(self.config.control(5)));
            actions.push(Action::Arm(
                Timer::Acknowledgment,
                self.policy.acknowledgment,
            ));
        }
        self.schedule(&actions);
        actions
    }

    /// Returns the next absolute connection-clock wake time.
    #[must_use]
    pub fn next_deadline(&self) -> Option<Duration> {
        self.timers.iter().flatten().copied().min()
    }

    /// Applies time before input; received fragments never postpone session or commit deadlines.
    /// # Errors
    /// Rejects reversed or overflowing time without changing the conversation.
    pub fn step(&mut self, now: Duration, event: Event) -> Result<Vec<Action>, Error> {
        let headroom = self
            .policy
            .session
            .max(self.policy.interbyte)
            .max(self.policy.commit)
            .max(self.policy.acknowledgment);
        self.clock
            .advance(now, headroom)
            .map_err(|error| match error {
                crate::ClockError::Reversed => Error::TimeReversed,
                crate::ClockError::Exhausted => Error::Exhausted,
            })?;
        if !matches!(event, Event::Disconnected) {
            for timer in [
                Timer::Session,
                Timer::Commit,
                Timer::Acknowledgment,
                Timer::Interbyte,
            ] {
                if self.timers[timer.index()].is_some_and(|deadline| now >= deadline) {
                    return Ok(self.on(Event::Expired(timer)));
                }
            }
        }
        if matches!(event, Event::Tick) {
            return Ok(Vec::new());
        }
        if matches!(event, Event::Activity) {
            if self.timers[Timer::Interbyte.index()].is_some() {
                self.timers[Timer::Interbyte.index()] = now.checked_add(self.policy.interbyte);
            }
            return Ok(Vec::new());
        }
        Ok(self.on(event))
    }

    fn schedule(&mut self, actions: &[Action]) {
        for action in actions {
            match action {
                Action::Arm(timer, wait) => {
                    self.timers[timer.index()] = self.clock.now().checked_add(*wait)
                }
                Action::Cancel(timer) => self.timers[timer.index()] = None,
                Action::Send(_) | Action::Store(_) | Action::Park | Action::Complete => {}
            }
        }
    }

    /// Advances using host events. Any unexpected event parks and stops.
    /// Compatibility entry for events at the last accepted clock time.
    /// Timed hosts use `step`; timers are updated before returning any store action.
    #[must_use]
    pub fn on(&mut self, event: Event) -> Vec<Action> {
        if matches!(event, Event::Tick | Event::Activity) {
            return Vec::new();
        }
        let mut actions = vec![
            Action::Cancel(Timer::Interbyte),
            Action::Arm(Timer::Interbyte, self.policy.interbyte),
        ];
        match (self.state, event) {
            (State::UploadReady, Event::Control(5)) => {
                self.state = State::UploadPacket;
                actions.push(Action::Send(self.config.control(6)));
            }
            (State::UploadPacket, Event::Packet(packet)) => {
                self.state = State::Commit;
                actions.push(Action::Store(packet));
                actions.push(Action::Arm(Timer::Commit, self.policy.commit));
            }
            (State::Commit, Event::Stored) => {
                actions.push(Action::Cancel(Timer::Commit));
                actions.push(Action::Send(self.config.control(6)));
                self.state = if self.config.mode() == Mode::Literal {
                    State::UploadEtx
                } else {
                    State::UploadEot
                };
            }
            (State::UploadEtx, Event::Control(3)) => self.state = State::UploadEot,
            (State::UploadEot, Event::Control(4)) => {
                self.state = State::Finished;
                actions.push(Action::Complete);
            }
            (State::DownloadReady, Event::Control(6)) => {
                self.state = State::DownloadAck;
                actions.push(Action::Cancel(Timer::Acknowledgment));
                if let Some(bytes) = self.outbound.take() {
                    actions.push(Action::Send(bytes));
                }
                actions.push(Action::Arm(
                    Timer::Acknowledgment,
                    self.policy.acknowledgment,
                ));
            }
            (State::DownloadAck, Event::Control(6)) => {
                actions.push(Action::Cancel(Timer::Acknowledgment));
                if self.config.mode() == Mode::Literal {
                    actions.push(Action::Send(self.config.control(3)));
                }
                actions.push(Action::Send(self.config.control(4)));
                actions.push(Action::Complete);
                self.state = State::Finished;
            }
            (
                State::Finished,
                Event::Control(_)
                | Event::Packet(_)
                | Event::Stored
                | Event::StorageFailed
                | Event::Expired(_)
                | Event::Disconnected
                | Event::Tick
                | Event::Activity,
            ) => return Vec::new(),
            (
                State::UploadReady
                | State::UploadPacket
                | State::Commit
                | State::UploadEtx
                | State::UploadEot
                | State::DownloadReady
                | State::DownloadAck,
                Event::Control(_)
                | Event::Packet(_)
                | Event::Stored
                | Event::StorageFailed
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
                actions.push(Action::Cancel(timer));
            }
        }
        self.schedule(&actions);
        actions
    }
}

impl Timer {
    fn index(self) -> usize {
        match self {
            Self::Acknowledgment => 0,
            Self::Interbyte => 1,
            Self::Session => 2,
            Self::Commit => 3,
        }
    }
}
