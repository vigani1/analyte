//! Runs an Autolyser session: polls for reports, stores each before acknowledging, downloads worklists between polls, and holds next-report (R) polls during last-report (L) recovery.
use crate::intake::ByteReason;
use crate::machine::hooks::Pause;
mod custody;
use super::driver::{self, Context, Effects, Flow};
use crate::intake;
use crate::machine::connection_input::Input;
use crate::machine::custody::{self as receive_custody, ms};
use crate::orders::outcome::AutolyserEnd;
mod error;
use analyte_dialog::autolyser::{Action, Config, Dialog, Event, Recovery, Request};
use analyte_model::ReceiptId;
use analyte_store::Handle;
pub(crate) use error::Error;
use std::{collections::VecDeque, time::Instant};
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
    end_code: AutolyserEnd,
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
        let policy = settings.policy();
        let dialog = Dialog::new(Config {
            poll: ms(policy.poll_ms),
            ack: ms(policy.acknowledgment_ms),
            response: ms(policy.response_ms),
            inter_byte: ms(policy.interbyte_ms),
            storage: ms(policy.storage_ms),
            max_bytes: settings.settings().frame_limit(),
            controls: policy.controls,
            recovery: if policy.recover_last_on_poll {
                Recovery::LastOnPoll
            } else {
                Recovery::ManualLast
            },
        })?;
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
            end_code: AutolyserEnd::TransferError,
        })
    }
}

impl driver::ByteEffects for Host<'_, '_> {
    async fn apply_byte(
        &mut self,
        byte: u8,
        conn: &mut analyte_transport::Connection,
        _stop: &watch::Receiver<bool>,
    ) -> Result<bool, Error> {
        let actions = self.dialog.step(self.start.elapsed(), Event::Byte(byte))?;
        self.apply(actions, conn).await
    }
}

impl Effects for Host<'_, '_> {
    driver::chunk_effects!(Error);
    async fn prepare(&mut self, _host: &mut Context<'_, '_>) -> Result<Flow, Error> {
        self.end_code = AutolyserEnd::InputFailed;
        Ok(Flow::Continue)
    }
    async fn input(&mut self, host: &mut Context<'_, '_>, input: Input) -> Result<Flow, Error> {
        let machine = self.machine;
        let store = self.store;
        let conn = &mut *host.conn;
        let stop = &mut host.stop;
        let ended = matches!(
            input,
            Input::Closed(_) | Input::Failed(_) | Input::RecordingFailed(_)
        );
        self.end_code = match &input {
            Input::Closed(_) | Input::Failed(_) | Input::RecordingFailed(_) => {
                if *stop.borrow() {
                    AutolyserEnd::TransferCancelled
                } else {
                    AutolyserEnd::TransferDisconnected
                }
            }
            Input::Tick => AutolyserEnd::TransferTimeout,
            Input::Opened { .. } | Input::Bytes { .. } => AutolyserEnd::TransferError,
        };
        let events = match input {
            Input::Opened { .. } => {
                self.phase.busy();
                vec![Event::Poll(Request::Next)]
            }
            Input::Bytes { bytes } => {
                if driver::feed(self, bytes, conn, stop).await? {
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
                        if self.apply(actions, conn).await? {
                            return Ok(Flow::Stop);
                        }
                        Vec::new()
                    } else {
                        vec![Event::Poll(Request::Next)]
                    }
                } else {
                    if self.apply(actions, conn).await? {
                        return Ok(Flow::Stop);
                    }
                    Vec::new()
                }
            }
            Input::Closed(_) | Input::Failed(_) | Input::RecordingFailed(_) => {
                vec![Event::Disconnected]
            }
        };
        for event in events {
            if matches!(event, Event::Poll(Request::Next)) {
                self.guard = Some(custody::reserve(store, machine.machine()).await?);
            }
            let actions = self.dialog.step(self.start.elapsed(), event)?;
            if self.apply(actions, conn).await? {
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
        let cleanup = custody::uncertain(
            self.store,
            self.guard,
            self.machine.machine(),
            self.end_code.as_str(),
        )
        .await;
        receive_custody::retain_cleanup(result, cleanup);
        self.work
            .close(self.store, self.end_code.into())
            .await
            .map_err(Into::into)
    }
}

impl<'a, 'm> Host<'a, 'm> {
    async fn store_report(
        &mut self,
        token: analyte_dialog::autolyser::StoreId,
        raw: Vec<u8>,
        actions: &mut VecDeque<Action>,
        machine: &intake::ConnectionEvidence<'_>,
        store: &Handle,
    ) -> Option<Error> {
        let prior_code = self.end_code;
        self.end_code = AutolyserEnd::StorageFailed;
        let id = ReceiptId::new();
        let stored = receive_custody::store_before_ack(
            machine.machine(),
            intake::receive(store, machine, id, raw),
        )
        .await;
        match stored {
            Ok(time) => {
                self.end_code = prior_code;
                self.receipt = Some((id, time));
                match self.dialog.step(self.start.elapsed(), Event::Stored(token)) {
                    Ok(next) => actions.extend(next),
                    Err(error) => return Some(error.into()),
                }
            }
            Err(error) => {
                if matches!(error, receive_custody::StoreWaitError::Timeout(_)) {
                    self.end_code = AutolyserEnd::StorageTimeout;
                }
                match self
                    .dialog
                    .step(self.start.elapsed(), Event::StoreFailed(token))
                {
                    Ok(next) => actions.extend(next),
                    Err(dialog_error) => {
                        return Some(receive_custody::effect_failure(
                            Some(error.into()),
                            dialog_error.into(),
                        ));
                    }
                }
                return Some(error.into());
            }
        }

        None
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

impl Host<'_, '_> {
    async fn apply(
        &mut self,
        actions: Vec<Action>,
        conn: &mut analyte_transport::Connection,
    ) -> Result<bool, Error> {
        let machine = self.machine;
        let store = self.store;
        let policy = self.settings.policy();
        let mut actions: VecDeque<_> = actions.into();
        let mut close = false;
        let mut failure = None;
        while let Some(action) = actions.pop_front() {
            match action {
                Action::Write(bytes) => {
                    self.end_code = AutolyserEnd::WriteFailed;
                    receive_custody::write(conn, &bytes, ms(policy.acknowledgment_ms)).await?;
                    self.end_code = AutolyserEnd::TransferError;
                }
                Action::Store(token, raw) => {
                    if let Some(error) = self
                        .store_report(token, raw, &mut actions, machine, store)
                        .await
                    {
                        failure = Some(error);
                    }
                }
                Action::Report(_) => {
                    self.interpret_report(machine, store).await?;
                }
                Action::Park(error) => {
                    if self.work.is_active() {
                        let code = if matches!(error, analyte_dialog::autolyser::Error::Incomplete)
                            && matches!(
                                self.end_code,
                                AutolyserEnd::TransferTimeout
                                    | AutolyserEnd::TransferDisconnected
                                    | AutolyserEnd::TransferCancelled
                            ) {
                            crate::orders::outcome::Transfer::AutolyserIncomplete(self.end_code)
                        } else {
                            crate::orders::outcome::Transfer::AutolyserParked(error.clone())
                        };
                        self.work.refuse(store, code).await?;
                        close = true;
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
                    if let Err(error) = parked {
                        return Err(receive_custody::effect_failure(failure, error.into()));
                    }
                    if let Err(error) =
                        custody::uncertain(store, self.guard, machine.machine(), &reason).await
                    {
                        return Err(receive_custody::effect_failure(failure, error));
                    }
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
                    close |= !ready;
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
                Action::Close => close = true,
                Action::Deadline(_) => {}
            }
        }
        match failure {
            Some(error) => Err(error),
            None => Ok(close),
        }
    }
}
