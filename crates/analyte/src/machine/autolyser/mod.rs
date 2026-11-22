//! Runs an Autolyser session: polls for reports, stores each before acknowledging, downloads worklists between polls, and holds next-report (R) polls during last-report (L) recovery.
use crate::machine::hooks::Pause;
mod custody;
use super::driver::{Flow, HostInput, Protocol, Session, Trigger};
use crate::intake;
use crate::machine::receive;
use crate::machine::receive::custody::{WriteDeadline, ms};
pub(crate) use custody::POLL_REASON;
mod error;
pub(crate) mod orders;
mod outcome;
pub(crate) mod settings;
use analyte_config::AutolyserStack;
use analyte_dialog::autolyser::{Action, Dialog, Event, Request};
use analyte_model::ReceiptId;
pub(crate) use error::Error;
use std::time::Duration;

// A stored parking reason; `receive::bytes` pins its spelling.
pub(crate) fn refusal_reason(error: &analyte_dialog::autolyser::Error) -> String {
    format!("Autolyser: {error}")
}

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
pub(super) struct Host<'a> {
    work: crate::orders::sending::Work<
        'a,
        analyte_config::AutolyserOrders,
        analyte_autolyser::syntax::Worklist,
    >,
    dialog: Dialog,
    receipt: Option<(ReceiptId, jiff::Timestamp)>,
    phase: Phase,
    guard: Option<analyte_store::PollIntentId>,
}

impl<'a> Host<'a> {
    /// Refuses to start while an earlier session left an R poll unreconciled.
    pub(super) async fn open(
        machine: &intake::ConnectionEvidence<'_>,
        settings: &'a AutolyserStack,
        store: &analyte_store::Handle,
    ) -> Result<Self, Error> {
        custody::ensure_reconciled(store, machine.machine()).await?;
        let dialog = Dialog::new(settings::dialog(settings))?;
        Ok(Self {
            work: crate::orders::sending::Work::new(
                settings.orders().filter(|o| o.send_ahead()),
                machine.machine(),
            ),
            dialog,
            receipt: None,
            phase: Phase::Busy,
            guard: None,
        })
    }
    /// Guards the R cursor before the dialog asks for the next report.
    async fn poll_next(&mut self, s: &mut Session<'_, '_, Self>) -> Result<(), Error> {
        self.guard = Some(custody::reserve(s.store, s.machine.machine()).await?);
        let actions = self.dialog.step(s.elapsed(), Event::Poll(Request::Next))?;
        s.actions.extend(actions);
        Ok(())
    }
    /// Stores a report before the dialog may acknowledge it. A failed store is
    /// kept until the dialog's own refusal has run; it never ends the queue early.
    async fn store_report(
        &mut self,
        s: &mut Session<'_, '_, Self>,
        token: analyte_dialog::autolyser::StoreId,
        raw: Vec<u8>,
    ) -> Result<(), Error> {
        let id = ReceiptId::new();
        let stored = receive::custody::store_before_ack(
            s.machine.machine(),
            intake::receive(s.store, s.machine, id, raw),
        )
        .await;
        let event = match stored {
            Ok(time) => {
                self.receipt = Some((id, time));
                Event::Stored(token)
            }
            Err(error) => {
                s.failures.keep(error.into());
                Event::StoreFailed(token)
            }
        };
        s.actions.extend(self.dialog.step(s.elapsed(), event)?);
        Ok(())
    }
    async fn interpret_report(&mut self, s: &Session<'_, '_, Self>) -> Result<(), Error> {
        let (id, _) = self
            .receipt
            .take()
            .ok_or_else(|| Error::ReportReceiptMissing)?;
        intake::interpret(s.store, s.machine.machine(), id).await?;
        self.phase = Phase::Ready;
        custody::complete(s.store, &mut self.guard, s.machine.machine()).await?;

        Ok(())
    }
}

impl Protocol for Host<'_> {
    type Settings = AutolyserStack;
    type Action = Action;
    type Error = Error;
    fn deadline(&self, _s: &Session<'_, '_, Self>) -> Result<Option<Duration>, Error> {
        Ok(self.dialog.next_deadline())
    }
    /// A test may pause here after machine bytes left the host ready and quiet.
    /// Only bytes leave it ready: an opened connection starts a poll first.
    async fn idle(&mut self, s: &mut Session<'_, '_, Self>) -> Result<Flow, Error> {
        let hooks = &s.machine.machine().hooks;
        if s.trigger() == Trigger::Exchange
            && self.phase == Phase::Ready
            && self.guard.is_none()
            && !self.work.is_active()
            && hooks.enabled(Pause::AutolyserQuiescent)
        {
            hooks
                .pause(Pause::AutolyserQuiescent, &mut s.stop)
                .await
                .map_err(Error::from)?;
        }
        Ok(Flow::Continue)
    }
    async fn input(
        &mut self,
        s: &mut Session<'_, '_, Self>,
        input: HostInput,
    ) -> Result<Flow, Error> {
        match input {
            HostInput::Opened => {
                self.phase.busy();
                self.poll_next(s).await?;
            }
            HostInput::Chunk => {}
            HostInput::Byte(byte) => {
                let actions = self.dialog.step(s.elapsed(), Event::Byte(byte))?;
                s.actions.extend(actions);
            }
            HostInput::Tick => {
                let actions = self.dialog.step(s.elapsed(), Event::Tick)?;
                // Tick alone can initiate L recovery. Never interleave R with it.
                if actions.is_empty() && self.phase == Phase::Ready {
                    self.phase = Phase::Busy;
                    if let Some(worklist) = self.work.poll(s.store, s.machine, s.settings).await? {
                        let actions = self.dialog.step(s.elapsed(), Event::Download(&worklist))?;
                        s.actions.extend(actions);
                    } else {
                        self.poll_next(s).await?;
                    }
                } else {
                    s.actions.extend(actions);
                }
            }
            HostInput::Disconnected => {
                let actions = self.dialog.step(s.elapsed(), Event::Disconnected)?;
                s.actions.extend(actions);
            }
        }
        Ok(Flow::Continue)
    }
    async fn effect(
        &mut self,
        s: &mut Session<'_, '_, Self>,
        action: Action,
    ) -> Result<Flow, Error> {
        let machine = s.machine;
        let store = s.store;
        match action {
            Action::Write(bytes) => {
                let deadline = WriteDeadline::Within(ms(s.settings.policy().acknowledgment_ms));
                receive::custody::write(s.conn, &bytes, deadline).await?;
            }
            Action::Store(token, raw) => {
                self.store_report(s, token, raw).await?;
            }
            Action::Report(_) => {
                self.interpret_report(s).await?;
            }
            Action::Park(error) => {
                if self.work.is_active() {
                    let code = outcome::parked(s.trigger(), &error);
                    self.work.refuse(store, &code).await?;
                    s.stop_when_drained();
                }
                self.phase = Phase::Recovering;
                let reason = refusal_reason(&error);
                let parked = match self.receipt.take() {
                    Some((id, _)) => {
                        intake::failed_reading(store, machine.machine(), id, &reason).await
                    }
                    None => intake::park_bytes(store, machine, Vec::new(), &reason).await,
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
                    s.stop_when_drained();
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
            Action::Close => s.stop_when_drained(),
            Action::Deadline(_) => {}
        }
        Ok(Flow::Continue)
    }
    async fn close(
        &mut self,
        s: &mut Session<'_, '_, Self>,
        result: &mut Result<(), Error>,
    ) -> Result<(), Error> {
        let code = outcome::session_end(s.trigger(), result);
        let cleanup =
            custody::uncertain(s.store, self.guard, s.machine.machine(), code.as_str()).await;
        receive::custody::retain_cleanup(result, cleanup);
        self.work
            .close(s.store, code.as_str())
            .await
            .map_err(Into::into)
    }
}
