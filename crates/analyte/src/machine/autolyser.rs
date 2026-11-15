//! Host-polled Autolyser connection; a corrupt report blocks R until L recovery.
use super::{
    vendor::{self, Input, ms},
    vendor_model,
};
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
    };
    loop {
        let input = vendor::input(
            machine,
            store,
            conn,
            &mut stop,
            state.wake.saturating_sub(state.start.elapsed()),
        )
        .await?;
        let ended = matches!(input, Input::End);
        let events = match input {
            Input::Opened => {
                state.ready = false;
                vec![Event::Poll(Request::Next)]
            }
            Input::Bytes(bytes) => bytes.into_iter().map(Event::Byte).collect(),
            Input::Tick => {
                let actions = state.dialog.step(state.start.elapsed(), Event::Tick);
                // Tick alone can initiate L recovery. Never interleave R with it.
                if actions.is_empty() && state.ready {
                    state.ready = false;
                    vec![Event::Poll(Request::Next)]
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
    } = state;
    let policy = machine
        .snapshot
        .effective()
        .machine()
        .autolyser
        .as_ref()
        .ok_or_else(|| invalid("Autolyser policy missing"))?;
    let mut actions: VecDeque<_> = actions.into();
    while let Some(action) = actions.pop_front() {
        match action {
            Action::Write(bytes) => {
                vendor::write(conn, &bytes, ms(policy.acknowledgment_ms)).await?;
            }
            Action::Store(token, raw) => {
                let id = ReceiptId::new();
                let stored = tokio::time::timeout(
                    ms(policy.storage_ms),
                    evidence::receive(store, machine, id, raw),
                )
                .await;
                match stored {
                    Ok(Ok(time)) => {
                        *receipt = Some((id, time));
                        actions.extend(dialog.step(start.elapsed(), Event::Stored(token)));
                    }
                    Ok(Err(error)) => {
                        let _ = dialog.step(start.elapsed(), Event::StoreFailed(token));
                        return Err(error);
                    }
                    Err(_) => {
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
                match vendor_model::autolyser(machine, id, &report, time) {
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
            Action::Insertion(_) => return Err(invalid("unexpected Autolyser insertion outcome")),
            Action::Uncertain => {
                *ready = false;
            }
            Action::Refused(error) => return Err(error.into()),
            Action::Close => return Ok(true),
            Action::Deadline(deadline) => *wake = deadline,
        }
    }
    Ok(false)
}
