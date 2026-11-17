//! Runs an Autolyser session: polls for reports, stores each before acknowledging, downloads worklists between polls, and holds next-report (R) polls during last-report (L) recovery.
use crate::intake::ByteReason;
use crate::machine::hooks::Pause;
mod custody;
use super::driver::{self, Context, Effects, Flow, Run, Trigger};
use crate::intake;
use crate::machine::connection_input::Input;
use crate::machine::receive;
use crate::machine::receive::custody::{WriteDeadline, ms};
mod error;
mod outcome;
pub(crate) mod settings;
use analyte_dialog::autolyser::{Action, Dialog, Event, Request};
use analyte_model::ReceiptId;
use analyte_store::Handle;
pub(crate) use error::Error;
use std::time::Instant;
use tokio::sync::watch;

/// Where the host is between machine exchanges; recovery ends only with a completed report.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Busy,
    Ready,
    Recovering,
}

impl Phase {
    /// Leaves `Ready` without ending an L recovery in progress.
    fn busy(&mut self) {
        if *self != Self::Recovering {
            *self = Self::Busy;
        }
    }

    /// An insertion never ends L recovery; only a completed report does.
    fn after_insertion(&mut self, ready: bool) {
        if *self != Self::Recovering {
            *self = if ready { Self::Ready } else { Self::Busy };
        }
    }
}

/// Mutable conversation state has one owner for the entire connection.
struct Host<'a, 'm> {
    machine: &'a intake::ConnectionEvidence<'m>,
    settings: &'a analyte_config::AutolyserStack,
    store: &'a Handle,
    work: crate::orders::sending::Work<analyte_autolyser::syntax::Worklist>,
    dialog: Dialog,
    start: Instant,
    receipt: Option<(ReceiptId, jiff::Timestamp)>,
    phase: Phase,
    guard: Option<analyte_store::PollIntentId>,
    trigger: Trigger,
}

pub(super) async fn session(
    machine: &intake::ConnectionEvidence<'_>,
    settings: &analyte_config::AutolyserStack,
    store: &Handle,
    conn: &mut analyte_transport::Connection,
    stop: watch::Receiver<bool>,
) -> Result<(), Error> {
    custody::ensure_reconciled(store, machine.machine()).await?;
    driver::run(
        || Host::open(machine, settings, store),
        Context {
            machine,
            store,
            conn,
            stop,
        },
    )
    .await
}

impl<'a, 'm> Host<'a, 'm> {
    fn open(
        machine: &'a intake::ConnectionEvidence<'m>,
        settings: &'a analyte_config::AutolyserStack,
        store: &'a Handle,
    ) -> Result<Self, Error> {
        let dialog = Dialog::new(settings::dialog(settings))?;
        Ok(Self {
            machine,
            settings,
            store,
            work: crate::orders::sending::Work::default(),
            dialog,
            start: Instant::now(),
            receipt: None,
            phase: Phase::Busy,
            guard: None,
            trigger: Trigger::Exchange,
        })
    }
}

impl driver::ByteEffects for Host<'_, '_> {
    async fn apply_byte(
        &mut self,
        byte: u8,
        conn: &mut analyte_transport::Connection,
        stop: &mut watch::Receiver<bool>,
    ) -> Result<Flow, Error> {
        let actions = self.dialog.step(self.start.elapsed(), Event::Byte(byte))?;
        driver::drain(self, actions, conn, stop).await
    }
}

impl Effects for Host<'_, '_> {
    driver::chunk_effects!(Error);
    async fn input(&mut self, host: &mut Context<'_, '_>, input: Input) -> Result<Flow, Error> {
        let machine = self.machine;
        let store = self.store;
        let conn = &mut *host.conn;
        let stop = &mut host.stop;
        let ended = matches!(input, Input::Closed(_) | Input::Failed(_));
        self.trigger = Trigger::of(&input, stop);
        let events = match input {
            Input::Opened { .. } => {
                self.phase.busy();
                vec![Event::Poll(Request::Next)]
            }
            Input::Bytes { bytes } => {
                if driver::feed(self, bytes, conn, stop).await? == Flow::Stop {
                    return Ok(Flow::Stop);
                }

                if self.phase == Phase::Ready
                    && self.guard.is_none()
                    && !self.work.is_active()
                    && machine.machine().hooks.enabled(Pause::AutolyserQuiescent)
                {
                    machine
                        .machine()
                        .hooks
                        .pause(Pause::AutolyserQuiescent, stop)
                        .await
                        .map_err(Error::from)?;
                }
                Vec::new()
            }
            Input::Tick => {
                let actions = self.dialog.step(self.start.elapsed(), Event::Tick)?;
                // Tick alone can initiate L recovery. Never interleave R with it.
                if actions.is_empty() && self.phase == Phase::Ready {
                    self.phase = Phase::Busy;
                    if let Some(worklist) = self.work.poll(store, machine, self.settings).await? {
                        let actions = self
                            .dialog
                            .step(self.start.elapsed(), Event::Download(&worklist))?;
                        if driver::drain(self, actions, conn, stop).await? == Flow::Stop {
                            return Ok(Flow::Stop);
                        }
                        Vec::new()
                    } else {
                        vec![Event::Poll(Request::Next)]
                    }
                } else {
                    if driver::drain(self, actions, conn, stop).await? == Flow::Stop {
                        return Ok(Flow::Stop);
                    }
                    Vec::new()
                }
            }
            Input::Closed(_) | Input::Failed(_) => {
                vec![Event::Disconnected]
            }
        };
        for event in events {
            if matches!(event, Event::Poll(Request::Next)) {
                self.guard = Some(custody::reserve(store, machine.machine()).await?);
            }
            let actions = self.dialog.step(self.start.elapsed(), event)?;
            if driver::drain(self, actions, conn, stop).await? == Flow::Stop {
                return Ok(Flow::Stop);
            }
        }
        if ended {
            return Ok(Flow::Stop);
        }

        Ok(Flow::Continue)
    }
    async fn close(
        &mut self,
        _host: &mut Context<'_, '_>,
        result: &mut Result<(), Error>,
    ) -> Result<(), Error> {
        let code = outcome::session_end(self.trigger, result);
        let cleanup = custody::uncertain(
            self.store,
            self.guard,
            self.machine.machine(),
            code.as_str(),
        )
        .await;
        receive::custody::retain_cleanup(result, cleanup);
        self.work
            .close(self.store, code.into())
            .await
            .map_err(Into::into)
    }
}

impl<'a, 'm> Host<'a, 'm> {
    /// Stores a report before the dialog may acknowledge it. A failed store is
    /// kept until the dialog's own refusal has run; it never ends the queue early.
    async fn store_report(
        &mut self,
        token: analyte_dialog::autolyser::StoreId,
        raw: Vec<u8>,
        run: &mut Run<'_, Action, Error>,
    ) -> Result<(), Error> {
        let id = ReceiptId::new();
        let stored = receive::custody::store_before_ack(
            self.machine.machine(),
            intake::receive(self.store, self.machine, id, raw),
        )
        .await;
        let event = match stored {
            Ok(time) => {
                self.receipt = Some((id, time));
                Event::Stored(token)
            }
            Err(error) => {
                run.failures.keep(error.into());
                Event::StoreFailed(token)
            }
        };
        run.actions
            .extend(self.dialog.step(self.start.elapsed(), event)?);
        Ok(())
    }
    async fn interpret_report(
        &mut self,
        machine: &intake::ConnectionEvidence<'_>,
        store: &Handle,
    ) -> Result<(), Error> {
        let (id, _) = self
            .receipt
            .take()
            .ok_or_else(|| Error::ReportReceiptMissing)?;
        intake::interpret(store, machine.machine(), id).await?;
        self.phase = Phase::Ready;
        custody::complete(store, &mut self.guard, machine.machine()).await?;

        Ok(())
    }
}

impl driver::Drain for Host<'_, '_> {
    type Action = Action;
    async fn effect(
        &mut self,
        action: Action,
        run: &mut Run<'_, Action, Error>,
    ) -> Result<Flow, Error> {
        let machine = self.machine;
        let store = self.store;
        match action {
            Action::Write(bytes) => {
                let deadline = WriteDeadline::Within(ms(self.settings.policy().acknowledgment_ms));
                receive::custody::write(run.conn, &bytes, deadline).await?;
            }
            Action::Store(token, raw) => {
                self.store_report(token, raw, run).await?;
            }
            Action::Report(_) => {
                self.interpret_report(machine, store).await?;
            }
            Action::Park(error) => {
                if self.work.is_active() {
                    let code = outcome::parked(self.trigger, &error);
                    self.work.refuse(store, code).await?;
                    run.stop_when_drained();
                }
                self.phase = Phase::Recovering;
                let reason = format!("Autolyser: {error}");
                let parked = match self.receipt.take() {
                    Some((id, _)) => {
                        intake::failed_reading(store, machine.machine(), id, &reason).await
                    }
                    None => {
                        intake::park_bytes(
                            store,
                            machine,
                            Vec::new(),
                            ByteReason::Autolyser(&error),
                        )
                        .await
                    }
                };
                parked?;
                custody::uncertain(store, self.guard, machine.machine(), &reason).await?;
            }
            Action::Empty => {
                self.receipt.take();
                if self.phase != Phase::Recovering {
                    self.phase = Phase::Ready;
                    custody::complete(store, &mut self.guard, machine.machine()).await?;
                }
            }
            Action::Insertion(reception) => {
                self.receipt.take();
                let ready = self.work.insertion(reception, store).await?;
                self.phase.after_insertion(ready);
                if !ready {
                    run.stop_when_drained();
                }
            }
            Action::Uncertain => {
                self.phase.busy();
                custody::uncertain(
                    store,
                    self.guard,
                    machine.machine(),
                    "Autolyser poll response outcome uncertain",
                )
                .await?;
            }
            Action::Refused(error) => return Err(error.into()),
            Action::Close => run.stop_when_drained(),
            Action::Deadline(_) => {}
        }
        Ok(Flow::Continue)
    }
}
