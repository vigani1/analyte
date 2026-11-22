//! Receives VIDAS uploads using configured checksum, suffix and timer rules.
use super::driver::{Flow, HostInput, Protocol, Session};
use crate::intake;
use crate::machine::receive::custody::{self, ms};
mod error;
pub(crate) mod orders;
mod outcome;
pub(crate) mod settings;
use analyte_config::VidasStack;
use analyte_dialog::vidas::{Action, Config as DialogConfig, Dialog, Event};
use analyte_framing::vidas::Config;
use analyte_model::ReceiptId;
use analyte_store::Handle;
pub(crate) use error::Error;
use std::time::{Duration, Instant};

// Parking reasons are stored text; `receive::bytes` pins their spellings.
pub(crate) const SUFFIX_REASON: &str = "VIDAS control suffix mismatch";
pub(crate) const INCOMPLETE_REASON: &str =
    "VIDAS transfer incomplete or refused; retained transport journal requires reconciliation";
pub(crate) fn framing_reason(error: &analyte_framing::vidas::Error) -> String {
    format!("VIDAS framing: {error}")
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Opening,
    Idle,
    Transferring,
}

/// Dialog, decoder and order work for one connection.
pub(super) struct Host<'a> {
    work: crate::orders::sending::Work<
        'a,
        analyte_config::VidasOrders,
        analyte_vidas::syntax::Message,
    >,
    dialog: Dialog,
    /// The opening and tick actions wait here for the next idle step, which
    /// polls for a due order download first.
    pending: Vec<Action>,
    phase: Phase,
    config: Config,
    policy: DialogConfig,
}

impl<'a> Host<'a> {
    pub(super) fn open(
        machine: &crate::installation::config::Machine,
        settings: &'a VidasStack,
    ) -> Result<Self, Error> {
        let config = settings::framing(settings)?;
        let policy = settings::dialog(settings);
        let mut dialog = Dialog::upload(config.clone(), policy.clone())?;
        let pending = dialog.start();
        Ok(Self {
            work: crate::orders::sending::Work::new(
                settings.orders().filter(|o| o.send_ahead()),
                machine,
            ),
            dialog,
            pending,
            phase: Phase::Opening,
            config,
            policy,
        })
    }
    /// Replaces the dialog with a download and returns its opening actions.
    fn start_download(
        &mut self,
        s: &mut Session<'_, '_, Self>,
        request: &analyte_vidas::syntax::Message,
    ) -> Result<Vec<Action>, Error> {
        self.dialog = Dialog::download(self.config.clone(), self.policy.clone(), request)?;
        s.restart_clock();
        self.phase = Phase::Transferring;
        Ok(self.dialog.start())
    }
    /// Steps one byte; a framing failure parks the byte and ends the session.
    async fn byte(&mut self, s: &Session<'_, '_, Self>, byte: u8) -> Result<Vec<Action>, Error> {
        match self.dialog.step(s.elapsed(), Event::Byte(byte)) {
            Ok(actions) => Ok(actions),
            Err(analyte_dialog::vidas::Error::Framing(error)) => {
                let suffix = error == analyte_framing::vidas::Error::ControlSuffix;
                let reason = if suffix {
                    SUFFIX_REASON.to_owned()
                } else {
                    framing_reason(&error)
                };
                let parked = intake::park_bytes(s.store, s.machine, vec![byte], &reason).await;
                let cause = if suffix {
                    Error::ControlSuffixMismatch
                } else {
                    error.into()
                };
                // A failed park keeps the framing failure as the session's cause.
                Err(match parked {
                    Ok(()) => cause,
                    Err(effect) => custody::effect_failure(Some(cause), effect.into()),
                })
            }
            Err(error) => Err(error.into()),
        }
    }
    /// Stores an upload before the dialog may acknowledge it. A failed store
    /// is kept until the dialog's own park has run.
    async fn store_packet(
        &mut self,
        s: &mut Session<'_, '_, Self>,
        raw: &[u8],
    ) -> Result<(), Error> {
        let id = ReceiptId::new();
        let operation = store_upload(s.store, s.machine, id, raw);
        match custody::store_before_ack(s.machine.machine(), operation).await {
            Ok(()) => {
                // The dialog expired its own commit deadline before accepting completion.
                s.actions
                    .extend(self.dialog.step(s.elapsed(), Event::Stored)?);
            }
            Err(error) => {
                s.actions
                    .extend(self.dialog.step(s.elapsed(), Event::StoreFailed)?);
                s.failures.keep(error.into());
            }
        }
        Ok(())
    }
}

impl Protocol for Host<'_> {
    type Settings = VidasStack;
    type Action = Action;
    type Error = Error;
    fn deadline(&self, s: &Session<'_, '_, Self>) -> Result<Option<Duration>, Error> {
        let mut next = self
            .dialog
            .next_deadline()
            .ok_or(Error::ActiveDialogHasNoDeadline)?;
        if self.phase == Phase::Idle {
            next = next.min(
                s.elapsed().saturating_add(
                    self.work
                        .next_poll()
                        .saturating_duration_since(Instant::now()),
                ),
            );
        }
        Ok(Some(next))
    }
    async fn idle(&mut self, s: &mut Session<'_, '_, Self>) -> Result<Flow, Error> {
        if self.phase == Phase::Idle
            && let Some(request) = self.work.poll(s.machine, s.store, s.settings).await?
        {
            let started = self.start_download(s, &request)?;
            self.pending.extend(started);
        }
        s.actions.extend(self.pending.drain(..));
        Ok(Flow::Continue)
    }
    async fn input(
        &mut self,
        s: &mut Session<'_, '_, Self>,
        input: HostInput,
    ) -> Result<Flow, Error> {
        match input {
            HostInput::Opened => self.phase = Phase::Idle,
            HostInput::Tick => {
                let actions = self.dialog.step(s.elapsed(), Event::Tick)?;
                self.pending.extend(actions);
            }
            HostInput::Disconnected => {
                let actions = self.dialog.step(s.elapsed(), Event::Disconnected)?;
                s.actions.extend(actions);
            }
            HostInput::Chunk => self.phase = Phase::Transferring,
            // Store runs before a merged ETX/EOT following it is fed.
            HostInput::Byte(byte) => s.actions.extend(self.byte(s, byte).await?),
        }
        Ok(Flow::Continue)
    }
    async fn effect(
        &mut self,
        s: &mut Session<'_, '_, Self>,
        action: Action,
    ) -> Result<Flow, Error> {
        match action {
            Action::Write(bytes) => {
                let budget = ms(s.settings.policy().acknowledgment_ms);
                custody::write(s.conn, &bytes, custody::WriteDeadline::Within(budget)).await?;
            }
            Action::Store(packet) => self.store_packet(s, &packet.raw).await?,
            Action::TimerArmed(_, _) | Action::TimerCancelled(_) => {}
            Action::Park => {
                // Kept before parking, so the park and its deadline stay
                // in the session's cause even when parking itself fails.
                s.failures.keep(Error::TransferUnreconciled {
                    expired: self.dialog.expired_timer(),
                });
                intake::park_bytes(s.store, s.machine, Vec::new(), INCOMPLETE_REASON).await?;
            }
            Action::Close => match self.work.confirmed(s.store).await? {
                orders::Progress::DownloadNext(request) => {
                    let started = self.start_download(s, &request)?;
                    s.actions.extend(started);
                }
                orders::Progress::DownloadFinished => {
                    self.dialog = Dialog::upload(self.config.clone(), self.policy.clone())?;
                    s.restart_clock();
                    s.actions.extend(self.dialog.start());
                    self.phase = Phase::Idle;
                }
                orders::Progress::UploadFinished => s.stop_when_drained(),
            },
        }
        Ok(Flow::Continue)
    }
    async fn close(
        &mut self,
        s: &mut Session<'_, '_, Self>,
        result: &mut Result<(), Error>,
    ) -> Result<(), Error> {
        self.work
            .close(s.store, outcome::session_end(s.trigger(), result).as_str())
            .await
            .map_err(Into::into)
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
