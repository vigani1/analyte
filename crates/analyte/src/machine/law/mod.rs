//! Drives LAW connections using dialog actions and durable store calls.
//! Kept whole: Keeps connection modes, ordered effects and reservation cleanup together.
pub(crate) mod policy;
use super::connection_input::Input;
use super::driver::{self, Context, Effects, Flow};
use crate::intake::ByteReason;
use crate::machine::hooks::Pause;
pub(crate) mod custody;
mod error;
use crate::orders::outcome::Code;
pub(crate) use error::Error;
pub(super) mod independent;
use crate::intake;
use crate::orders::law::{LawOrderLink, Reply};
use analyte_dialog::law::{self as protocol, Action, Dialog, Event, Interpretation, Link};
use analyte_model::ReceiptId;
use analyte_store::Handle;
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};
use tokio::sync::{oneshot, watch};

pub(super) async fn session(
    machine: &intake::ConnectionEvidence<'_>,
    settings: &analyte_config::LawStack,
    store: &Handle,
    conn: &mut analyte_transport::Connection,
    stop: watch::Receiver<bool>,
) -> Result<(), Error> {
    session_with_outbound(machine, settings, store, conn, stop, None).await
}
async fn session_with_outbound(
    machine: &intake::ConnectionEvidence<'_>,
    settings: &analyte_config::LawStack,
    store: &Handle,
    conn: &mut analyte_transport::Connection,
    stop: watch::Receiver<bool>,
    outbound: Option<&tokio::sync::mpsc::Sender<independent::Work>>,
) -> Result<(), Error> {
    let mut orders = LawOrderLink::new()?;
    driver::run(
        || {
            let mut host = Host::open(machine, settings, store, &mut orders, Mode::inbound())?;
            host.outbound = outbound;
            Ok(host)
        },
        Context {
            machine,
            store,
            conn,
            stop,
        },
    )
    .await
}

/// Owns side effects and reservation cleanup. A cancelled store request may still commit;
/// the dialog cannot accept it or issue an ACK until its completion is driven explicitly.
struct Host<'a, 'm> {
    machine: &'a intake::ConnectionEvidence<'m>,
    settings: &'a analyte_config::LawStack,
    link: Link,
    store: &'a Handle,
    orders: &'a mut LawOrderLink,
    dialog: Dialog,
    mode: Mode<'a>,
    clock: Instant,
    receipt: Option<(protocol::ExchangeId, ReceiptId)>,
    ready: Option<oneshot::Sender<()>>,
    stored: bool,
    outbound: Option<&'a tokio::sync::mpsc::Sender<independent::Work>>,
    stopped: bool,
    completed: bool,
    pending_input: VecDeque<u8>,
}
impl<'a, 'm> Host<'a, 'm> {
    fn open(
        machine: &'a intake::ConnectionEvidence<'m>,
        settings: &'a analyte_config::LawStack,
        store: &'a Handle,
        orders: &'a mut LawOrderLink,
        mode: Mode<'a>,
    ) -> Result<Self, Error> {
        let link = match mode {
            Mode::Inbound { .. } => Link::Inbound,
            Mode::Outbound(_) => Link::WorkResponse,
        };
        let mut options = settings.options();
        options.limits.message_bytes = options
            .limits
            .message_bytes
            .min(settings.settings().frame_limit());
        Ok(Self {
            machine,
            settings,
            link,
            store,
            orders,
            mode,
            dialog: Dialog::new(
                options,
                Duration::from_millis(u64::from(
                    settings.settings().timeouts().receive_timeout_ms(),
                )),
                Duration::from_millis(u64::from(settings.settings().timeouts().send_timeout_ms())),
                link,
            )?,
            clock: Instant::now(),
            receipt: None,
            ready: None,
            stored: false,
            outbound: None,
            stopped: false,
            completed: false,
            pending_input: VecDeque::new(),
        })
    }
    fn absolute(&self, until: Duration) -> Result<Instant, Error> {
        self.clock
            .checked_add(until)
            .ok_or_else(|| Error::ConnectionDeadlineOverflow)
    }
    fn receipt(&self, token: protocol::ExchangeId) -> Result<ReceiptId, Error> {
        self.receipt
            .filter(|(expected, _)| *expected == token)
            .map(|(_, id)| id)
            .ok_or_else(|| Error::ReceiptCorrelationMissing)
    }
    async fn transition(
        &mut self,
        event: Event<'_>,
        host: &mut Context<'_, '_>,
    ) -> Result<(), Error> {
        let actions = self.dialog.step(self.clock.elapsed(), event)?;
        self.apply(actions, host).await
    }
    async fn apply(
        &mut self,
        actions: Vec<Action>,
        host: &mut Context<'_, '_>,
    ) -> Result<(), Error> {
        let mut actions: VecDeque<_> = actions.into();
        while let Some(action) = actions.pop_front() {
            let end = self
                .dialog
                .next_deadline()
                .map(|until| self.absolute(until))
                .transpose()?;
            let mut stop = host.stop.clone();
            if let Action::Unconsumed(byte) = action {
                self.pending_input.push_front(byte);
                continue;
            }
            let protects_raw = matches!(
                &action,
                Action::Store(..) | Action::Rejected(..) | Action::Uncommitted(..)
            );
            let timeout_reason = if matches!(&action, Action::Write(..)) {
                error::DeadlineReason::Write
            } else {
                error::DeadlineReason::Effect
            };
            let operation = async {
                match end {
                    Some(end) if !protects_raw => {
                        tokio::time::timeout_at(end.into(), self.effect(action, host))
                            .await
                            .map_err(|source| Error::Deadline {
                                reason: timeout_reason,
                                source,
                            })?
                    }
                    // Custody uses bounded store deadlines, even after the wire budget expires.
                    Some(_) | None => self.effect(action, host).await,
                }
            };
            let effects = if protects_raw {
                operation.await?
            } else {
                tokio::select! {
                biased;
                    () = super::stopping(&mut stop) => {
                        self.stopped = true;
                        return Ok(());
                    },
                    result = operation => result?,
                }
            };
            actions.extend(effects);
        }
        Ok(())
    }
    async fn effect(
        &mut self,
        action: Action,
        host: &mut Context<'_, '_>,
    ) -> Result<Vec<Action>, Error> {
        let machine = self.machine.machine();
        let event = match action {
            Action::Store(token, raw) => {
                let id = ReceiptId::new();
                self.receipt = Some((token, id));
                self.stored = false;

                machine
                    .hooks
                    .pause(Pause::LawPrecommit, &mut host.stop)
                    .await
                    .map_err(Error::from)?;

                super::custody::store_before_ack(
                    machine,
                    intake::receive(self.store, self.machine, id, raw),
                )
                .await?;
                self.stored = true;

                if machine.hooks.enabled(Pause::LawCommitted) {
                    let mut stop = host.stop.clone();
                    tokio::select! {
                    biased;
                            () = super::stopping(&mut stop) => {
                                self.stopped = true;
                                return Ok(Vec::new());
                            },
                            result = machine.hooks.pause(Pause::LawCommitted, &mut host.stop) => result.map_err(Error::from)?,
                        }
                }
                Event::Stored(token)
            }
            Action::Query(token, message, started) => {
                return self.answer_query(token, message, started, host).await;
            }
            Action::WorkResponse(token, message) => {
                self.orders
                    .orl(self.store, machine, &message, self.receipt(token)?)
                    .await?;
                Event::Complete(token)
            }
            Action::Interpret(token) => {
                return self.interpret_receipt(token).await;
            }
            Action::Parked(token, reason) => return self.park_failure(token, reason).await,
            Action::ResponseNeeded(token) => {
                let metadata = crate::orders::hl7::response::response_metadata(
                    crate::orders::hl7::response::Profile::Law,
                );
                return Ok(self.dialog.step(
                    self.clock.elapsed(),
                    Event::Respond(token, metadata.response()?),
                )?);
            }
            Action::Write(write) => self.write_response(write, host).await?,
            Action::WorkIssued => {
                self.orders.issued(Duration::from_millis(u64::from(
                    self.settings.policy().response_ms(),
                )))?;
                if self.link == Link::WorkResponse {
                    let until = self
                        .orders
                        .due()
                        .ok_or(Error::ResponseDeadlineAbsentAfterIssuance)?
                        .saturating_duration_since(self.clock);
                    return Ok(self
                        .dialog
                        .step(self.clock.elapsed(), Event::WorkDeadline(until))?);
                }
                return Ok(Vec::new());
            }
            Action::Done => {
                self.complete_response(host).await?;
                return Ok(Vec::new());
            }
            Action::Unconsumed(byte) => {
                self.pending_input.push_front(byte);
                return Ok(Vec::new());
            }
            Action::Uncommitted(_, raw) => {
                if !self.stored {
                    intake::park_bytes(self.store, self.machine, raw, ByteReason::LawInterrupted)
                        .await?;
                }
                return Ok(Vec::new());
            }
            Action::Rejected(error, raw) => {
                intake::park_bytes(self.store, self.machine, raw, ByteReason::Mllp(&error)).await?;
                return Ok(Vec::new());
            }
            Action::Refused(error) => return Err(error.into()),
            Action::Deadline(_) => return Ok(Vec::new()),
        };
        Ok(self.dialog.step(self.clock.elapsed(), event)?)
    }
    async fn complete_response(&mut self, host: &mut Context<'_, '_>) -> Result<(), Error> {
        self.completed = true;
        self.receipt = None;
        self.stored = false;
        if self.ready.is_some() {
            self.machine
                .machine()
                .hooks
                .pause(Pause::LawRsp, &mut host.stop)
                .await
                .map_err(Error::from)?;
        }
        if let Some(ready) = self.ready.take() {
            let _ = ready.send(());
        }
        Ok(())
    }
    async fn answer_query(
        &mut self,
        token: protocol::ExchangeId,
        message: analyte_hl7::Message,
        started: Duration,
        host: &mut Context<'_, '_>,
    ) -> Result<Vec<Action>, Error> {
        let mut reply = self
            .orders
            .answer(
                self.store,
                self.machine.machine(),
                &message,
                self.receipt(token)?,
                self.absolute(started)?,
                self.settings.options(),
            )
            .await?;
        let due = self
            .orders
            .due()
            .map(|until| until.saturating_duration_since(self.clock));
        if let (Reply::Followup { response, work }, Some(sender)) = (&reply, self.outbound) {
            let (notify, wait) = oneshot::channel();
            let pending = independent::Work {
                orders: std::mem::replace(self.orders, LawOrderLink::new()?),
                message: work.clone(),
                ready: wait,
            };
            if let Err(error) = sender.try_send(pending) {
                *self.orders = error.into_inner().orders;
                return Err(Error::OutboundQueueUnavailable);
            }
            self.ready = Some(notify);

            self.machine
                .machine()
                .hooks
                .pause(Pause::LawHandover, &mut host.stop)
                .await
                .map_err(Error::from)?;

            reply = Reply::Response(response.clone());
        }
        Ok(self
            .dialog
            .step(self.clock.elapsed(), Event::Reply(token, &reply, due))?)
    }
    async fn interpret_receipt(
        &mut self,
        token: protocol::ExchangeId,
    ) -> Result<Vec<Action>, Error> {
        let (outcome, location) = intake::interpret_with_location(
            self.store,
            self.machine.machine(),
            self.receipt(token)?,
        )
        .await?;
        let detail = crate::machine::hl7::error_location::build(location.as_ref(), b"", b"");
        let location = location.as_ref().map(|_| protocol::Location {
            segment: detail.segment,
            occurrence: detail.occurrence,
            field: detail.field,
        });
        let interpreted = match outcome {
            intake::InterpretationOutcome::Accepted => Interpretation::Accepted,
            intake::InterpretationOutcome::WorkConflict(location) => {
                Interpretation::WorkConflict(protocol::Location {
                    segment: location.segment,
                    occurrence: location.occurrence,
                    field: location.field,
                })
            }
            intake::InterpretationOutcome::Conflict => Interpretation::Conflict,
            intake::InterpretationOutcome::Failed => Interpretation::Failed(location),
        };
        Ok(self
            .dialog
            .step(self.clock.elapsed(), Event::Interpreted(token, interpreted))?)
    }
    async fn park_failure(
        &self,
        token: protocol::ExchangeId,
        reason: protocol::Failure,
    ) -> Result<Vec<Action>, Error> {
        intake::failed_reading(
            self.store,
            self.machine.machine(),
            self.receipt(token)?,
            &reason.to_string(),
        )
        .await?;
        Ok(Vec::new())
    }
    async fn write_response(
        &mut self,
        write: analyte_dialog::Write,
        host: &mut Context<'_, '_>,
    ) -> Result<Event<'static>, Error> {
        let until = write.until.ok_or(protocol::Error::Stale)?;
        host.conn
            .write_all(&write.bytes, Some(self.absolute(until)?.into()))
            .await
            .map_err(|error| {
                if matches!(error, analyte_transport::Error::Deadline("write all")) {
                    Error::ResponseSendDeadlineExpired(error)
                } else {
                    error.into()
                }
            })?;
        Ok(Event::Written)
    }
    async fn feed(&mut self, bytes: Vec<u8>, host: &mut Context<'_, '_>) -> Result<(), Error> {
        if !self.pending_input.is_empty() {
            return Err(Error::InputStillAwaitsCustody);
        }
        self.pending_input = bytes.into();
        while let Some(byte) = self.pending_input.front().copied() {
            self.pending_input.pop_front();
            self.transition(Event::Byte(byte), host).await?;
            if self.stopped {
                return Ok(());
            }
        }

        self.machine
            .machine()
            .hooks
            .pause(Pause::InputProcessed, &mut host.stop)
            .await
            .map_err(Error::from)?;

        Ok(())
    }
    async fn finish(&mut self) -> Result<(), Error> {
        for action in self
            .dialog
            .step(self.clock.elapsed(), Event::Disconnected)?
        {
            match action {
                Action::Rejected(error, raw) => {
                    intake::park_bytes(self.store, self.machine, raw, ByteReason::Mllp(&error))
                        .await?;
                }
                Action::Uncommitted(_, raw) if !self.stored => {
                    intake::park_bytes(self.store, self.machine, raw, ByteReason::LawInterrupted)
                        .await?;
                }
                Action::Store(..)
                | Action::Query(..)
                | Action::WorkResponse(..)
                | Action::Interpret(..)
                | Action::Parked(..)
                | Action::ResponseNeeded(..)
                | Action::Write(..)
                | Action::WorkIssued
                | Action::Done
                | Action::Uncommitted(..)
                | Action::Unconsumed(_)
                | Action::Refused(_)
                | Action::Deadline(_) => {}
            }
        }
        if !self.pending_input.is_empty() {
            intake::park_bytes(
                self.store,
                self.machine,
                self.pending_input.drain(..).collect(),
                ByteReason::LawTail,
            )
            .await?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
pub(super) enum Mode<'a> {
    Inbound { opened: bool, next_idle: Instant },
    Outbound(&'a analyte_hl7::Message),
}
impl Mode<'_> {
    pub(super) fn inbound() -> Self {
        Self::Inbound {
            opened: false,
            next_idle: Instant::now(),
        }
    }
}
impl Effects for Host<'_, '_> {
    type Error = Error;
    fn wait(&self) -> Result<Option<Duration>, Error> {
        let until = match self.mode {
            Mode::Outbound(_) => self.orders.due().ok_or(Error::OutboundDeadlineAbsent)?,
            Mode::Inbound { next_idle, .. } => {
                let dialog = self
                    .dialog
                    .next_deadline()
                    .map(|until| self.absolute(until))
                    .transpose()?;
                [Some(next_idle), self.orders.due(), dialog]
                    .into_iter()
                    .flatten()
                    .min()
                    .ok_or(Error::ConnectionDeadlineOverflow)?
            }
        };
        Ok(Some(until.saturating_duration_since(Instant::now())))
    }
    fn receive_error(&self, error: analyte_transport::Error) -> Result<Input, Error> {
        match (self.mode, error) {
            (Mode::Inbound { .. }, analyte_transport::Error::Deadline(_)) => Ok(Input::Tick),
            (Mode::Inbound { .. } | Mode::Outbound(_), error) => Err(error.into()),
        }
    }
    async fn input(&mut self, host: &mut Context<'_, '_>, input: Input) -> Result<Flow, Error> {
        match input {
            Input::Tick => self.tick(host).await?,
            Input::Opened { .. } => match &mut self.mode {
                Mode::Inbound { opened, .. } => *opened = true,
                Mode::Outbound(message) => {
                    let message = *message;
                    let until = self
                        .orders
                        .due()
                        .ok_or(Error::OutboundDeadlineAbsent)?
                        .saturating_duration_since(self.clock);
                    self.transition(Event::Send(message, until), host).await?;
                }
            },
            Input::Bytes { bytes } => self.feed(bytes, host).await?,
            Input::Closed(reason) => match (self.mode, reason) {
                (Mode::Inbound { .. }, _) => return Ok(Flow::Stop),
                (Mode::Outbound(_), None) => return Err(Error::PeerClosedBeforeOrl),
                (Mode::Outbound(_), Some(_)) => return Err(Error::IndependentPeerDisconnected),
            },
            Input::Failed(reason) => match self.mode {
                Mode::Inbound { .. } => {
                    tracing::warn!(machine=self.machine.machine().id.as_str(),error_kind=?reason,"LAW transport attempt failed");
                }
                Mode::Outbound(_) => return Err(Error::TransportFailed(reason)),
            },
            Input::RecordingFailed(_failure) => {}
        }
        Ok(
            if self.stopped || matches!(self.mode, Mode::Outbound(_)) && self.completed {
                Flow::Stop
            } else {
                Flow::Continue
            },
        )
    }
    async fn close(
        &mut self,
        _host: &mut Context<'_, '_>,
        result: &mut Result<(), Error>,
    ) -> Result<(), Error> {
        let finished = self.finish().await;
        let cleanup = match self.mode {
            Mode::Inbound { .. } => self
                .orders
                .close(
                    self.store,
                    if result.is_err() {
                        Code::LawSessionError.as_str()
                    } else {
                        Code::LawDisconnect.as_str()
                    },
                )
                .await
                .map_err(Into::into),
            Mode::Outbound(_) => Ok(()),
        };
        crate::machine::custody::retain_cleanup(result, finished);
        cleanup
    }
}
impl Host<'_, '_> {
    async fn tick(&mut self, host: &mut Context<'_, '_>) -> Result<(), Error> {
        let now = Instant::now();
        match self.mode {
            Mode::Outbound(_) => {
                let until = self.orders.due().ok_or(Error::OutboundDeadlineAbsent)?;
                tokio::time::timeout_at(until.into(), std::future::pending::<()>())
                    .await
                    .map_err(Error::IndependentTransactionDeadline)?;
            }
            Mode::Inbound { opened, next_idle } => {
                if self.orders.due().is_some_and(|due| now >= due) {
                    self.orders.tick(self.store).await?;
                } else if self
                    .dialog
                    .next_deadline()
                    .is_some_and(|due| self.clock.elapsed() >= due)
                {
                    self.transition(Event::Tick, host).await?;
                } else if now >= next_idle {
                    let next_idle = now
                        .checked_add(Duration::from_millis(100))
                        .ok_or(Error::ConnectionDeadlineOverflow)?;
                    self.mode = Mode::Inbound { opened, next_idle };
                    if self.outbound.is_none() && opened && !self.dialog.receiving() {
                        let mut stop = host.stop.clone();
                        let outgoing = tokio::select! {
                            biased;
                            () = crate::machine::stopping(&mut stop) => { self.stopped = true; return Ok(()); },
                            result = self.orders.idle(self.store, self.machine.machine(), self.settings.options()) => match result {
                                Ok(message) => message,
                                Err(error) => {
                                    self.orders.close(self.store, Code::LawIdleAdmissionFailed.as_str()).await?;
                                    tracing::error!(machine=self.machine.machine().id.as_str(),error_kind=%error,"LAW idle admission stopped; no command retried");
                                    None
                                }
                            },
                        };
                        if let Some(message) = outgoing {
                            let until = self
                                .orders
                                .due()
                                .ok_or(Error::OutgoingDeadlineAbsent)?
                                .saturating_duration_since(self.clock);
                            self.transition(Event::Send(&message, until), host).await?;
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
