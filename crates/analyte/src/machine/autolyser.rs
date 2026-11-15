//! Host-polled Autolyser connection; a corrupt report blocks R until L recovery.
use super::vendor::{self, Input, ms};
use crate::{Error, config::Machine, evidence, invalid};
use analyte_autolyser::dialog::{Action, Config, Dialog, Event, Recovery, Request};
use analyte_model::ReceiptId;
use analyte_store::Handle;
use std::{collections::VecDeque, time::Instant};
use tokio::sync::watch;

/// Mutable conversation state has one owner for the entire connection.
struct ConnectionState {
    dialog: Dialog,
    start: Instant,
    wake: std::time::Duration,
    receipt: Option<(ReceiptId, jiff::Timestamp)>,
    ready: bool,
    recovering: bool,
    guard: Option<i64>,
    active: Option<analyte_store::MachineOrder>,
    cursor: String,
    end_code: &'static str,
}

pub(super) async fn session(
    machine: &Machine,
    store: &Handle,
    conn: &mut analyte_transport::Connection,
    mut stop: watch::Receiver<bool>,
) -> Result<(), Error> {
    super::autolyser_custody::ensure_reconciled(store, machine).await?;
    let settings = machine.snapshot.effective().machine();
    let policy = settings
        .autolyser
        .as_ref()
        .ok_or_else(|| invalid("Autolyser policy missing"))?;
    let dialog = Dialog::new(Config {
        poll: ms(policy.poll_ms),
        ack: ms(policy.acknowledgment_ms),
        response: ms(policy.response_ms),
        inter_byte: ms(policy.interbyte_ms),
        storage: ms(policy.storage_ms),
        max_bytes: settings.framing.limit,
        controls: policy.controls,
        recovery: if policy.recover_last_on_poll {
            Recovery::LastOnPoll
        } else {
            Recovery::ManualLast
        },
    })?;
    let mut state = ConnectionState {
        dialog,
        start: Instant::now(),
        wake: std::time::Duration::ZERO,
        receipt: None,
        ready: false,
        recovering: false,
        guard: None,
        active: None,
        cursor: String::new(),
        end_code: "Autolyser-transfer-error",
    };
    // Dropping this owner after reservation leaves Sending for shared recovery.
    // Observed failures are finalized Uncertain before returning, without replay.
    let result = async {
        loop {
            state.end_code = "Autolyser-input-failed";
            let input = vendor::input(
                machine,
                store,
                conn,
                &mut stop,
                state.wake.saturating_sub(state.start.elapsed()),
            )
            .await?;
            let ended = matches!(input, Input::End);
            state.end_code = match &input {
                Input::End => {
                    if *stop.borrow() {
                        "Autolyser-transfer-cancelled"
                    } else {
                        "Autolyser-transfer-disconnected"
                    }
                }
                Input::Tick => "Autolyser-transfer-timeout",
                Input::Opened | Input::Bytes(_) => "Autolyser-transfer-error",
            };
            let events = match input {
                Input::Opened => {
                    state.ready = false;
                    vec![Event::Poll(Request::Next)]
                }
                Input::Bytes(bytes) => bytes.into_iter().map(Event::Byte).collect(),
                Input::Tick => {
                    let actions = state.dialog.step(state.start.elapsed(), Event::Tick);
                    // Tick alone can initiate L recovery. Never interleave R with it.
                    if actions.is_empty() && state.ready && !state.recovering {
                        state.ready = false;
                        if let Some(reserved) =
                            crate::orders::sending::next(store, machine, &mut state.cursor).await?
                        {
                            state.active = Some(reserved.order);
                            let analyte_mapping::MachineOrder::Autolyser(worklist) =
                                reserved.payload
                            else {
                                return Err(invalid("Autolyser received another stack's order"));
                            };
                            let actions = state
                                .dialog
                                .step(state.start.elapsed(), Event::Download(&worklist));
                            if process(actions, &mut state, machine, store, conn).await? {
                                return Ok(());
                            }
                            Vec::new()
                        } else {
                            vec![Event::Poll(Request::Next)]
                        }
                    } else {
                        if process(actions, &mut state, machine, store, conn).await? {
                            return Ok(());
                        }
                        if state.wake <= state.start.elapsed() {
                            state.wake = state.start.elapsed().saturating_add(ms(policy.poll_ms));
                        }
                        Vec::new()
                    }
                }
                Input::End => vec![Event::Disconnect],
            };
            for event in events {
                if matches!(event, Event::Poll(Request::Next)) {
                    state.guard = Some(super::autolyser_custody::reserve(store, machine).await?);
                }
                let actions = state.dialog.step(state.start.elapsed(), event);
                if process(actions, &mut state, machine, store, conn).await? {
                    return Ok(());
                }
            }
            if ended {
                return Ok(());
            }
        }
    }
    .await;
    if let Some(order) = state.active.as_ref() {
        crate::orders::sending::uncertain(store, order, state.end_code).await?;
    }
    result
}

async fn process(
    actions: Vec<Action>,
    state: &mut ConnectionState,
    machine: &Machine,
    store: &Handle,
    conn: &mut analyte_transport::Connection,
) -> Result<bool, Error> {
    let ConnectionState {
        dialog,
        start,
        wake,
        receipt,
        ready,
        recovering,
        guard,
        active,
        end_code,
        ..
    } = state;
    let policy = machine
        .snapshot
        .effective()
        .machine()
        .autolyser
        .as_ref()
        .ok_or_else(|| invalid("Autolyser policy missing"))?;
    let mut actions: VecDeque<_> = actions.into();
    let mut close = false;
    while let Some(action) = actions.pop_front() {
        match action {
            Action::Write(bytes) => {
                *end_code = "Autolyser-write-failed";
                vendor::write(conn, &bytes, ms(policy.acknowledgment_ms)).await?;
                *end_code = "Autolyser-transfer-error";
            }
            Action::Store(token, raw) => {
                let prior_code = *end_code;
                *end_code = "Autolyser-storage-failed";
                let id = ReceiptId::new();
                let stored = tokio::time::timeout(
                    ms(policy.storage_ms),
                    evidence::receive(store, machine, id, raw),
                )
                .await;
                match stored {
                    Ok(Ok(time)) => {
                        *end_code = prior_code;
                        *receipt = Some((id, time));
                        actions.extend(dialog.step(start.elapsed(), Event::Stored(token)));
                    }
                    Ok(Err(error)) => {
                        let _ = dialog.step(start.elapsed(), Event::StoreFailed(token));
                        return Err(error);
                    }
                    Err(_) => {
                        *end_code = "Autolyser-storage-timeout";
                        let _ = dialog.step(start.elapsed(), Event::StoreFailed(token));
                        return Err(invalid(
                            "Autolyser storage deadline exceeded; custody uncertain",
                        ));
                    }
                }
            }
            Action::Report(report) => {
                let (id, time) = receipt
                    .take()
                    .ok_or_else(|| invalid("Autolyser report receipt missing"))?;
                match super::interpretation::map(machine, id, time, &report.raw) {
                    Ok(reading) => evidence::vendor_reading(store, machine, id, reading).await?,
                    Err(error) => {
                        vendor::failed(
                            machine,
                            store,
                            id,
                            &format!("Autolyser interpretation: {error}"),
                        )
                        .await?;
                    }
                }
                *ready = true;
                *recovering = false;
                super::autolyser_custody::complete(store, guard, machine).await?;
            }
            Action::Park(error) => {
                if let Some(order) = active.as_ref() {
                    let code = if matches!(error, analyte_autolyser::dialog::Error::Incomplete)
                        && matches!(
                            *end_code,
                            "Autolyser-transfer-timeout"
                                | "Autolyser-transfer-disconnected"
                                | "Autolyser-transfer-cancelled"
                        ) {
                        format!("{end_code}-incomplete")
                    } else {
                        park_code(&error)
                    };
                    crate::orders::sending::uncertain(store, order, &code).await?;
                    *active = None;
                    close = true;
                }
                *ready = false;
                *recovering = true;
                if let Some((id, _)) = receipt.take() {
                    vendor::failed(machine, store, id, &format!("Autolyser: {error}")).await?;
                }
            }
            Action::Empty => {
                receipt.take();
                *ready = !*recovering;
                if !*recovering {
                    super::autolyser_custody::complete(store, guard, machine).await?;
                }
            }
            Action::Insertion(reception) => {
                receipt.take();
                let order = active
                    .as_ref()
                    .ok_or_else(|| invalid("unexpected Autolyser insertion outcome"))?;
                let (outcome, code) = match reception {
                    analyte_autolyser::dialog::Reception::Accepted(position) => (
                        analyte_model::OrderState::Sent,
                        format!("Autolyser-insertion-confirmed-position-{position:02X}"),
                    ),
                    analyte_autolyser::dialog::Reception::Rejected(error) => (
                        analyte_model::OrderState::Uncertain,
                        format!("Autolyser-insertion-rejected-{error:?}"),
                    ),
                    analyte_autolyser::dialog::Reception::Other(prefix, detail) => (
                        analyte_model::OrderState::Uncertain,
                        format!("Autolyser-insertion-unknown-{prefix:02X}-{detail:02X}"),
                    ),
                };
                crate::orders::sending::complete(store, order, outcome, &code).await?;
                *active = None;
                *ready = outcome == analyte_model::OrderState::Sent;
                if !*ready {
                    close = true;
                }
            }
            Action::Uncertain => {
                *ready = false;
            }
            Action::Refused(error) => return Err(error.into()),
            Action::Close => close = true,
            Action::Deadline(deadline) => *wake = deadline,
        }
    }
    Ok(close)
}

fn park_code(error: &analyte_autolyser::dialog::Error) -> String {
    use analyte_autolyser::dialog::Error;
    match error {
        Error::Config => "Autolyser-parked-config".into(),
        Error::Busy => "Autolyser-parked-busy".into(),
        Error::Time => "Autolyser-parked-time".into(),
        Error::Incomplete => "Autolyser-parked-incomplete".into(),
        Error::Storage => "Autolyser-parked-storage".into(),
        Error::Stale => "Autolyser-parked-stale".into(),
        Error::Ack => "Autolyser-parked-ack".into(),
        Error::Record(cause) => format!("Autolyser-parked-record-{cause}"),
    }
}
