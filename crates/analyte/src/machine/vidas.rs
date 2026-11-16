//! Receives VIDAS uploads using configured checksum, suffix and timer rules.
use super::connection_input::Input;
use super::custody::{self, ms};
use super::transfer_end::VidasEnd;
use crate::{Error, intake, invalid};
use analyte_dialog::vidas::{Action, Dialog, Event, Policy, Timer};
use analyte_framing::vidas::{Config, Decoder};
use analyte_model::ReceiptId;
use analyte_store::Handle;
use analyte_vidas::syntax::Message;
use std::{collections::VecDeque, time::Instant};
use tokio::sync::watch;

/// Dialog, decoder and timers cannot outlive their owning transport connection.
struct ConnectionState {
    dialog: Dialog,
    decoder: Option<Decoder>,
    suffix: Option<(u8, usize)>,
    start: Instant,
    actions: VecDeque<Action>,
    active: Option<analyte_store::MachineOrder>,
    requests: VecDeque<Message>,
    idle: bool,
    opened: bool,
    end_code: VidasEnd,
    cursor: String,
    next_poll: Instant,
    config: Config,
    policy: Policy,
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
        let config = analyte_mapping::read::vidas_config(&machine.snapshot)?;
        let policy = Policy {
            acknowledgment: ms(installed.acknowledgment_ms),
            interbyte: ms(installed.interbyte_ms),
            session: ms(installed.session_ms),
            commit: ms(installed.commit_ms),
            retries: installed.retries,
            backoff: ms(installed.backoff_ms),
        };
        let mut dialog = Dialog::upload(config.clone(), policy.clone())?;
        let actions = dialog.start().into();
        Ok(Self {
            dialog,
            decoder: None,
            suffix: None,
            start: Instant::now(),
            actions,
            active: None,
            requests: VecDeque::new(),
            idle: true,
            opened: false,
            end_code: VidasEnd::TransferError,
            cursor: String::new(),
            next_poll: Instant::now(),
            config,
            policy,
        })
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
            self.poll_orders(machine, store).await?;
            if process(self, machine, store, conn, installed).await? {
                return Ok(());
            }
            let mut next = self
                .dialog
                .next_deadline()
                .ok_or_else(|| invalid("VIDAS active dialog has no deadline"))?;
            if self.idle && self.opened {
                next = next.min(
                    self.start
                        .elapsed()
                        .saturating_add(self.next_poll.saturating_duration_since(Instant::now())),
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
            self.actions
                .extend(self.dialog.step(self.start.elapsed(), Event::Activity)?);
            if process(self, machine, store, conn, installed).await? {
                return Ok(true);
            }
            let event = if let Some(decoder) = &mut self.decoder {
                match Decoder::push(decoder, byte) {
                    Ok(Some(packet)) => Some(Event::Packet(packet)),
                    Ok(None) => None,
                    Err(error) => {
                        self.end_code = VidasEnd::FramingError;
                        intake::park_bytes(
                            store,
                            machine,
                            vec![byte],
                            &format!("VIDAS framing: {error}"),
                        )
                        .await?;
                        return Err(error.into());
                    }
                }
            } else if let Some((control, index)) = self.suffix {
                if installed.suffix.get(index) != Some(&byte) {
                    self.end_code = VidasEnd::ControlSuffixMismatch;
                    intake::park_bytes(store, machine, vec![byte], "VIDAS control suffix mismatch")
                        .await?;
                    return Err(invalid("VIDAS control suffix mismatch"));
                }
                if index.saturating_add(1) == installed.suffix.len() {
                    self.suffix = None;
                    Some(Event::Control(control))
                } else {
                    self.suffix = Some((control, index.saturating_add(1)));
                    None
                }
            } else if byte == 2 {
                let mut next = Decoder::new(self.config.clone());
                next.push(byte)?;
                self.decoder = Some(next);
                None
            } else if installed.suffix.is_empty() {
                Some(Event::Control(byte))
            } else {
                self.suffix = Some((byte, 0usize));
                None
            };
            if let Some(event) = event {
                if matches!(event, Event::Packet(_)) {
                    self.decoder = None;
                }
                self.actions
                    .extend(self.dialog.step(self.start.elapsed(), event)?);
                // Process Store before consuming a merged ETX/EOT following it.
                if process(self, machine, store, conn, installed).await? {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
    async fn poll_orders(
        &mut self,
        machine: &intake::Connection<'_>,
        store: &Handle,
    ) -> Result<(), Error> {
        if self.idle && self.opened && Instant::now() >= self.next_poll {
            self.next_poll = Instant::now()
                .checked_add(ms(1000))
                .ok_or_else(|| invalid("VIDAS order poll deadline overflow"))?;
            if let Some(reserved) =
                crate::orders::sending::next(store, machine, &mut self.cursor).await?
            {
                self.active = Some(reserved.order);
                let analyte_mapping::MachineOrder::Vidas(requests) = reserved.payload else {
                    return Err(invalid("VIDAS received another stack's order"));
                };
                self.requests = requests.into();
                start_download(self)?;
            }
        }
        Ok(())
    }
    async fn close(&self, store: &Handle, mut result: Result<(), Error>) -> Result<(), Error> {
        if let Some(order) = self.active.as_ref() {
            let cleanup =
                crate::orders::sending::uncertain(store, order, self.end_code.as_str()).await;
            custody::retain_cleanup(&mut result, cleanup.map_err(Into::into));
        }
        result
    }
}

fn start_download(state: &mut ConnectionState) -> Result<(), Error> {
    let request = state
        .requests
        .pop_front()
        .ok_or_else(|| invalid("VIDAS mapped request missing"))?;
    state.dialog = Dialog::download(state.config.clone(), state.policy.clone(), &request)?;
    state.start = Instant::now();
    state.idle = false;
    state.end_code = VidasEnd::TransferError;
    state.actions.extend(state.dialog.start());
    Ok(())
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
                match custody::store_within(custody::budget(machine), operation).await {
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
                let parked = intake::park_bytes(store, machine, Vec::new(), "VIDAS transfer incomplete or refused; retained transport journal requires reconciliation").await;
                if let Err(error) = parked {
                    return Err(custody::effect_failure(failure, error.into()));
                }
                if failure.is_none() {
                    failure = Some(invalid("VIDAS transfer parked; reconcile before reconnect"));
                }
            }
            Action::Complete => {
                if let Some(order) = state.active.as_ref() {
                    if !state.requests.is_empty() {
                        start_download(state)?;
                    } else {
                        crate::orders::sending::complete(
                            store,
                            order,
                            analyte_model::OrderState::Sent,
                            VidasEnd::TransferConfirmed.as_str(),
                        )
                        .await?;
                        state.active = None;
                        state.dialog = Dialog::upload(state.config.clone(), state.policy.clone())?;
                        state.start = Instant::now();
                        state.actions.extend(state.dialog.start());
                        state.idle = true;
                    }
                } else {
                    complete = true;
                }
            }
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
    let time = intake::receive(store, machine, id, raw.to_vec()).await?;
    let reading = super::interpretation::map(machine, id, time, raw);
    match reading {
        Ok(reading) => {
            intake::vendor_reading(store, machine, id, reading).await?;
        }
        Err(error) => {
            intake::failed_reading(
                store,
                machine,
                id,
                &format!("VIDAS interpretation: {error}"),
            )
            .await?;
        }
    }
    Ok::<_, intake::Error>(())
}
