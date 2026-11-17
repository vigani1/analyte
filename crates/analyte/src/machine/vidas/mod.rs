//! Receives VIDAS uploads using configured checksum, suffix and timer rules.
use super::connection_input::Input;
use super::custody::{self, ms};
use super::driver::{self, Context, Effects, Flow};
use crate::intake;
use crate::intake::ByteReason;
use crate::orders::outcome::VidasEnd;
mod error;
use analyte_dialog::vidas::{Action, Config as DialogConfig, Dialog, Event, Timer};
use analyte_framing::vidas::Config;
use analyte_model::ReceiptId;
use analyte_store::Handle;
pub(crate) use error::Error;
use std::{collections::VecDeque, time::Instant};
use tokio::sync::watch;

/// Dialog, decoder and timers cannot outlive their owning transport connection.
struct ConnectionState {
    work: crate::orders::vidas::Work,
    dialog: Dialog,
    start: Instant,
    actions: VecDeque<Action>,
    idle: bool,
    opened: bool,
    end_code: VidasEnd,
    config: Config,
    policy: DialogConfig,
}

pub(super) async fn session(
    machine: &intake::ConnectionEvidence<'_>,
    settings: &analyte_config::VidasStack,
    store: &Handle,
    conn: &mut analyte_transport::Connection,
    stop: watch::Receiver<bool>,
) -> Result<(), Error> {
    driver::run(
        || ConnectionState::open(machine, settings),
        Context {
            machine,
            settings,
            store,
            conn,
            stop,
        },
    )
    .await
}

impl ConnectionState {
    fn open(
        machine: &intake::ConnectionEvidence<'_>,
        settings: &analyte_config::VidasStack,
    ) -> Result<Self, Error> {
        let installed = &settings.policy();
        let config = crate::reading::vidas_config(&machine.machine().snapshot)?;
        let policy = DialogConfig {
            ack: ms(installed.acknowledgment_ms),
            inter_byte: ms(installed.interbyte_ms),
            session: ms(installed.session_ms),
            commit: ms(installed.commit_ms),
            retries: installed.retries,
            backoff: ms(installed.backoff_ms),
        };
        let mut dialog = Dialog::upload(config.clone(), policy.clone())?;
        let actions = dialog.start().into();
        Ok(Self {
            work: crate::orders::vidas::Work::new(),
            dialog,
            start: Instant::now(),
            actions,
            idle: true,
            opened: false,
            end_code: VidasEnd::TransferError,
            config,
            policy,
        })
    }
    fn start_download(&mut self, request: &analyte_vidas::syntax::Message) -> Result<(), Error> {
        self.dialog = Dialog::download(self.config.clone(), self.policy.clone(), request)?;
        self.start = Instant::now();
        self.idle = false;
        self.end_code = VidasEnd::TransferError;
        self.actions.extend(self.dialog.start());
        Ok(())
    }
    async fn receive_bytes(
        &mut self,
        bytes: Vec<u8>,
        machine: &intake::ConnectionEvidence<'_>,
        store: &Handle,
        conn: &mut analyte_transport::Connection,
        installed: &analyte_config::Vidas,
    ) -> Result<bool, Error> {
        self.idle = false;
        for byte in bytes {
            match self.dialog.step(self.start.elapsed(), Event::Byte(byte)) {
                Ok(actions) => self.actions.extend(actions),
                Err(analyte_dialog::vidas::Error::Framing(error)) => {
                    let reason = if error == analyte_framing::vidas::Error::ControlSuffix {
                        self.end_code = VidasEnd::ControlSuffixMismatch;
                        ByteReason::VidasSuffix
                    } else {
                        self.end_code = VidasEnd::FramingError;
                        ByteReason::VidasFrame(&error)
                    };
                    intake::park_bytes(store, machine, vec![byte], reason).await?;
                    return Err(if error == analyte_framing::vidas::Error::ControlSuffix {
                        Error::ControlSuffixMismatch
                    } else {
                        error.into()
                    });
                }
                Err(error) => return Err(error.into()),
            }
            // Process Store before consuming a merged ETX/EOT following it.
            if process(self, machine, store, conn, installed).await? {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

impl Effects for ConnectionState {
    type Settings = analyte_config::VidasStack;
    type Error = Error;
    fn custody(&self) -> driver::Custody {
        driver::Custody::Chunks
    }
    fn wait(&self) -> Result<Option<std::time::Duration>, Error> {
        let mut next = self
            .dialog
            .next_deadline()
            .ok_or(Error::ActiveDialogHasNoDeadline)?;
        if self.idle && self.opened {
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
    async fn prepare(&mut self, host: &mut Context<'_, '_, Self::Settings>) -> Result<Flow, Error> {
        if self.idle
            && self.opened
            && let Some(request) = self.work.poll(host.machine, host.store).await?
        {
            self.start_download(&request)?;
        }
        if process(
            self,
            host.machine,
            host.store,
            host.conn,
            host.settings.policy(),
        )
        .await?
        {
            return Ok(Flow::Stop);
        }
        self.end_code = VidasEnd::InputFailed;
        Ok(Flow::Continue)
    }
    async fn input(
        &mut self,
        host: &mut Context<'_, '_, Self::Settings>,
        input: Input,
    ) -> Result<Flow, Error> {
        let Context {
            machine,
            store,
            conn,
            stop,
            settings,
        } = host;
        let installed = settings.policy();
        self.end_code = VidasEnd::TransferError;
        match input {
            Input::Opened { .. } => self.opened = true,
            Input::Tick => {
                self.actions
                    .extend(self.dialog.step(self.start.elapsed(), Event::Tick)?);
            }
            Input::Closed(_) | Input::Failed(_) | Input::RecordingFailed(_) => {
                self.end_code = if *stop.borrow() {
                    VidasEnd::TransferCancelled
                } else {
                    VidasEnd::TransferDisconnected
                };
                self.actions.extend(
                    self.dialog
                        .step(self.start.elapsed(), Event::Disconnected)?,
                );
                process(self, machine, store, conn, installed).await?;
                return Ok(Flow::Stop);
            }
            Input::Bytes { bytes } => {
                if self
                    .receive_bytes(bytes, machine, store, conn, installed)
                    .await?
                {
                    return Ok(Flow::Stop);
                }
            }
        }

        Ok(Flow::Continue)
    }
    async fn close(
        &mut self,
        host: &mut Context<'_, '_, Self::Settings>,
        _result: &mut Result<(), Error>,
    ) -> Result<(), Error> {
        self.work.close(host.store, self.end_code.into()).await
    }
}

async fn process(
    state: &mut ConnectionState,
    machine: &intake::ConnectionEvidence<'_>,
    store: &Handle,
    conn: &mut analyte_transport::Connection,
    installed: &analyte_config::Vidas,
) -> Result<bool, Error> {
    let mut failure = None;
    let mut complete = false;
    while let Some(action) = state.actions.pop_front() {
        match action {
            Action::Send(bytes) => {
                state.end_code = VidasEnd::WriteFailed;
                custody::write(conn, &bytes, ms(installed.acknowledgment_ms)).await?;
                state.end_code = VidasEnd::TransferError;
            }
            Action::Store(packet) => {
                let id = ReceiptId::new();
                let operation = store_upload(store, machine, id, &packet.raw);
                match custody::store_before_ack(machine.machine(), operation).await {
                    Ok(()) => {
                        // The dialog expired its own commit deadline before accepting completion.
                        state
                            .actions
                            .extend(state.dialog.step(state.start.elapsed(), Event::Stored)?);
                    }
                    Err(error) => {
                        state.actions.extend(
                            state
                                .dialog
                                .step(state.start.elapsed(), Event::StorageFailed)?,
                        );
                        failure = Some(error.into());
                    }
                }
            }
            Action::Arm(_, _) | Action::Cancel(_) => {}
            Action::Park => {
                if state.end_code == VidasEnd::TransferError {
                    state.end_code = match state.dialog.expired_timer() {
                        Some(Timer::Acknowledgment) => VidasEnd::AcknowledgmentTimeout,
                        Some(Timer::Interbyte) => VidasEnd::InterbyteTimeout,
                        Some(Timer::Session) => VidasEnd::SessionTimeout,
                        Some(Timer::Commit) => VidasEnd::CommitTimeout,
                        None => VidasEnd::TransferRefusedOrUnexpected,
                    };
                }
                let parked =
                    intake::park_bytes(store, machine, Vec::new(), ByteReason::VidasIncomplete)
                        .await;
                if let Err(error) = parked {
                    return Err(custody::effect_failure(failure, error.into()));
                }
                if failure.is_none() {
                    failure = Some(Error::TransferParkedReconcileBeforeReconnect);
                }
            }
            Action::Complete => match state.work.confirmed(store).await? {
                crate::orders::vidas::Progress::DownloadNext(request) => {
                    state.start_download(&request)?;
                }
                crate::orders::vidas::Progress::DownloadFinished => {
                    state.dialog = Dialog::upload(state.config.clone(), state.policy.clone())?;
                    state.start = Instant::now();
                    state.actions.extend(state.dialog.start());
                    state.idle = true;
                }
                crate::orders::vidas::Progress::UploadFinished => complete = true,
            },
        }
    }
    match failure {
        Some(error) => Err(error),
        None => Ok(complete),
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
