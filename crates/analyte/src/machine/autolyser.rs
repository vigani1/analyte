//! Runs an Autolyser session: polls for reports, stores each before acknowledging, downloads worklists between polls, and holds next-report (R) polls during last-report (L) recovery.
use super::connection_input::Input;
use super::custody::{self, ms};
use super::transfer_end::AutolyserEnd;
use crate::{Error, intake, invalid};
use analyte_dialog::autolyser::{Action, Config, Dialog, Event, Recovery, Request};
use analyte_model::ReceiptId;
use analyte_store::Handle;
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
    super::poll_custody::ensure_reconciled(store, machine).await?;
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
            let input = super::connection_input::journaled(
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
                    Vec::new()
                }
                Input::Tick => {
                    let actions = self.dialog.step(self.start.elapsed(), Event::Tick);
                    // Tick alone can initiate L recovery. Never interleave R with it.
                    if actions.is_empty() && self.ready && !self.recovering {
                        self.ready = false;
                        if let Some(reserved) =
                            crate::orders::sending::next(store, machine, &mut self.cursor).await?
                        {
                            self.active = Some(reserved.order);
                            let analyte_mapping::MachineOrder::Autolyser(worklist) =
                                reserved.payload
                            else {
                                return Err(invalid("Autolyser received another stack's order"));
                            };
                            let actions = self
                                .dialog
                                .step(self.start.elapsed(), Event::Download(&worklist));
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
                    self.guard = Some(super::poll_custody::reserve(store, machine).await?);
                }
                let actions = self.dialog.step(self.start.elapsed(), event);
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
            let actions = self.dialog.step(self.start.elapsed(), Event::Byte(byte));
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
            super::poll_custody::uncertain(store, self.guard, machine, self.end_code.as_str()).await
        {
            result = Err(custody::effect_failure(result.err(), effect));
        }
        if let Some(order) = self.active.as_ref()
            && let Err(effect) =
                crate::orders::sending::uncertain(store, order, self.end_code.as_str()).await
        {
            result = Err(custody::effect_failure(result.err(), effect));
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
        let stored = custody::store_within(
            custody::budget(machine),
            intake::receive(store, machine, id, raw),
        )
        .await;
        match stored {
            Ok(time) => {
                self.end_code = prior_code;
                self.receipt = Some((id, time));
                actions.extend(self.dialog.step(self.start.elapsed(), Event::Stored(token)));
            }
            Err(error) => {
                if error.downcast_ref::<custody::StoreTimeout>().is_some() {
                    self.end_code = AutolyserEnd::StorageTimeout;
                }
                actions.extend(
                    self.dialog
                        .step(self.start.elapsed(), Event::StoreFailed(token)),
                );
                return Some(error);
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
            .ok_or_else(|| invalid("Autolyser report receipt missing"))?;
        let receipt = store
            .request(crate::deadline(), move |s| {
                s.receipt(id)?.ok_or(analyte_store::Error::UnknownReceipt)
            })
            .await?;
        match super::interpretation::map_retained(store, machine, &receipt).await {
            Ok(reading) => intake::vendor_reading(store, machine, id, reading).await?,
            Err(error) => {
                intake::failed_reading(
                    store,
                    machine,
                    id,
                    &format!("Autolyser interpretation: {error}"),
                )
                .await?;
            }
        }
        self.ready = true;
        self.recovering = false;
        super::poll_custody::complete(store, &mut self.guard, machine).await?;

        Ok(())
    }
    async fn complete_insertion(
        &mut self,
        reception: analyte_dialog::autolyser::Reception,
        store: &Handle,
    ) -> Result<bool, Error> {
        self.receipt.take();
        let order = self
            .active
            .as_ref()
            .ok_or_else(|| invalid("unexpected Autolyser insertion outcome"))?;
        let (outcome, code) = match reception {
            analyte_dialog::autolyser::Reception::Accepted(position) => (
                analyte_model::OrderState::Sent,
                format!("Autolyser-insertion-confirmed-position-{position:02X}"),
            ),
            analyte_dialog::autolyser::Reception::Rejected(error) => (
                analyte_model::OrderState::Uncertain,
                format!(
                    "Autolyser-insertion-rejected-{}",
                    super::transfer_end::insertion_rejection(error)
                ),
            ),
            analyte_dialog::autolyser::Reception::Other(prefix, detail) => (
                analyte_model::OrderState::Uncertain,
                format!("Autolyser-insertion-unknown-{prefix:02X}-{detail:02X}"),
            ),
        };
        crate::orders::sending::complete(store, order, outcome, &code).await?;
        self.active = None;
        self.ready = outcome == analyte_model::OrderState::Sent;

        Ok(!self.ready)
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
                custody::write(conn, &bytes, ms(policy.acknowledgment_ms)).await?;
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
                        format!("{}-incomplete", state.end_code)
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
                    None => intake::park_bytes(store, machine, Vec::new(), &reason).await,
                };
                if let Err(error) = parked {
                    return Err(custody::effect_failure(failure, error));
                }
                if let Err(error) =
                    super::poll_custody::uncertain(store, state.guard, machine, &reason).await
                {
                    return Err(custody::effect_failure(failure, error));
                }
            }
            Action::Empty => {
                state.receipt.take();
                state.ready = !state.recovering;
                if !state.recovering {
                    super::poll_custody::complete(store, &mut state.guard, machine).await?;
                }
            }
            Action::Insertion(reception) => {
                close |= state.complete_insertion(reception, store).await?;
            }
            Action::Uncertain => {
                state.ready = false;
                super::poll_custody::uncertain(
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
        Error::Config => "Autolyser-parked-config".into(),
        Error::Busy => "Autolyser-parked-busy".into(),
        Error::Time => "Autolyser-parked-time".into(),
        Error::Incomplete => "Autolyser-parked-incomplete".into(),
        Error::Storage => "Autolyser-parked-storage".into(),
        Error::Stale => "Autolyser-parked-stale".into(),
        Error::Ack => "Autolyser-parked-ack".into(),
        Error::Framing(cause) => format!("Autolyser-parked-framing-{cause}"),
        Error::Record(cause) => format!("Autolyser-parked-record-{cause}"),
    }
}
