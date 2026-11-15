//! Per-connection reviewer dialog: events in, ordered host actions out, no I/O.
use crate::{
    AckKind, ApplicationError, Body, Capabilities, ControlId, EscapeReason, Message, Nullable,
    Outbound, TerminationReason,
};
use std::{collections::VecDeque, time::Duration};

/// Host limits and deadlines; these are policy, not claimed standard defaults.
#[derive(Debug, Clone)]
pub struct DialogConfig {
    /// Maximum uncommitted dynamic messages, including asynchronous observations.
    pub pending_messages: usize,
    /// Maximum accepted bytes in any message.
    pub message_bytes: usize,
    /// Generous wait for the device's next response.
    pub response_timeout: Duration,
    /// Host's explicit ACK budget when the device omits DCP.
    pub unannounced_ack_timeout: Duration,
}

/// Invalid configuration or an event that cannot be applied safely.
#[derive(Debug, thiserror::Error)]
pub enum DialogError {
    /// Limits and timeout budgets must be positive.
    #[error("invalid dialog configuration")]
    Config,
    /// Logical time moved backwards.
    #[error("dialog time moved backwards")]
    TimeReversed,
    /// A deadline or identifier cannot be represented.
    #[error("dialog counter or deadline exhausted")]
    Exhausted,
    /// No pending commit has this ticket, or it already has an outcome.
    #[error("unknown or already completed commit ticket")]
    Commit,
    /// The host command is not valid in this conversation phase.
    #[error("host command is invalid in the current phase")]
    State,
    /// Session ID is invalid.
    #[error(transparent)]
    Identity(#[from] crate::MessageError),
}

/// A store-operation identity scoped by the host's session identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitId {
    session: ControlId,
    sequence: u64,
}

impl CommitId {
    /// Host-supplied session identity; it must survive store recovery.
    #[must_use]
    pub fn session(&self) -> &ControlId {
        &self.session
    }

    /// Monotonic sequence within this connection.
    #[must_use]
    pub fn sequence(&self) -> u64 {
        self.sequence
    }
}

/// Storage completion is explicit: failure never produces an acceptance ACK.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommitOutcome {
    /// Raw bytes and interpreted observations are durably in custody.
    Stored,
    /// Raw bytes are parked durably, but semantic processing failed.
    ApplicationError(ApplicationError),
    /// Storage failed or reserve capacity is unavailable.
    Failed,
}

/// Events supplied by the host in source order, using monotonic elapsed time.
#[derive(Debug)]
pub enum DialogEvent {
    /// A structurally checked received message. Host retains rejected raw receipts.
    Received(Box<Message>),
    /// Host policy accepts or rejects the inspected device.
    HelloDecision(bool),
    /// A specific durable storage request completed, possibly out of order.
    Committed {
        /// Ticket emitted with the store request.
        ticket: CommitId,
        /// Durable outcome, never inferred from a timeout.
        outcome: CommitOutcome,
    },
    /// Request observations again after the previous topic has ended.
    RequestObservations,
    /// End an idle conversation normally.
    Finish,
    /// Abort on operator request, without replaying a topic.
    Abort,
    /// The reliable transport has disconnected.
    Disconnected,
    /// Advance deadlines without any received input.
    Tick,
}

/// Ordered side effects for the host. No action performs I/O itself.
#[derive(Debug)]
pub enum DialogAction {
    /// Ask host policy about the device; no acceptance has been sent yet.
    InspectHello {
        /// Source device identity.
        device_id: String,
        /// Advertised optional capabilities, if present.
        capabilities: Option<Capabilities>,
    },
    /// Persist raw bytes and semantic outcome in the protected store.
    Store {
        /// Identity to echo only after durable completion.
        ticket: CommitId,
        /// Exact received evidence and typed content.
        message: Box<Message>,
    },
    /// Build and issue this message in action order; host supplies creation time.
    Send {
        /// Unique conversation-local identity for the outgoing message.
        control_id: ControlId,
        /// Typed reviewer body.
        body: Outbound,
    },
    /// Resume or pause transport reads; TCP flow control provides backpressure.
    ReadEnabled(bool),
    /// Caller submitted input despite backpressure; retain and retry this message.
    Deferred(Box<Message>),
    /// Observations topic ended after all prior ACKs were issued.
    TopicComplete,
    /// Close this transport. Pending store operations remain owned by the host.
    Close {
        /// Whether the conversation completed its termination handshake.
        normal: bool,
        /// Unresolved tickets for recovery; timeout never implies store failure.
        unresolved: Vec<CommitId>,
    },
}

#[derive(Debug)]
enum Phase {
    Hello,
    Inspect(Box<Message>),
    HelloCommit,
    Status,
    StatusCommit,
    Observations(ControlId),
    DrainTopic,
    Ready,
    RemoteEnd,
    Ending(ControlId),
    Closed,
}

#[derive(Debug, Clone, Copy)]
enum Purpose {
    Hello,
    Status,
    Observation,
    Termination,
}

#[derive(Debug)]
struct Pending {
    ticket: CommitId,
    reference: ControlId,
    purpose: Purpose,
    deadline: Duration,
    outcome: Option<CommitOutcome>,
}

/// One deterministic reviewer conversation. Dropping it never cancels a store.
/// The host owns store tasks and recovery, and must retain every raw receipt
/// before discarding input (including syntax errors and deferred messages).
/// Sends start their deadlines when issued as actions; the host budgets actual
/// write latency inside those deadlines and supplies a fresh session on reconnect.
#[derive(Debug)]
pub struct Dialog {
    config: DialogConfig,
    session: ControlId,
    counter: u64,
    phase: Phase,
    pending: VecDeque<Pending>,
    now: Duration,
    response_deadline: Duration,
    ack_timeout: Duration,
    device_message_limit: usize,
    reads: bool,
    hello_received: Duration,
    last_sent: Option<ControlId>,
}

impl Dialog {
    /// Creates a reviewer waiting for Hello. Capacity must be reserved by the host.
    /// # Errors
    /// Rejects zero limits/deadlines and a session too long for derived send IDs.
    pub fn new(config: DialogConfig, session: ControlId) -> Result<Self, DialogError> {
        if config.pending_messages == 0
            || config.message_bytes == 0
            || config.response_timeout.is_zero()
            || config.unannounced_ack_timeout.is_zero()
            || session.as_str().len() > 200
        {
            return Err(DialogError::Config);
        }
        Ok(Self {
            response_deadline: config.response_timeout,
            ack_timeout: config.unannounced_ack_timeout,
            device_message_limit: config.message_bytes,
            config,
            session,
            counter: 0,
            phase: Phase::Hello,
            pending: VecDeque::new(),
            now: Duration::ZERO,
            reads: true,
            hello_received: Duration::ZERO,
            last_sent: None,
        })
    }

    /// Applies one event, expiring deadlines first. Never retransmits on timeout.
    /// # Errors
    /// Rejects reversed/unrepresentable time, unknown completion tickets and host
    /// commands in the wrong phase. Refused received data is returned as Deferred.
    pub fn step(
        &mut self,
        now: Duration,
        event: DialogEvent,
    ) -> Result<Vec<DialogAction>, DialogError> {
        if now < self.now {
            return Err(DialogError::TimeReversed);
        }
        let response_deadline = now
            .checked_add(self.config.response_timeout)
            .ok_or(DialogError::Exhausted)?;
        now.checked_add(self.ack_timeout)
            .ok_or(DialogError::Exhausted)?;
        self.now = now;
        let mut actions = Vec::new();
        if !matches!(self.phase, Phase::Closed)
            && (now >= self.response_deadline || self.pending.iter().any(|p| now >= p.deadline))
        {
            self.close(false, &mut actions);
        }
        if matches!(self.phase, Phase::Closed) {
            if let DialogEvent::Received(message) = event {
                actions.push(DialogAction::Deferred(message));
            }
            return Ok(actions);
        }
        match event {
            DialogEvent::Received(message) => {
                self.response_deadline = response_deadline;
                self.receive(message, &mut actions)?;
            }
            DialogEvent::HelloDecision(accepted) => self.decide_hello(accepted, &mut actions)?,
            DialogEvent::Committed { ticket, outcome } => {
                let pending = self
                    .pending
                    .iter_mut()
                    .find(|p| p.ticket == ticket)
                    .ok_or(DialogError::Commit)?;
                if pending.outcome.is_some() {
                    return Err(DialogError::Commit);
                }
                pending.outcome = Some(outcome);
                if !matches!(self.phase, Phase::Ending(_)) {
                    self.drain(&mut actions)?;
                }
            }
            DialogEvent::RequestObservations => {
                if !matches!(self.phase, Phase::Ready) {
                    return Err(DialogError::State);
                }
                self.request(&mut actions)?;
            }
            DialogEvent::Finish => {
                if !matches!(self.phase, Phase::Ready) {
                    return Err(DialogError::State);
                }
                self.terminate(TerminationReason::Normal, &mut actions)?;
            }
            DialogEvent::Abort => self.terminate(TerminationReason::User, &mut actions)?,
            DialogEvent::Disconnected => self.close(false, &mut actions),
            DialogEvent::Tick => {}
        }
        let reads = self.pending.len() < self.config.pending_messages;
        if !matches!(self.phase, Phase::Closed) && reads != self.reads {
            self.reads = reads;
            actions.push(DialogAction::ReadEnabled(reads));
        }
        Ok(actions)
    }

    fn receive(
        &mut self,
        message: Box<Message>,
        actions: &mut Vec<DialogAction>,
    ) -> Result<(), DialogError> {
        if message.document().raw().len() > self.config.message_bytes
            || self.pending.len() >= self.config.pending_messages
        {
            actions.push(DialogAction::Deferred(message));
            return Ok(());
        }
        let reference = message.header().control_id.clone();
        match message.body() {
            Body::Hello {
                device_id,
                capabilities,
                application_timeout,
            } => {
                if !matches!(self.phase, Phase::Hello) {
                    return self.unexpected(reference, actions);
                }
                actions.push(DialogAction::InspectHello {
                    device_id: device_id.clone(),
                    capabilities: capabilities.clone(),
                });
                self.hello_received = self.now;
                if let Some(Nullable::Value(seconds)) = application_timeout
                    && let Some(timeout) = parse_seconds(seconds)
                {
                    self.ack_timeout = timeout;
                }
                self.response_deadline = self
                    .now
                    .checked_add(self.ack_timeout)
                    .ok_or(DialogError::Exhausted)?;
                self.phase = Phase::Inspect(message);
            }
            Body::Status { .. } => {
                if !matches!(self.phase, Phase::Status) {
                    return self.unexpected(reference, actions);
                }
                self.phase = Phase::StatusCommit;
                self.store(message, Purpose::Status, actions)?;
            }
            Body::PatientObservations(_) | Body::NonPatientObservations(_) => {
                if !matches!(self.phase, Phase::Observations(_)) {
                    return self.unexpected(reference, actions);
                }
                self.store(message, Purpose::Observation, actions)?;
            }
            Body::EndTopic(topic) => {
                if !matches!(self.phase, Phase::Observations(_))
                    || !matches!(topic,Nullable::Value(c) if c.value == "OBS" && c.system_name.as_deref().is_none_or(|s|s == "POCT01"))
                {
                    return self.unexpected(reference, actions);
                }
                self.phase = Phase::DrainTopic;
                self.complete_topic(actions);
            }
            Body::Terminate(_) => {
                if matches!(self.phase, Phase::Ending(_)) {
                    return self.unexpected(reference, actions);
                }
                self.phase = Phase::RemoteEnd;
                self.store(message, Purpose::Termination, actions)?;
            }
            Body::Acknowledgement { kind, reference } => {
                if let Phase::Ending(sent) = &self.phase {
                    if sent == reference {
                        self.close(*kind == AckKind::Accept, actions);
                    } else {
                        self.close(false, actions);
                    }
                } else {
                    return self.unexpected(reference.clone(), actions);
                }
            }
            Body::Escape {
                reference: escaped, ..
            } => {
                if let Phase::Observations(request) = &self.phase {
                    if request != escaped && self.last_sent.as_ref() != Some(escaped) {
                        return self.unexpected(reference, actions);
                    }
                    self.phase = Phase::DrainTopic;
                    self.complete_topic(actions);
                } else {
                    self.close(false, actions);
                }
            }
            Body::Request(_) => return self.unexpected(reference, actions),
        }
        Ok(())
    }

    fn decide_hello(
        &mut self,
        accepted: bool,
        actions: &mut Vec<DialogAction>,
    ) -> Result<(), DialogError> {
        if !matches!(self.phase, Phase::Inspect(_)) {
            return Err(DialogError::State);
        }
        let Phase::Inspect(message) = std::mem::replace(&mut self.phase, Phase::HelloCommit) else {
            return Err(DialogError::State);
        };
        let mut supported = accepted;
        if let Body::Hello {
            application_timeout,
            capabilities,
            ..
        } = message.body()
        {
            if let Some(timeout) = application_timeout {
                match timeout {
                    Nullable::Value(seconds) => {
                        if let Some(timeout) = parse_seconds(seconds) {
                            self.ack_timeout = timeout;
                        } else {
                            supported = false;
                        }
                    }
                    Nullable::Null(_) => supported = false,
                }
            }
            if let Some(capabilities) = capabilities {
                match &capabilities.maximum_message_bytes {
                    Nullable::Value(value) => {
                        if let Ok(limit) = value.parse::<usize>() {
                            self.device_message_limit = limit.min(self.config.message_bytes);
                        } else {
                            supported = false;
                        }
                    }
                    Nullable::Null(crate::Null::PositiveInfinity) => {}
                    Nullable::Null(
                        crate::Null::NoInformation
                        | crate::Null::NotApplicable
                        | crate::Null::Unknown
                        | crate::Null::NotAsked
                        | crate::Null::AskedUnknown
                        | crate::Null::NotAvailable
                        | crate::Null::Other
                        | crate::Null::NegativeInfinity,
                    ) => supported = false,
                }
            }
        }
        if !supported {
            self.send(
                Outbound::ApplicationError {
                    reference: message.header().control_id.clone(),
                    error: ApplicationError::UnsupportedValue,
                },
                actions,
            )?;
            self.close(false, actions);
        } else {
            self.store(message, Purpose::Hello, actions)?;
        }
        Ok(())
    }

    fn store(
        &mut self,
        message: Box<Message>,
        purpose: Purpose,
        actions: &mut Vec<DialogAction>,
    ) -> Result<(), DialogError> {
        let sequence = self.next()?;
        let ticket = CommitId {
            session: self.session.clone(),
            sequence,
        };
        let received = if matches!(purpose, Purpose::Hello) {
            self.hello_received
        } else {
            self.now
        };
        let deadline = received
            .checked_add(self.ack_timeout)
            .ok_or(DialogError::Exhausted)?;
        self.pending.push_back(Pending {
            ticket: ticket.clone(),
            reference: message.header().control_id.clone(),
            purpose,
            deadline,
            outcome: None,
        });
        actions.push(DialogAction::Store { ticket, message });
        Ok(())
    }

    fn drain(&mut self, actions: &mut Vec<DialogAction>) -> Result<(), DialogError> {
        while self.pending.front().is_some_and(|p| p.outcome.is_some()) {
            let pending = self.pending.pop_front().ok_or(DialogError::Commit)?;
            match pending.outcome.ok_or(DialogError::Commit)? {
                CommitOutcome::Stored => {
                    self.send(Outbound::Accept(pending.reference), actions)?;
                    match pending.purpose {
                        Purpose::Hello => {
                            if !matches!(self.phase, Phase::RemoteEnd) {
                                self.phase = Phase::Status;
                            }
                        }
                        Purpose::Status => {
                            if !matches!(self.phase, Phase::RemoteEnd) {
                                self.request(actions)?;
                            }
                        }
                        Purpose::Observation => {}
                        Purpose::Termination => {
                            self.close(true, actions);
                            break;
                        }
                    }
                }
                CommitOutcome::ApplicationError(error) => {
                    self.send(
                        Outbound::ApplicationError {
                            reference: pending.reference,
                            error,
                        },
                        actions,
                    )?;
                    match pending.purpose {
                        Purpose::Observation => {}
                        Purpose::Hello | Purpose::Status | Purpose::Termination => {
                            self.close(false, actions);
                            break;
                        }
                    }
                }
                CommitOutcome::Failed => {
                    self.send(
                        Outbound::Escape {
                            reference: pending.reference,
                            reason: EscapeReason::CannotComplete,
                        },
                        actions,
                    )?;
                    self.terminate(TerminationReason::Abnormal, actions)?;
                    break;
                }
            }
        }
        self.complete_topic(actions);
        Ok(())
    }

    fn complete_topic(&mut self, actions: &mut Vec<DialogAction>) {
        if matches!(self.phase, Phase::DrainTopic) && self.pending.is_empty() {
            self.phase = Phase::Ready;
            actions.push(DialogAction::TopicComplete);
        }
    }

    fn request(&mut self, actions: &mut Vec<DialogAction>) -> Result<(), DialogError> {
        let control_id = self.send(Outbound::RequestObservations, actions)?;
        self.phase = Phase::Observations(control_id);
        self.response_deadline = self
            .now
            .checked_add(self.config.response_timeout)
            .ok_or(DialogError::Exhausted)?;
        Ok(())
    }

    fn unexpected(
        &mut self,
        reference: ControlId,
        actions: &mut Vec<DialogAction>,
    ) -> Result<(), DialogError> {
        self.send(
            Outbound::Escape {
                reference,
                reason: EscapeReason::Other,
            },
            actions,
        )?;
        self.terminate(TerminationReason::Abnormal, actions)
    }

    fn terminate(
        &mut self,
        reason: TerminationReason,
        actions: &mut Vec<DialogAction>,
    ) -> Result<(), DialogError> {
        let id = self.send(Outbound::Terminate(reason), actions)?;
        self.phase = Phase::Ending(id);
        self.response_deadline = self
            .now
            .checked_add(self.config.response_timeout)
            .ok_or(DialogError::Exhausted)?;
        Ok(())
    }

    fn send(
        &mut self,
        body: Outbound,
        actions: &mut Vec<DialogAction>,
    ) -> Result<ControlId, DialogError> {
        let sequence = self.next()?;
        let id = ControlId::new(&format!("{}-{sequence}", self.session.as_str()))?;
        self.response_deadline = self
            .now
            .checked_add(self.config.response_timeout)
            .ok_or(DialogError::Exhausted)?;
        self.last_sent = Some(id.clone());
        actions.push(DialogAction::Send {
            control_id: id.clone(),
            body,
        });
        Ok(id)
    }

    fn next(&mut self) -> Result<u64, DialogError> {
        self.counter = self.counter.checked_add(1).ok_or(DialogError::Exhausted)?;
        Ok(self.counter)
    }

    fn close(&mut self, normal: bool, actions: &mut Vec<DialogAction>) {
        self.phase = Phase::Closed;
        self.reads = false;
        actions.push(DialogAction::ReadEnabled(false));
        actions.push(DialogAction::Close {
            normal,
            unresolved: self.pending.iter().map(|p| p.ticket.clone()).collect(),
        });
    }

    /// Returns the smaller of host and advertised device receive limits.
    /// The host passes this bound into the message builder for every send.
    #[must_use]
    pub fn outbound_message_limit(&self) -> usize {
        self.device_message_limit
    }
}

fn parse_seconds(text: &str) -> Option<Duration> {
    let (seconds, fraction) = text.split_once('.').unwrap_or((text, ""));
    if fraction.len() > 9 || !fraction.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let seconds = seconds.parse::<u64>().ok()?;
    let mut nanos = fraction.parse::<u32>().unwrap_or(0);
    for _ in fraction.len()..9 {
        nanos = nanos.checked_mul(10)?;
    }
    let value = Duration::new(seconds, nanos);
    if value.is_zero() { None } else { Some(value) }
}
