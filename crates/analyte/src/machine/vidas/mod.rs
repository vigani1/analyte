//! Receives VIDAS uploads using configured checksum, suffix and timer rules.
use super::connection_input::Input;
use super::driver::{self, Context, Effects, Flow, Run, Trigger};
use crate::intake;
use crate::intake::ByteReason;
use crate::machine::receive::custody::{self, ms};
mod error;
mod outcome;
pub(crate) mod settings;
use analyte_dialog::vidas::{Action, Config as DialogConfig, Dialog, Event};
use analyte_framing::vidas::Config;
use analyte_model::ReceiptId;
use analyte_store::Handle;
pub(crate) use error::Error;
use std::{collections::VecDeque, time::Instant};
use tokio::sync::watch;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Opening,
    Idle,
    Transferring,
}

/// Dialog, decoder and timers cannot outlive their owning transport connection.
pub(super) struct Host<'a, 'm> {
    machine: &'a intake::ConnectionEvidence<'m>,
    settings: &'a analyte_config::VidasStack,
    store: &'a Handle,
    work: crate::orders::sending::Work<
        'a,
        analyte_config::VidasOrders,
        analyte_vidas::syntax::Message,
    >,
    dialog: Dialog,
    start: Instant,
    /// Actions produced outside a drain wait here for the next one.
    pending: VecDeque<Action>,
    phase: Phase,
    trigger: Trigger,
    config: Config,
    policy: DialogConfig,
}

impl<'a, 'm> Host<'a, 'm> {
    pub(super) fn open(
        machine: &'a intake::ConnectionEvidence<'m>,
        settings: &'a analyte_config::VidasStack,
        store: &'a Handle,
    ) -> Result<Self, Error> {
        let config = settings::framing(settings)?;
        let policy = settings::dialog(settings);
        let mut dialog = Dialog::upload(config.clone(), policy.clone())?;
        let pending = dialog.start().into();
        Ok(Self {
            machine,
            settings,
            store,
            work: crate::orders::sending::Work::new(settings.orders().filter(|o| o.send_ahead())),
            dialog,
            start: Instant::now(),
            pending,
            phase: Phase::Opening,
            trigger: Trigger::Exchange,
            config,
            policy,
        })
    }
    /// Replaces the dialog with a download and returns its opening actions.
    fn start_download(
        &mut self,
        request: &analyte_vidas::syntax::Message,
    ) -> Result<Vec<Action>, Error> {
        self.dialog = Dialog::download(self.config.clone(), self.policy.clone(), request)?;
        self.start = Instant::now();
        self.phase = Phase::Transferring;
        Ok(self.dialog.start())
    }
}

impl driver::ByteEffects for Host<'_, '_> {
    async fn apply_byte(
        &mut self,
        byte: u8,
        conn: &mut analyte_transport::Connection,
        stop: &mut watch::Receiver<bool>,
    ) -> Result<Flow, Error> {
        match self.dialog.step(self.start.elapsed(), Event::Byte(byte)) {
            Ok(actions) => self.pending.extend(actions),
            Err(analyte_dialog::vidas::Error::Framing(error)) => {
                let suffix = error == analyte_framing::vidas::Error::ControlSuffix;
                let reason = if suffix {
                    ByteReason::VidasSuffix
                } else {
                    ByteReason::VidasFrame(&error)
                };
                let parked = intake::park_bytes(self.store, self.machine, vec![byte], reason).await;
                let cause = if suffix {
                    Error::ControlSuffixMismatch
                } else {
                    error.into()
                };
                // A failed park keeps the framing failure as the session's cause.
                return Err(match parked {
                    Ok(()) => cause,
                    Err(effect) => custody::effect_failure(Some(cause), effect.into()),
                });
            }
            Err(error) => return Err(error.into()),
        }
        // Process Store before consuming a merged ETX/EOT following it.
        self.drain_pending(conn, stop).await
    }
}

impl Effects for Host<'_, '_> {
    type Error = Error;
    const CUSTODY: driver::Custody = driver::Custody::Chunks;
    fn wait(&self) -> Result<Option<std::time::Duration>, Error> {
        let mut next = self
            .dialog
            .next_deadline()
            .ok_or(Error::ActiveDialogHasNoDeadline)?;
        if self.phase == Phase::Idle {
            next = next.min(
                self.start.elapsed().saturating_add(
                    self.work
                        .next_poll()
                        .saturating_duration_since(Instant::now()),
                ),
            );
        }
        Ok(Some(next.saturating_sub(self.start.elapsed())))
    }
    async fn prepare(&mut self, host: &mut Context<'_, '_>) -> Result<Flow, Error> {
        if self.phase == Phase::Idle
            && let Some(request) = self
                .work
                .poll(self.machine, self.store, self.settings)
                .await?
        {
            let started = self.start_download(&request)?;
            self.pending.extend(started);
        }
        if self.drain_pending(host.conn, &mut host.stop).await? == Flow::Stop {
            return Ok(Flow::Stop);
        }
        Ok(Flow::Continue)
    }
    async fn input(&mut self, host: &mut Context<'_, '_>, input: Input) -> Result<Flow, Error> {
        let conn = &mut *host.conn;
        self.trigger = Trigger::of(&input, &host.stop);
        match input {
            Input::Opened { .. } => self.phase = Phase::Idle,
            Input::Tick => {
                self.pending
                    .extend(self.dialog.step(self.start.elapsed(), Event::Tick)?);
            }
            Input::Closed(_) | Input::Failed(_) => {
                self.pending.extend(
                    self.dialog
                        .step(self.start.elapsed(), Event::Disconnected)?,
                );
                self.drain_pending(conn, &mut host.stop).await?;
                return Ok(Flow::Stop);
            }
            Input::Bytes { bytes } => {
                self.phase = Phase::Transferring;
                if driver::feed(self, bytes, conn, &mut host.stop).await? == Flow::Stop {
                    return Ok(Flow::Stop);
                }
            }
        }

        Ok(Flow::Continue)
    }
    async fn close(
        &mut self,
        _host: &mut Context<'_, '_>,
        result: &mut Result<(), Error>,
    ) -> Result<(), Error> {
        self.work
            .close(
                self.store,
                outcome::session_end(self.trigger, result).as_str(),
            )
            .await
            .map_err(Into::into)
    }
}

impl Host<'_, '_> {
    /// Runs the actions produced outside a drain (start, tick, disconnect,
    /// byte) before the session waits again.
    async fn drain_pending(
        &mut self,
        conn: &mut analyte_transport::Connection,
        stop: &mut watch::Receiver<bool>,
    ) -> Result<Flow, Error> {
        let pending = std::mem::take(&mut self.pending);
        driver::drain(self, pending, conn, stop).await
    }
    /// Stores an upload before the dialog may acknowledge it. A failed store
    /// is kept until the dialog's own park has run.
    async fn store_packet(
        &mut self,
        raw: &[u8],
        run: &mut Run<'_, Action, Error>,
    ) -> Result<(), Error> {
        let id = ReceiptId::new();
        let operation = store_upload(self.store, self.machine, id, raw);
        match custody::store_before_ack(self.machine.machine(), operation).await {
            Ok(()) => {
                // The dialog expired its own commit deadline before accepting completion.
                run.actions
                    .extend(self.dialog.step(self.start.elapsed(), Event::Stored)?);
            }
            Err(error) => {
                run.actions
                    .extend(self.dialog.step(self.start.elapsed(), Event::StoreFailed)?);
                run.failures.keep(error.into());
            }
        }
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
        match action {
            Action::Write(bytes) => {
                let budget = ms(self.settings.policy().acknowledgment_ms);
                custody::write(run.conn, &bytes, custody::WriteDeadline::Within(budget)).await?;
            }
            Action::Store(packet) => self.store_packet(&packet.raw, run).await?,
            Action::TimerArmed(_, _) | Action::TimerCancelled(_) => {}
            Action::Park => {
                // Kept before parking, so the park and its deadline stay
                // in the session's cause even when parking itself fails.
                run.failures.keep(Error::TransferUnreconciled {
                    expired: self.dialog.expired_timer(),
                });
                let reason = ByteReason::VidasIncomplete;
                intake::park_bytes(self.store, self.machine, Vec::new(), reason).await?;
            }
            Action::Close => match self.work.confirmed(self.store).await? {
                crate::orders::vidas::Progress::DownloadNext(request) => {
                    run.actions.extend(self.start_download(&request)?);
                }
                crate::orders::vidas::Progress::DownloadFinished => {
                    self.dialog = Dialog::upload(self.config.clone(), self.policy.clone())?;
                    self.start = Instant::now();
                    run.actions.extend(self.dialog.start());
                    self.phase = Phase::Idle;
                }
                crate::orders::vidas::Progress::UploadFinished => run.stop_when_drained(),
            },
        }
        Ok(Flow::Continue)
    }
}

async fn store_upload(
    store: &Handle,
    machine: &intake::ConnectionEvidence<'_>,
    id: ReceiptId,
    raw: &[u8],
) -> Result<(), intake::Error> {
    intake::receive(store, machine, id, raw.to_vec()).await?;
    intake::interpret(store, machine.machine(), id).await?;
    Ok(())
}
