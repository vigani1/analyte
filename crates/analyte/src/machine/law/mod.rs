//! Drives LAW connections using dialog actions and durable store calls.
use super::connection_input::{self, Input};
use crate::intake::ByteReason;
pub(crate) mod custody;
mod error;
use crate::orders::outcome::Code;
pub(crate) use error::Error;
pub(super) mod independent;
use crate::intake;
use crate::orders::law::{Connection, Reply};
use analyte_dialog::law::{self as protocol, Action, Dialog, Event, Interpretation, Link};
use analyte_model::ReceiptId;
use analyte_store::Handle;
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};
use tokio::sync::{oneshot, watch};

pub(super) async fn session(
    machine: &intake::Connection<'_>,
    settings: &analyte_config::LawStack,
    store: &Handle,
    conn: &mut analyte_transport::Connection,
    stop: watch::Receiver<bool>,
) -> Result<(), Error> {
    session_with_outbound(machine, settings, store, conn, stop, None).await
}
async fn session_with_outbound(
    machine: &intake::Connection<'_>,
    settings: &analyte_config::LawStack,
    store: &Handle,
    conn: &mut analyte_transport::Connection,
    stop: watch::Receiver<bool>,
    outbound: Option<&tokio::sync::mpsc::Sender<independent::Work>>,
) -> Result<(), Error> {
    let mut orders = Connection::new()?;
    let mut host = Host::new(
        machine,
        settings,
        store,
        conn,
        stop,
        &mut orders,
        Link::Inbound,
    )?;
    host.outbound = outbound;
    let mut result = host.inbound().await;
    // Failed evidence retention must not bypass reserved-item settlement.
    let finished = host.finish().await;
    let cleanup = orders
        .close(
            store,
            if result.is_err() {
                Code::LawSessionError.as_str()
            } else {
                Code::LawDisconnect.as_str()
            },
        )
        .await;
    super::custody::retain_cleanup(&mut result, finished);
    super::custody::retain_cleanup(&mut result, cleanup.map_err(Into::into));
    result
}

/// Owns side effects and reservation cleanup. A cancelled store request may still commit;
/// the dialog cannot accept it or issue an ACK until its completion is driven explicitly.
struct Host<'a, 'm> {
    machine: &'a intake::Connection<'m>,
    settings: &'a analyte_config::LawStack,
    link: Link,
    store: &'a Handle,
    conn: &'a mut analyte_transport::Connection,
    stop: watch::Receiver<bool>,
    orders: &'a mut Connection,
    dialog: Dialog,
    clock: Instant,
    receipt: Option<(protocol::ReceiptId, ReceiptId)>,
    ready: Option<oneshot::Sender<()>>,
    stored: bool,
    outbound: Option<&'a tokio::sync::mpsc::Sender<independent::Work>>,
    stopped: bool,
    completed: bool,
    pending_input: VecDeque<u8>,
}
impl<'a, 'm> Host<'a, 'm> {
    fn new(
        machine: &'a intake::Connection<'m>,
        settings: &'a analyte_config::LawStack,
        store: &'a Handle,
        conn: &'a mut analyte_transport::Connection,
        stop: watch::Receiver<bool>,
        orders: &'a mut Connection,
        link: Link,
    ) -> Result<Self, Error> {
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
            conn,
            stop,
            orders,
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
    fn receipt(&self, token: protocol::ReceiptId) -> Result<ReceiptId, Error> {
        self.receipt
            .filter(|(expected, _)| *expected == token)
            .map(|(_, id)| id)
            .ok_or_else(|| Error::ReceiptCorrelationMissing)
    }
    async fn drive(&mut self, event: Event<'_>) -> Result<(), Error> {
        let mut actions: VecDeque<_> = self.dialog.step(self.clock.elapsed(), event)?.into();
        while let Some(action) = actions.pop_front() {
            let end = self
                .dialog
                .next_deadline()
                .map(|until| self.absolute(until))
                .transpose()?;
            let mut stop = self.stop.clone();
            if let Action::Unconsumed(byte) = action {
                self.pending_input.push_front(byte);
                continue;
            }
            let protects_raw = matches!(
                &action,
                Action::Commit(..) | Action::Rejected(..) | Action::Uncommitted(..)
            );
            let timeout_reason = if matches!(&action, Action::Write(..)) {
                error::DeadlineReason::Write
            } else {
                error::DeadlineReason::Effect
            };
            let operation = async {
                match end {
                    Some(end) if !protects_raw => {
                        tokio::time::timeout_at(end.into(), self.effect(action))
                            .await
                            .map_err(|source| Error::Deadline {
                                reason: timeout_reason,
                                source,
                            })?
                    }
                    // Custody uses bounded store deadlines, even after the wire budget expires.
                    Some(_) | None => self.effect(action).await,
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
    async fn effect(&mut self, action: Action) -> Result<Vec<Action>, Error> {
        let machine = self.machine.machine();
        let event = match action {
            Action::Commit(token, raw) => {
                let id = ReceiptId::new();
                self.receipt = Some((token, id));
                self.stored = false;
                #[cfg(feature = "test-hooks")]
                if let Some(hook) = &machine.hooks.law_precommit {
                    hook.pause(&mut self.stop).await.map_err(Error::TestHook)?;
                }
                super::custody::store_before_ack(
                    machine,
                    intake::receive(self.store, self.machine, id, raw),
                )
                .await?;
                self.stored = true;
                #[cfg(feature = "test-hooks")]
                if let Some(hook) = &machine.hooks.law_committed {
                    let mut stop = self.stop.clone();
                    tokio::select! {
                    biased;
                            () = super::stopping(&mut stop) => {
                                self.stopped = true;
                                return Ok(Vec::new());
                            },
                            result = hook.pause(&mut self.stop) => result.map_err(Error::TestHook)?,
                        }
                }
                Event::Committed(token)
            }
            Action::Query(token, message, started) => {
                return self.answer_query(token, message, started).await;
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
            Action::Failed(token, reason) => {
                intake::failed_reading(self.store, machine, self.receipt(token)?, &reason).await?;
                return Ok(Vec::new());
            }
            Action::ResponseNeeded(token) => {
                let metadata = crate::orders::hl7::response::response_metadata(
                    crate::orders::hl7::response::Profile::Law,
                );
                return Ok(self.dialog.step(
                    self.clock.elapsed(),
                    Event::Respond(token, metadata.response()?),
                )?);
            }
            Action::Write(bytes, until) => self.write_response(bytes, until).await?,
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
                self.complete_response().await?;
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
    async fn complete_response(&mut self) -> Result<(), Error> {
        self.completed = true;
        self.receipt = None;
        self.stored = false;
        if self.ready.is_some() {
            #[cfg(feature = "test-hooks")]
            if let Some(gate) = &self.machine.machine().hooks.law_gates.rsp {
                gate.pause(&mut self.stop).await.map_err(Error::TestHook)?;
            }
        }
        if let Some(ready) = self.ready.take() {
            let _ = ready.send(());
        }
        Ok(())
    }
    async fn answer_query(
        &mut self,
        token: protocol::ReceiptId,
        message: analyte_hl7::Message,
        started: Duration,
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
                orders: std::mem::replace(self.orders, Connection::new()?),
                message: work.clone(),
                ready: wait,
            };
            if let Err(error) = sender.try_send(pending) {
                *self.orders = error.into_inner().orders;
                return Err(Error::OutboundQueueUnavailableReservedWorkNotIssued);
            }
            self.ready = Some(notify);
            #[cfg(feature = "test-hooks")]
            if let Some(gate) = &self.machine.machine().hooks.law_gates.handover {
                gate.pause(&mut self.stop).await.map_err(Error::TestHook)?;
            }
            reply = Reply::Response(response.clone());
        }
        Ok(self
            .dialog
            .step(self.clock.elapsed(), Event::Reply(token, &reply, due))?)
    }
    async fn interpret_receipt(
        &mut self,
        token: protocol::ReceiptId,
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
    async fn write_response(
        &mut self,
        bytes: Vec<u8>,
        until: Duration,
    ) -> Result<Event<'static>, Error> {
        self.conn
            .write_all(&bytes, Some(self.absolute(until)?.into()))
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
    async fn input(&mut self, bytes: Vec<u8>) -> Result<(), Error> {
        if !self.pending_input.is_empty() {
            return Err(Error::InputStillAwaitsCustody);
        }
        self.pending_input = bytes.into();
        while let Some(byte) = self.pending_input.front().copied() {
            self.pending_input.pop_front();
            self.drive(Event::Byte(byte)).await?;
            if self.stopped {
                return Ok(());
            }
        }
        #[cfg(feature = "test-hooks")]
        if let Some(hook) = &self.machine.machine().hooks.input_processed {
            hook.pause(&mut self.stop).await.map_err(Error::TestHook)?;
        }
        Ok(())
    }
    async fn finish(&mut self) -> Result<(), Error> {
        for action in self.dialog.step(self.clock.elapsed(), Event::Disconnect)? {
            match action {
                Action::Rejected(error, raw) => {
                    intake::park_bytes(self.store, self.machine, raw, ByteReason::Mllp(&error))
                        .await?;
                }
                Action::Uncommitted(_, raw) if !self.stored => {
                    intake::park_bytes(self.store, self.machine, raw, ByteReason::LawInterrupted)
                        .await?;
                }
                Action::Commit(..)
                | Action::Query(..)
                | Action::WorkResponse(..)
                | Action::Interpret(..)
                | Action::Failed(..)
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
    async fn inbound(&mut self) -> Result<(), Error> {
        let mut opened = false;
        let mut idle = tokio::time::interval(Duration::from_millis(100));
        idle.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            let due = self.orders.due();
            let end = self
                .dialog
                .next_deadline()
                .map(|until| self.absolute(until))
                .transpose()?;
            let event = tokio::select! {
                biased;
                () = super::stopping(&mut self.stop) => break,
                () = wait_until(due) => {
                    self.orders.tick(self.store).await?;
                    continue;
                },
                () = wait_until(end) => {
                    self.drive(Event::Tick).await?;
                    continue;
                },
                _ = idle.tick() => {
                    if self.outbound.is_none() && opened && !self.dialog.receiving() {
                        let mut stop = self.stop.clone();
                        let outgoing = tokio::select! {
                            biased;
                            () = super::stopping(&mut stop) => break,
                            result = self.orders.idle(self.store, self.machine.machine(), self.settings.options()) => match result {
                                Ok(message) => message,
                                Err(error) => {
                                    self.orders.close(self.store, Code::LawIdleAdmissionFailed.as_str()).await?;
                                    tracing::error!(machine=self.machine.machine().id.as_str(),error_kind=%error,"LAW idle admission stopped; no command retried"); None
                                },
                            },
                        };
                        if let Some(message) = outgoing {
                            let until = self.orders.due().ok_or(Error::OutgoingDeadlineAbsent)?.saturating_duration_since(self.clock);
                            self.drive(Event::Send(&message, until)).await?;
                            if self.stopped { break; }
                        }
                    }
                    continue;
                },
                event = connection_input::receive(self.conn) => match event {
                    Ok(event) => event,
                    Err(analyte_transport::Error::Deadline(_)) => continue,
                    Err(error) => return Err(error.into()),
                },
            };
            match event {
                Input::Bytes { bytes } => {
                    self.input(bytes).await?;
                    if self.stopped {
                        return Ok(());
                    }
                }
                Input::Opened { .. } => opened = true,
                Input::Closed(_) => break,
                Input::Failed(reason) => {
                    tracing::warn!(machine=self.machine.machine().id.as_str(),error_kind=?reason,"LAW transport attempt failed");
                }
                Input::RecordingFailed(_failure) => {}
                Input::Tick => {}
            }
        }
        Ok(())
    }
    async fn outbound(&mut self, message: &analyte_hl7::Message) -> Result<(), Error> {
        loop {
            let until = self.orders.due().ok_or(Error::OutboundDeadlineAbsent)?;
            let event = tokio::select! {
                biased;
                () = super::stopping(&mut self.stop) => return Ok(()),
                event = tokio::time::timeout_at(until.into(), connection_input::receive(self.conn)) => event.map_err(Error::IndependentTransactionDeadline)??,
            };
            match event {
                Input::Opened { .. } => {
                    self.drive(Event::Send(
                        message,
                        until.saturating_duration_since(self.clock),
                    ))
                    .await?;
                }
                Input::Bytes { bytes } => {
                    self.input(bytes).await?;
                    if self.stopped {
                        return Ok(());
                    }
                    if self.completed {
                        return Ok(());
                    }
                }
                Input::Closed(None) => return Err(Error::PeerClosedBeforeOrl),
                Input::Closed(Some(_)) => {
                    return Err(Error::IndependentPeerDisconnected);
                }
                Input::Failed(reason) => {
                    return Err(Error::TransportFailed(reason));
                }
                Input::RecordingFailed(_failure) => {}
                Input::Tick => {}
            }
        }
    }
}

async fn wait_until(until: Option<Instant>) {
    match until {
        Some(until) => tokio::time::sleep_until(until.into()).await,
        None => std::future::pending().await,
    }
}
