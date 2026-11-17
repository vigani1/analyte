//! Receives VIDAS uploads using configured checksum, suffix and timer rules.
use crate::intake::ByteReason;
pub(crate) mod transfer_end;
use self::transfer_end::VidasEnd;
use super::connection_input::Input;
use super::custody::{self, ms};
use crate::intake;
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
    machine: &intake::Connection<'_>,
    settings: &analyte_config::VidasStack,
    store: &Handle,
    conn: &mut analyte_transport::Connection,
    mut stop: watch::Receiver<bool>,
) -> Result<(), Error> {
    let mut state = ConnectionState::open(machine, settings)?;
    // Cancellation after reservation leaves Sending for restart recovery. Every
    // observed non-normal end is committed Uncertain before this owner returns.
    let result = state
        .receive_loop(machine, store, conn, &mut stop, settings.policy())
        .await;
    state.close(store, result).await
}

impl ConnectionState {
    fn open(
        machine: &intake::Connection<'_>,
        settings: &analyte_config::VidasStack,
    ) -> Result<Self, Error> {
        let installed = &settings.policy();
        let config = crate::reading::vidas_config(&machine.snapshot)?;
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
    async fn receive_loop(
        &mut self,
        machine: &intake::Connection<'_>,
        store: &Handle,
        conn: &mut analyte_transport::Connection,
        stop: &mut watch::Receiver<bool>,
        installed: &analyte_config::Vidas,
    ) -> Result<(), Error> {
        loop {
            if self.idle
                && self.opened
                && let Some(request) = self.work.poll(machine, store).await?
            {
                self.start_download(&request)?;
            }
            if process(self, machine, store, conn, installed).await? {
                return Ok(());
            }
            let mut next = self
                .dialog
                .next_deadline()
                .ok_or_else(|| Error::ActiveDialogHasNoDeadline)?;
            if self.idle && self.opened {
                next = next.min(
                    self.start.elapsed().saturating_add(
                        self.work
                            .next_poll()
                            .saturating_duration_since(Instant::now()),
                    ),
                );
            }
            self.end_code = VidasEnd::InputFailed;
            let input = super::connection_input::journaled(
                machine,
                store,
                conn,
                stop,
                Some(next.saturating_sub(self.start.elapsed())),
            )
            .await?;
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
                    return Ok(());
                }
                Input::Bytes { bytes } => {
                    if self
                        .receive_bytes(bytes, machine, store, conn, installed)
                        .await?
                    {
                        return Ok(());
                    }
                }
            }
        }
    }
    async fn receive_bytes(
        &mut self,
        bytes: Vec<u8>,
        machine: &intake::Connection<'_>,
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

    async fn close(&self, store: &Handle, mut result: Result<(), Error>) -> Result<(), Error> {
        let cleanup = self.work.close(store, self.end_code.as_str()).await;
        custody::retain_cleanup(&mut result, cleanup);
        result
    }
}

async fn process(
    state: &mut ConnectionState,
    machine: &intake::Connection<'_>,
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
                match custody::store_before_ack(machine, operation).await {
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
    machine: &intake::Connection<'_>,
    id: ReceiptId,
    raw: &[u8],
) -> Result<(), intake::Error> {
    intake::receive(store, machine, id, raw.to_vec()).await?;
    intake::interpret(store, machine, id).await?;
    Ok(())
}
