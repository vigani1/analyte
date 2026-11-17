//! Runs an Autolyser session: polls for reports, stores each before acknowledging, downloads worklists between polls, and holds next-report (R) polls during last-report (L) recovery.
use crate::intake::ByteReason;
mod custody;
mod orders;
mod transfer_end;
use self::transfer_end::AutolyserEnd;
use crate::intake;
use crate::machine::connection_input::Input;
use crate::machine::custody::{self as receive_custody, ms};
mod error;
use crate::orders::outcome::Code;
use analyte_dialog::autolyser::{Action, Config, Dialog, Event, Recovery, Request};
use analyte_model::ReceiptId;
use analyte_store::Handle;
pub(crate) use error::Error;
use std::{collections::VecDeque, time::Instant};
use tokio::sync::watch;

/// Mutable conversation state has one owner for the entire connection.
struct ConnectionState {
    dialog: Dialog,
    start: Instant,
    receipt: Option<(ReceiptId, jiff::Timestamp)>,
    ready: bool,
    recovering: bool,
    guard: Option<analyte_store::PollIntentId>,
    active: Option<analyte_store::MachineOrder>,
    cursor: String,
    end_code: AutolyserEnd,
}

pub(super) async fn session(
    machine: &intake::Connection<'_>,
    settings: &analyte_config::AutolyserStack,
    store: &Handle,
    conn: &mut analyte_transport::Connection,
    mut stop: watch::Receiver<bool>,
) -> Result<(), Error> {
    custody::ensure_reconciled(store, machine).await?;
    let mut state = ConnectionState::open(settings)?;
    // Dropping this owner after reservation leaves Sending for shared recovery.
    // Observed failures are finalized Uncertain before returning, without replay.
    let result = state
        .receive_loop(machine, store, conn, &mut stop, settings.policy())
        .await;
    state.close(machine, store, result).await
}

impl ConnectionState {
    fn open(settings: &analyte_config::AutolyserStack) -> Result<Self, Error> {
        let policy = settings.policy();
        let dialog = Dialog::new(Config {
            poll: ms(policy.poll_ms),
            ack: ms(policy.acknowledgment_ms),
            response: ms(policy.response_ms),
            inter_byte: ms(policy.interbyte_ms),
            storage: ms(policy.storage_ms),
            max_bytes: settings.frame_limit(),
            controls: policy.controls,
            recovery: if policy.recover_last_on_poll {
                Recovery::LastOnPoll
            } else {
                Recovery::ManualLast
            },
        })?;
        Ok(Self {
            dialog,
            start: Instant::now(),
            receipt: None,
            ready: false,
            recovering: false,
            guard: None,
            active: None,
            cursor: String::new(),
            end_code: AutolyserEnd::TransferError,
        })
    }

    async fn receive_loop(
        &mut self,
        machine: &intake::Connection<'_>,
        store: &Handle,
        conn: &mut analyte_transport::Connection,
        stop: &mut watch::Receiver<bool>,
        policy: &analyte_config::Autolyser,
    ) -> Result<(), Error> {
        loop {
            self.end_code = AutolyserEnd::InputFailed;
            let input = crate::machine::connection_input::journaled(
                machine,
                store,
                conn,
                stop,
                self.dialog
                    .next_deadline()
                    .map(|deadline| deadline.saturating_sub(self.start.elapsed())),
            )
            .await?;
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
                    self.ready = false;
                    vec![Event::Poll(Request::Next)]
                }
                Input::Bytes { bytes } => {
                    if self
                        .receive_bytes(bytes, machine, store, conn, policy)
                        .await?
                    {
                        return Ok(());
                    }
                    #[cfg(feature = "test-hooks")]
                    if self.ready
                        && self.guard.is_none()
                        && self.active.is_none()
                        && let Some(hook) = &machine.hooks.autolyser_quiescent
                    {
                        hook.pause(stop).await.map_err(Error::TestHook)?;
                    }
                    Vec::new()
                }
                Input::Tick => {
                    let actions = self.dialog.step(self.start.elapsed(), Event::Tick)?;
                    // Tick alone can initiate L recovery. Never interleave R with it.
                    if actions.is_empty() && self.ready && !self.recovering {
                        self.ready = false;
                        if let Some(actions) = self.next_download(store, machine).await? {
                            if process(actions, self, machine, store, conn, policy).await? {
                                return Ok(());
                            }
                            Vec::new()
                        } else {
                            vec![Event::Poll(Request::Next)]
                        }
                    } else {
                        if process(actions, self, machine, store, conn, policy).await? {
                            return Ok(());
                        }
                        Vec::new()
                    }
                }
                Input::Closed(_) | Input::Failed(_) | Input::RecordingFailed(_) => {
                    vec![Event::Disconnect]
                }
            };
            for event in events {
                if matches!(event, Event::Poll(Request::Next)) {
                    self.guard = Some(custody::reserve(store, machine).await?);
                }
                let actions = self.dialog.step(self.start.elapsed(), event)?;
                if process(actions, self, machine, store, conn, policy).await? {
                    return Ok(());
                }
            }
            if ended {
                return Ok(());
            }
        }
    }

    async fn receive_bytes(
        &mut self,
        bytes: Vec<u8>,
        machine: &intake::Connection<'_>,
        store: &Handle,
        conn: &mut analyte_transport::Connection,
        policy: &analyte_config::Autolyser,
    ) -> Result<bool, Error> {
        for byte in bytes {
            let actions = self.dialog.step(self.start.elapsed(), Event::Byte(byte))?;
            if process(actions, self, machine, store, conn, policy).await? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    async fn close(
        &self,
        machine: &intake::Connection<'_>,
        store: &Handle,
        result: Result<(), Error>,
    ) -> Result<(), Error> {
        let mut result = result;
        if let Err(effect) =
            custody::uncertain(store, self.guard, machine, self.end_code.as_str()).await
        {
            result = Err(receive_custody::effect_failure(result.err(), effect));
        }
        if let Some(order) = self.active.as_ref()
            && let Err(effect) =
                crate::orders::sending::uncertain(store, order, self.end_code.as_str()).await
        {
            result = Err(receive_custody::effect_failure(result.err(), effect.into()));
        }
        result
    }
}

impl ConnectionState {
    async fn store_report(
        &mut self,
        token: u64,
        raw: Vec<u8>,
        actions: &mut VecDeque<Action>,
        machine: &intake::Connection<'_>,
        store: &Handle,
    ) -> Option<Error> {
        let prior_code = self.end_code;
        self.end_code = AutolyserEnd::StorageFailed;
        let id = ReceiptId::new();
        let stored =
            receive_custody::store_before_ack(machine, intake::receive(store, machine, id, raw))
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
        machine: &intake::Connection<'_>,
        store: &Handle,
    ) -> Result<(), Error> {
        let (id, _) = self
            .receipt
            .take()
            .ok_or_else(|| Error::ReportReceiptMissing)?;
        intake::interpret(store, machine, id).await?;
        self.ready = true;
        self.recovering = false;
        custody::complete(store, &mut self.guard, machine).await?;

        Ok(())
    }
}

async fn process(
    actions: Vec<Action>,
    state: &mut ConnectionState,
    machine: &intake::Connection<'_>,
    store: &Handle,
    conn: &mut analyte_transport::Connection,
    policy: &analyte_config::Autolyser,
) -> Result<bool, Error> {
    let mut actions: VecDeque<_> = actions.into();
    let mut close = false;
    let mut failure = None;
    while let Some(action) = actions.pop_front() {
        match action {
            Action::Write(bytes) => {
                state.end_code = AutolyserEnd::WriteFailed;
                receive_custody::write(conn, &bytes, ms(policy.acknowledgment_ms)).await?;
                state.end_code = AutolyserEnd::TransferError;
            }
            Action::Store(token, raw) => {
                if let Some(error) = state
                    .store_report(token, raw, &mut actions, machine, store)
                    .await
                {
                    failure = Some(error);
                }
            }
            Action::Report(_) => {
                state.interpret_report(machine, store).await?;
            }
            Action::Park(error) => {
                if let Some(order) = state.active.as_ref() {
                    let code = if matches!(error, analyte_dialog::autolyser::Error::Incomplete)
                        && matches!(
                            state.end_code,
                            AutolyserEnd::TransferTimeout
                                | AutolyserEnd::TransferDisconnected
                                | AutolyserEnd::TransferCancelled
                        ) {
                        state.end_code.incomplete()
                    } else {
                        park_code(&error)
                    };
                    crate::orders::sending::uncertain(store, order, &code).await?;
                    state.active = None;
                    close = true;
                }
                state.ready = false;
                state.recovering = true;
                let reason = format!("Autolyser: {error}");
                let parked = match state.receipt.take() {
                    Some((id, _)) => intake::failed_reading(store, machine, id, &reason).await,
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
                if let Err(error) = custody::uncertain(store, state.guard, machine, &reason).await {
                    return Err(receive_custody::effect_failure(failure, error));
                }
            }
            Action::Empty => {
                state.receipt.take();
                state.ready = !state.recovering;
                if !state.recovering {
                    custody::complete(store, &mut state.guard, machine).await?;
                }
            }
            Action::Insertion(reception) => {
                close |= state.complete_insertion(reception, store).await?;
            }
            Action::Uncertain => {
                state.ready = false;
                custody::uncertain(
                    store,
                    state.guard,
                    machine,
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

fn park_code(error: &analyte_dialog::autolyser::Error) -> String {
    use analyte_dialog::autolyser::Error;
    match error {
        Error::Config => Code::AutolyserParkedConfig.as_str().into(),
        Error::Busy => Code::AutolyserParkedBusy.as_str().into(),
        Error::Time => Code::AutolyserParkedTime.as_str().into(),
        Error::Incomplete => Code::AutolyserParkedIncomplete.as_str().into(),
        Error::Storage => Code::AutolyserParkedStorage.as_str().into(),
        Error::Stale => Code::AutolyserParkedStale.as_str().into(),
        Error::Ack => Code::AutolyserParkedAck.as_str().into(),
        Error::Framing(cause) => {
            crate::orders::outcome::Detail::AutolyserFraming(cause.to_string()).to_string()
        }
        Error::Record(cause) => {
            crate::orders::outcome::Detail::AutolyserRecord(cause.to_string()).to_string()
        }
    }
}
