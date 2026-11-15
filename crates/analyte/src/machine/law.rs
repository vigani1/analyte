//! Host services for LAW dialogs; protocol state and ACK policy live in analyte-dialog.
pub(super) mod independent;
use crate::orders::law::{Connection, Reply};
use crate::{Error, evidence, invalid};
use analyte_dialog::law::{self as protocol, Action, Dialog, Event, Interpretation, Link};
use analyte_model::ReceiptId;
use analyte_store::Handle;
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};
use tokio::sync::{oneshot, watch};

pub(super) async fn session(
    machine: &evidence::Connection<'_>,
    settings: &analyte_config::LawStack,
    store: &Handle,
    conn: &mut analyte_transport::Connection,
    stop: watch::Receiver<bool>,
) -> Result<(), Error> {
    session_with_outbound(machine, settings, store, conn, stop, None).await
}
async fn session_with_outbound(
    machine: &evidence::Connection<'_>,
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
    let result = host.inbound().await;
    host.finish().await?;
    orders
        .close(
            store,
            if result.is_err() {
                "LAW-session-error"
            } else {
                "LAW-disconnect"
            },
        )
        .await?;
    result
}

/// Owns side effects and reservation cleanup. A cancelled store request may still commit;
/// the dialog cannot accept it or issue an ACK until its completion is driven explicitly.
struct Host<'a, 'm> {
    machine: &'a evidence::Connection<'m>,
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
        machine: &'a evidence::Connection<'m>,
        settings: &'a analyte_config::LawStack,
        store: &'a Handle,
        conn: &'a mut analyte_transport::Connection,
        stop: watch::Receiver<bool>,
        orders: &'a mut Connection,
        link: Link,
    ) -> Result<Self, Error> {
        let mut options = settings.options();
        options.limits.message_bytes = options.limits.message_bytes.min(settings.frame_limit);
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
                Duration::from_millis(u64::from(settings.timeouts.receive_timeout_ms)),
                Duration::from_millis(u64::from(settings.timeouts.send_timeout_ms)),
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
            .ok_or_else(|| invalid("LAW connection deadline overflow"))
    }
    fn receipt(&self, token: protocol::ReceiptId) -> Result<ReceiptId, Error> {
        self.receipt
            .filter(|(expected, _)| *expected == token)
            .map(|(_, id)| id)
            .ok_or_else(|| invalid("LAW receipt correlation missing"))
    }
    async fn drive(&mut self, event: Event<'_>) -> Result<(), Error> {
        let mut actions: VecDeque<_> = self.dialog.step(self.clock.elapsed(), event)?.into();
        while let Some(action) = actions.pop_front() {
            let end = self
                .dialog
                .deadline()
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
                "LAW response send deadline expired"
            } else {
                "LAW processing deadline expired; no acceptance promised"
            };
            let operation = async {
                match end {
                    Some(end) => tokio::time::timeout_at(end.into(), self.effect(action))
                        .await
                        .map_err(|_| invalid(timeout_reason))?,
                    None => self.effect(action).await,
                }
            };
            let effects = if protects_raw {
                operation.await?
            } else {
                tokio::select! { biased;
                    () = super::stopping(&mut stop) => { self.stopped = true; return Ok(()); },
                    result = operation => result?,
                }
            };
            actions.extend(effects);
        }
        Ok(())
    }
    async fn effect(&mut self, action: Action) -> Result<Vec<Action>, Error> {
        let event = match action {
            Action::Commit(token, raw) => {
                let id = ReceiptId::new();
                self.receipt = Some((token, id));
                self.stored = false;
                evidence::receive(self.store, self.machine, id, raw).await?;
                self.stored = true;
                #[cfg(feature = "test-hooks")]
                if let Some(hook) = &self.machine.law_committed {
                    let mut stop = self.stop.clone();
                    tokio::select! { biased;
                        () = super::stopping(&mut stop) => { self.stopped = true; return Ok(Vec::new()); },
                        result = hook.pause(&mut self.stop) => result?,
                    }
                }
                Event::Committed(token)
            }
            Action::Query(token, message, started) => {
                let mut reply = self
                    .orders
                    .answer(
                        self.store,
                        self.machine,
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
                if let (Reply::Followup { response, work }, Some(sender)) = (&reply, self.outbound)
                {
                    let (notify, wait) = oneshot::channel();
                    let pending = independent::Work {
                        orders: std::mem::replace(self.orders, Connection::new()?),
                        message: work.clone(),
                        ready: wait,
                    };
                    if let Err(error) = sender.try_send(pending) {
                        *self.orders = error.into_inner().orders;
                        return Err(invalid(
                            "LAW outbound queue unavailable; reserved work not issued",
                        ));
                    }
                    self.ready = Some(notify);
                    reply = Reply::Response(response.clone());
                }
                return Ok(self
                    .dialog
                    .step(self.clock.elapsed(), Event::Reply(token, &reply, due))?);
            }
            Action::WorkResponse(token, message) => {
                self.orders
                    .orl(self.store, self.machine, &message, self.receipt(token)?)
                    .await?;
                Event::Complete(token)
            }
            Action::Interpret(token) => {
                let (outcome, location) = evidence::interpret_with_location(
                    self.store,
                    self.machine,
                    self.receipt(token)?,
                )
                .await?;
                let detail = super::error_location::build(location.as_ref(), b"", b"");
                let location = location.as_ref().map(|_| protocol::Location {
                    segment: detail.segment,
                    occurrence: detail.occurrence,
                    field: detail.field,
                });
                let interpreted = match outcome {
                    evidence::InterpretationOutcome::Accepted => Interpretation::Accepted,
                    evidence::InterpretationOutcome::WorkConflict(location) => {
                        Interpretation::WorkConflict(protocol::Location {
                            segment: location.segment,
                            occurrence: location.occurrence,
                            field: location.field,
                        })
                    }
                    evidence::InterpretationOutcome::Conflict => Interpretation::Conflict,
                    evidence::InterpretationOutcome::Failed => Interpretation::Failed(location),
                };
                return Ok(self
                    .dialog
                    .step(self.clock.elapsed(), Event::Interpreted(token, interpreted))?);
            }
            Action::Failed(token, reason) => {
                evidence::failed_reading(self.store, self.machine, self.receipt(token)?, &reason)
                    .await?;
                return Ok(Vec::new());
            }
            Action::ResponseNeeded(token) => {
                let metadata =
                    crate::hl7_response::response_metadata(crate::hl7_response::Profile::Law);
                return Ok(self.dialog.step(
                    self.clock.elapsed(),
                    Event::Respond(token, metadata.response()?),
                )?);
            }
            Action::Write(bytes, until) => {
                self.conn
                    .write_all(&bytes, Some(self.absolute(until)?.into()))
                    .await
                    .map_err(|error| {
                        if matches!(error, analyte_transport::Error::Deadline("write all")) {
                            invalid("LAW response send deadline expired")
                        } else {
                            error.into()
                        }
                    })?;
                Event::Written
            }
            Action::WorkIssued => {
                self.orders.issued(Duration::from_millis(u64::from(
                    self.settings.policy.response_ms,
                )))?;
                if self.link == Link::WorkResponse {
                    let until = self
                        .orders
                        .due()
                        .ok_or("LAW response deadline absent after issuance")?
                        .saturating_duration_since(self.clock);
                    return Ok(self
                        .dialog
                        .step(self.clock.elapsed(), Event::WorkDeadline(until))?);
                }
                return Ok(Vec::new());
            }
            Action::Done => {
                self.completed = true;
                self.receipt = None;
                self.stored = false;
                if let Some(ready) = self.ready.take() {
                    let _ = ready.send(());
                }
                return Ok(Vec::new());
            }
            Action::Unconsumed(byte) => {
                self.pending_input.push_front(byte);
                return Ok(Vec::new());
            }
            Action::Uncommitted(_, raw) => {
                if !self.stored {
                    evidence::park_bytes(
                        self.store,
                        self.machine,
                        raw,
                        "LAW receipt processing interrupted before commit confirmation",
                    )
                    .await?;
                }
                return Ok(Vec::new());
            }
            Action::Rejected(error, raw) => {
                evidence::park_bytes(self.store, self.machine, raw, &error.to_string()).await?;
                return Ok(Vec::new());
            }
            Action::Refused(error) => return Err(error.into()),
            Action::Deadline(_) => return Ok(Vec::new()),
        };
        Ok(self.dialog.step(self.clock.elapsed(), event)?)
    }
    async fn input(&mut self, bytes: Vec<u8>) -> Result<(), Error> {
        if !self.pending_input.is_empty() {
            return Err(invalid("LAW input still awaits custody"));
        }
        self.pending_input = bytes.into();
        while let Some(byte) = self.pending_input.front().copied() {
            self.pending_input.pop_front();
            self.drive(Event::Byte(byte)).await?;
            if self.stopped {
                return Ok(());
            }
        }
        Ok(())
    }
    async fn finish(&mut self) -> Result<(), Error> {
        for action in self.dialog.step(self.clock.elapsed(), Event::Disconnect)? {
            match action {
                Action::Rejected(error, raw) => {
                    evidence::park_bytes(self.store, self.machine, raw, &error.to_string()).await?;
                }
                Action::Uncommitted(_, raw) if !self.stored => {
                    evidence::park_bytes(
                        self.store,
                        self.machine,
                        raw,
                        "LAW receipt processing interrupted before commit confirmation",
                    )
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
            evidence::park_bytes(
                self.store,
                self.machine,
                self.pending_input.drain(..).collect(),
                "LAW unprocessed input when session ended",
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
                .deadline()
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
                            result = self.orders.idle(self.store, self.machine, self.settings.options()) => match result {
                                Ok(message) => message,
                                Err(error) => {
                                    self.orders.close(self.store, "LAW-idle-admission-failed").await?;
                                    tracing::error!(machine=self.machine.id.as_str(),error_kind=%error,"LAW idle admission stopped; no command retried"); None
                                },
                            },
                        };
                        if let Some(message) = outgoing {
                            let until = self.orders.due().ok_or("LAW outgoing deadline absent")?.saturating_duration_since(self.clock);
                            self.drive(Event::Send(&message, until)).await?;
                            if self.stopped { break; }
                        }
                    }
                    continue;
                },
                event = self.conn.receive() => match event {
                    Ok(Some(event)) => event, Ok(None) => break,
                    Err(analyte_transport::Error::Deadline(_)) => continue,
                    Err(error) => return Err(error.into()),
                },
            };
            match event {
                analyte_transport::Event::Received { bytes, .. } => {
                    self.input(bytes).await?;
                    if self.stopped {
                        return Ok(());
                    }
                }
                analyte_transport::Event::Opened { .. } => opened = true,
                analyte_transport::Event::Closed(_) => break,
                analyte_transport::Event::Failed(reason) => {
                    tracing::warn!(machine=self.machine.id.as_str(),error_kind=?reason,"LAW transport attempt failed");
                }
                analyte_transport::Event::RecordingFailed(_) => {}
            }
        }
        Ok(())
    }
    async fn outbound(&mut self, message: &analyte_hl7::Message) -> Result<(), Error> {
        loop {
            let until = self.orders.due().ok_or("LAW outbound deadline absent")?;
            let event = tokio::select! {
                biased;
                () = super::stopping(&mut self.stop) => return Ok(()),
                event = tokio::time::timeout_at(until.into(), self.conn.receive()) => event.map_err(|_| invalid("LAW independent transaction deadline"))??,
            }.ok_or("LAW independent peer closed before ORL")?;
            match event {
                analyte_transport::Event::Opened { .. } => {
                    self.drive(Event::Send(
                        message,
                        until.saturating_duration_since(self.clock),
                    ))
                    .await?;
                }
                analyte_transport::Event::Received { bytes, .. } => {
                    self.input(bytes).await?;
                    if self.stopped {
                        return Ok(());
                    }
                    if self.completed {
                        return Ok(());
                    }
                }
                analyte_transport::Event::Closed(_) => {
                    return Err(invalid("LAW independent peer disconnected"));
                }
                analyte_transport::Event::Failed(reason) => {
                    return Err(invalid(&format!(
                        "LAW independent transport failed: {reason:?}"
                    )));
                }
                analyte_transport::Event::RecordingFailed(_) => {}
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
