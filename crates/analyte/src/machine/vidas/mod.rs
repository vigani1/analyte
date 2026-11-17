//! Receives VIDAS uploads using configured checksum, suffix and timer rules.
use super::connection_input::Input;
use super::driver::{self, Context, Effects, Failures, Flow, Trigger};
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
    work: crate::orders::sending::Work<analyte_vidas::syntax::Message>,
    dialog: Dialog,
    start: Instant,
    actions: VecDeque<Action>,
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
        let actions = dialog.start().into();
        Ok(Self {
            machine,
            settings,
            store,
            work: crate::orders::sending::Work::default(),
            dialog,
            start: Instant::now(),
            actions,
            phase: Phase::Opening,
            trigger: Trigger::Exchange,
            config,
            policy,
        })
    }
    fn start_download(&mut self, request: &analyte_vidas::syntax::Message) -> Result<(), Error> {
        self.dialog = Dialog::download(self.config.clone(), self.policy.clone(), request)?;
        self.start = Instant::now();
        self.phase = Phase::Transferring;
        self.actions.extend(self.dialog.start());
        Ok(())
    }
    async fn receive_bytes(
        &mut self,
        bytes: Vec<u8>,
        conn: &mut analyte_transport::Connection,
    ) -> Result<Flow, Error> {
        let machine = self.machine;
        let store = self.store;
        self.phase = Phase::Transferring;
        for byte in bytes {
            match self.dialog.step(self.start.elapsed(), Event::Byte(byte)) {
                Ok(actions) => self.actions.extend(actions),
                Err(analyte_dialog::vidas::Error::Framing(error)) => {
                    let suffix = error == analyte_framing::vidas::Error::ControlSuffix;
                    let reason = if suffix {
                        ByteReason::VidasSuffix
                    } else {
                        ByteReason::VidasFrame(&error)
                    };
                    let parked = intake::park_bytes(store, machine, vec![byte], reason).await;
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
            if self.apply(Vec::new(), conn).await? == Flow::Stop {
                return Ok(Flow::Stop);
            }
        }
        Ok(Flow::Continue)
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
            self.start_download(&request)?;
        }
        if self.apply(Vec::new(), host.conn).await? == Flow::Stop {
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
                self.actions
                    .extend(self.dialog.step(self.start.elapsed(), Event::Tick)?);
            }
            Input::Closed(_) | Input::Failed(_) => {
                self.actions.extend(
                    self.dialog
                        .step(self.start.elapsed(), Event::Disconnected)?,
                );
                self.apply(Vec::new(), conn).await?;
                return Ok(Flow::Stop);
            }
            Input::Bytes { bytes } => {
                if self.receive_bytes(bytes, conn).await? == Flow::Stop {
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
            .close(self.store, outcome::end_code(self.trigger, result).into())
            .await
            .map_err(Into::into)
    }
}

impl Host<'_, '_> {
    async fn apply(
        &mut self,
        actions: Vec<Action>,
        conn: &mut analyte_transport::Connection,
    ) -> Result<Flow, Error> {
        let machine = self.machine;
        let store = self.store;
        let installed = self.settings.policy();
        self.actions.extend(actions);
        let mut failures = Failures::new();
        let result = async {
            let mut complete = false;
            while let Some(action) = self.actions.pop_front() {
                match action {
                    Action::Write(bytes) => {
                        custody::write(conn, &bytes, ms(installed.acknowledgment_ms)).await?;
                    }
                    Action::Store(packet) => {
                        let id = ReceiptId::new();
                        let operation = store_upload(store, machine, id, &packet.raw);
                        match custody::store_before_ack(machine.machine(), operation).await {
                            Ok(()) => {
                                // The dialog expired its own commit deadline before accepting completion.
                                self.actions
                                    .extend(self.dialog.step(self.start.elapsed(), Event::Stored)?);
                            }
                            Err(error) => {
                                self.actions.extend(
                                    self.dialog.step(self.start.elapsed(), Event::StoreFailed)?,
                                );
                                failures.keep(error.into());
                            }
                        }
                    }
                    Action::TimerArmed(_, _) | Action::TimerCancelled(_) => {}
                    Action::Park => {
                        // Kept before parking, so the park and its deadline stay
                        // in the session's cause even when parking itself fails.
                        failures.keep(Error::TransferUnreconciled {
                            expired: self.dialog.expired_timer(),
                        });
                        let parked = intake::park_bytes(
                            store,
                            machine,
                            Vec::new(),
                            ByteReason::VidasIncomplete,
                        )
                        .await;
                        parked?;
                    }
                    Action::Close => match self.work.confirmed(store).await? {
                        crate::orders::vidas::Progress::DownloadNext(request) => {
                            self.start_download(&request)?;
                        }
                        crate::orders::vidas::Progress::DownloadFinished => {
                            self.dialog = Dialog::upload(self.config.clone(), self.policy.clone())?;
                            self.start = Instant::now();
                            self.actions.extend(self.dialog.start());
                            self.phase = Phase::Idle;
                        }
                        crate::orders::vidas::Progress::UploadFinished => complete = true,
                    },
                }
            }
            Ok(if complete { Flow::Stop } else { Flow::Continue })
        }
        .await;
        failures.finish(result)
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
