//! VIDAS upload custody, with installed checksum, suffix and timer policy.
use super::vendor::{self, Input, ms};
use crate::{Error, evidence, invalid};
use analyte_dialog::vidas::{Action, Dialog, Event, Policy, Timer};
use analyte_framing::vidas::{Config, Decoder};
use analyte_model::ReceiptId;
use analyte_store::Handle;
use analyte_vidas::syntax::Message;
use std::{collections::VecDeque, time::Instant};
use tokio::sync::watch;

fn timer_index(timer: Timer) -> usize {
    match timer {
        Timer::Acknowledgment => 0,
        Timer::Interbyte => 1,
        Timer::Session => 2,
        Timer::Commit => 3,
    }
}

/// Dialog, decoder and timers cannot outlive their owning transport connection.
struct ConnectionState {
    dialog: Dialog,
    decoder: Option<Decoder>,
    suffix: Option<(u8, usize)>,
    timers: [Option<std::time::Duration>; 4],
    start: Instant,
    actions: VecDeque<Action>,
    active: Option<analyte_store::MachineOrder>,
    requests: VecDeque<Message>,
    idle: bool,
    opened: bool,
    end_code: &'static str,
    cursor: String,
    next_poll: Instant,
    config: Config,
    policy: Policy,
}

pub(super) async fn session(
    machine: &evidence::Connection<'_>,
    settings: &analyte_config::VidasStack,
    store: &Handle,
    conn: &mut analyte_transport::Connection,
    mut stop: watch::Receiver<bool>,
) -> Result<(), Error> {
    let installed = &settings.policy;
    let config = super::interpretation::vidas_config(machine)?;
    let policy = Policy {
        acknowledgment: ms(installed.acknowledgment_ms),
        interbyte: ms(installed.interbyte_ms),
        session: ms(installed.session_ms),
        commit: ms(installed.commit_ms),
        retries: installed.retries,
        backoff: ms(installed.backoff_ms),
    };
    let dialog = Dialog::upload(config.clone(), policy.clone())?;
    let actions = dialog.start().into();
    let mut state = ConnectionState {
        dialog,
        decoder: None,
        suffix: None,
        timers: [None; 4],
        start: Instant::now(),
        actions,
        active: None,
        requests: VecDeque::new(),
        idle: true,
        opened: false,
        end_code: "VIDAS-transfer-error",
        cursor: String::new(),
        next_poll: Instant::now(),
        config: config.clone(),
        policy,
    };
    // Cancellation after reservation leaves Sending for restart recovery. Every
    // observed non-normal end is committed Uncertain before this owner returns.
    let result = async {
        loop {
            if state.idle && state.opened && Instant::now() >= state.next_poll {
                state.next_poll = Instant::now()
                    .checked_add(ms(1000))
                    .ok_or_else(|| invalid("VIDAS order poll deadline overflow"))?;
                if let Some(reserved) =
                    crate::orders::sending::next(store, machine, &mut state.cursor).await?
                {
                    state.active = Some(reserved.order);
                    let analyte_mapping::MachineOrder::Vidas(requests) = reserved.payload else {
                        return Err(invalid("VIDAS received another stack's order"));
                    };
                    state.requests = requests.into();
                    start_download(&mut state)?;
                }
            }
            if process_inline(&mut state, machine, store, conn, installed).await? {
                return Ok(());
            }
            let mut next = state.timers.iter().flatten().copied().min().unwrap_or(
                state
                    .start
                    .elapsed()
                    .saturating_add(ms(installed.session_ms)),
            );
            if state.idle && state.opened {
                next = next.min(
                    state
                        .start
                        .elapsed()
                        .saturating_add(state.next_poll.saturating_duration_since(Instant::now())),
                );
            }
            state.end_code = "VIDAS-input-failed";
            let input = vendor::input(
                machine,
                store,
                conn,
                &mut stop,
                next.saturating_sub(state.start.elapsed()),
            )
            .await?;
            state.end_code = "VIDAS-transfer-error";
            match input {
                Input::Opened => state.opened = true,
                Input::Tick => {
                    for timer in [
                        Timer::Session,
                        Timer::Commit,
                        Timer::Acknowledgment,
                        Timer::Interbyte,
                    ] {
                        if state
                            .timers
                            .get(timer_index(timer))
                            .copied()
                            .flatten()
                            .is_some_and(|d| d <= state.start.elapsed())
                        {
                            state.end_code = match timer {
                                Timer::Acknowledgment => "VIDAS-acknowledgment-timeout",
                                Timer::Interbyte => "VIDAS-interbyte-timeout",
                                Timer::Session => "VIDAS-session-timeout",
                                Timer::Commit => "VIDAS-commit-timeout",
                            };
                            state.actions.extend(state.dialog.on(Event::Expired(timer)));
                            break;
                        }
                    }
                }
                Input::End => {
                    state.end_code = if *stop.borrow() {
                        "VIDAS-transfer-cancelled"
                    } else {
                        "VIDAS-transfer-disconnected"
                    };
                    state.actions.extend(state.dialog.on(Event::Disconnected));
                    process_inline(&mut state, machine, store, conn, installed).await?;
                    return Ok(());
                }
                Input::Bytes(bytes) => {
                    state.idle = false;
                    for byte in bytes {
                        if let Some(deadline) = state.timers.get_mut(timer_index(Timer::Interbyte))
                        {
                            *deadline = Some(
                                state
                                    .start
                                    .elapsed()
                                    .saturating_add(ms(installed.interbyte_ms)),
                            );
                        }
                        let event = if let Some(decoder) = &mut state.decoder {
                            match Decoder::push(decoder, byte) {
                                Ok(Some(packet)) => Some(Event::Packet(packet)),
                                Ok(None) => None,
                                Err(error) => {
                                    state.end_code = "VIDAS-framing-error";
                                    evidence::park_bytes(
                                        store,
                                        machine,
                                        vec![byte],
                                        &format!("VIDAS framing: {error}"),
                                    )
                                    .await?;
                                    return Err(error.into());
                                }
                            }
                        } else if let Some((control, index)) = state.suffix {
                            if installed.suffix.get(index) != Some(&byte) {
                                state.end_code = "VIDAS-control-suffix-mismatch";
                                evidence::park_bytes(
                                    store,
                                    machine,
                                    vec![byte],
                                    "VIDAS control suffix mismatch",
                                )
                                .await?;
                                return Err(invalid("VIDAS control suffix mismatch"));
                            }
                            if index.saturating_add(1) == installed.suffix.len() {
                                state.suffix = None;
                                Some(Event::Control(control))
                            } else {
                                state.suffix = Some((control, index.saturating_add(1)));
                                None
                            }
                        } else if byte == 2 {
                            let mut next = Decoder::new(config.clone());
                            next.push(byte)?;
                            state.decoder = Some(next);
                            None
                        } else if installed.suffix.is_empty() {
                            Some(Event::Control(byte))
                        } else {
                            state.suffix = Some((byte, 0usize));
                            None
                        };
                        if let Some(event) = event {
                            if matches!(event, Event::Packet(_)) {
                                state.decoder = None;
                            }
                            state.actions.extend(state.dialog.on(event));
                            // Process Store before consuming a merged ETX/EOT following it.
                            if process_inline(&mut state, machine, store, conn, installed).await? {
                                return Ok(());
                            }
                        }
                    }
                }
            }
        }
    }
    .await;
    if let Some(order) = state.active.as_ref() {
        crate::orders::sending::uncertain(store, order, state.end_code).await?;
    }
    result
}

fn start_download(state: &mut ConnectionState) -> Result<(), Error> {
    let request = state
        .requests
        .pop_front()
        .ok_or_else(|| invalid("VIDAS mapped request missing"))?;
    state.dialog = Dialog::download(state.config.clone(), state.policy.clone(), &request)?;
    state.start = Instant::now();
    state.timers = [None; 4];
    state.idle = false;
    state.end_code = "VIDAS-transfer-error";
    state.actions.extend(state.dialog.start());
    Ok(())
}

async fn process_inline(
    state: &mut ConnectionState,
    machine: &evidence::Connection<'_>,
    store: &Handle,
    conn: &mut analyte_transport::Connection,
    installed: &analyte_config::Vidas,
) -> Result<bool, Error> {
    let mut failure = None;
    let mut complete = false;
    while let Some(action) = state.actions.pop_front() {
        match action {
            Action::Send(bytes) => {
                state.end_code = "VIDAS-write-failed";
                vendor::write(conn, &bytes, ms(installed.acknowledgment_ms)).await?;
                state.end_code = "VIDAS-transfer-error";
            }
            Action::Store(packet) => {
                let id = ReceiptId::new();
                let operation = async {
                    let time = evidence::receive(store, machine, id, packet.raw.clone()).await?;
                    let reading = super::interpretation::map(machine, id, time, &packet.raw);
                    match reading {
                        Ok(reading) => {
                            evidence::vendor_reading(store, machine, id, reading).await?;
                        }
                        Err(error) => {
                            evidence::failed_reading(
                                store,
                                machine,
                                id,
                                &format!("VIDAS interpretation: {error}"),
                            )
                            .await?;
                        }
                    }
                    Ok::<_, Error>(())
                };
                match tokio::time::timeout(ms(installed.commit_ms), operation).await {
                    Ok(Ok(())) => {
                        // Completion effects precede previously queued timer changes.
                        state.actions.extend(state.dialog.on(Event::Stored));
                    }
                    Ok(Err(error)) => {
                        state.actions.extend(state.dialog.on(Event::StorageFailed));
                        failure = Some(error);
                    }
                    Err(_) => {
                        state.actions.extend(state.dialog.on(Event::StorageFailed));
                        failure =
                            Some(invalid("VIDAS commit deadline exceeded; custody uncertain"));
                    }
                }
            }
            Action::Arm(timer, wait) => {
                *state
                    .timers
                    .get_mut(timer_index(timer))
                    .ok_or_else(|| invalid("VIDAS timer index"))? =
                    Some(state.start.elapsed().saturating_add(wait));
            }
            Action::Cancel(timer) => {
                *state
                    .timers
                    .get_mut(timer_index(timer))
                    .ok_or_else(|| invalid("VIDAS timer index"))? = None;
            }
            Action::Park => {
                if state.end_code == "VIDAS-transfer-error" {
                    state.end_code = "VIDAS-transfer-refused-or-unexpected";
                }
                let parked = evidence::park_bytes(store, machine, Vec::new(), "VIDAS transfer incomplete or refused; retained transport journal requires reconciliation").await;
                if let Err(error) = parked {
                    return Err(vendor::effect_failure(failure, error));
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
                            "VIDAS-transfer-confirmed",
                        )
                        .await?;
                        state.active = None;
                        state.dialog = Dialog::upload(state.config.clone(), state.policy.clone())?;
                        state.start = Instant::now();
                        state.timers = [None; 4];
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
