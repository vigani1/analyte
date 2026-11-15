//! VIDAS upload custody, with installed checksum, suffix and timer policy.
use super::{
    vendor::{self, Input, ms},
    vendor_model,
};
use crate::{Error, config::Machine, evidence, invalid};
use analyte_model::ReceiptId;
use analyte_store::Handle;
use analyte_vidas::{
    dialog::{Action, Dialog, Event, Policy, Timer},
    framing::{Checksum, Config, Decoder, Mode},
    syntax::{Message, SampleLimit},
};
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
}

pub(super) async fn session(
    machine: &Machine,
    store: &Handle,
    conn: &mut analyte_transport::Connection,
    mut stop: watch::Receiver<bool>,
) -> Result<(), Error> {
    let settings = machine.snapshot.effective().machine();
    let installed = settings
        .vidas
        .as_ref()
        .ok_or_else(|| invalid("VIDAS settings missing"))?;
    let config = Config::new(
        if installed.alternate {
            Mode::Alternate
        } else {
            Mode::Literal
        },
        installed.chunk,
        settings.framing.limit,
        &installed.suffix,
        Checksum {
            include_stx: installed.include_stx,
            include_suffix: installed.include_suffix,
            uppercase: installed.uppercase,
            accept_either_case: installed.accept_either_case,
        },
    )?;
    let policy = Policy {
        acknowledgment: ms(installed.acknowledgment_ms),
        interbyte: ms(installed.interbyte_ms),
        session: ms(installed.session_ms),
        commit: ms(installed.commit_ms),
        retries: installed.retries,
        backoff: ms(installed.backoff_ms),
    };
    let dialog = Dialog::upload(config.clone(), policy)?;
    let actions = dialog.start().into();
    let mut state = ConnectionState {
        dialog,
        decoder: None,
        suffix: None,
        timers: [None; 4],
        start: Instant::now(),
        actions,
    };
    loop {
        if process_inline(&mut state, machine, store, conn, installed).await? {
            return Ok(());
        }
        let next = state.timers.iter().flatten().copied().min().unwrap_or(
            state
                .start
                .elapsed()
                .saturating_add(ms(installed.session_ms)),
        );
        let input = vendor::input(
            machine,
            store,
            conn,
            &mut stop,
            next.saturating_sub(state.start.elapsed()),
        )
        .await?;
        match input {
            Input::Opened => {}
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
                        state.actions.extend(state.dialog.on(Event::Expired(timer)));
                        break;
                    }
                }
            }
            Input::End => {
                let _ = state.dialog.on(Event::Disconnected);
                return Ok(());
            }
            Input::Bytes(bytes) => {
                for byte in bytes {
                    if let Some(deadline) = state.timers.get_mut(timer_index(Timer::Interbyte)) {
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

async fn process_inline(
    state: &mut ConnectionState,
    machine: &Machine,
    store: &Handle,
    conn: &mut analyte_transport::Connection,
    installed: &analyte_config::Vidas,
) -> Result<bool, Error> {
    let ConnectionState {
        actions,
        dialog,
        timers,
        start,
        ..
    } = state;
    while let Some(action) = actions.pop_front() {
        match action {
            Action::Send(bytes) => {
                vendor::write(conn, &bytes, ms(installed.acknowledgment_ms)).await?;
            }
            Action::Store(packet) => {
                let id = ReceiptId::new();
                let operation = async {
                    let time = evidence::receive(store, machine, id, packet.raw).await?;
                    let reading = std::str::from_utf8(&packet.body)
                        .map_err(|e| -> Error { e.into() })
                        .and_then(|text| {
                            Message::parse(
                                text,
                                if installed.sample_limit == 12 {
                                    SampleLimit::Twelve
                                } else {
                                    SampleLimit::Seventeen
                                },
                            )
                            .map_err(Into::into)
                        })
                        .and_then(|message| vendor_model::vidas(machine, id, &message, time));
                    match reading {
                        Ok(reading) => {
                            evidence::vendor_reading(store, machine, id, reading).await?;
                        }
                        Err(error) => {
                            vendor::failed(
                                machine,
                                store,
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
                        actions.extend(dialog.on(Event::Stored));
                    }
                    Ok(Err(error)) => {
                        let _ = dialog.on(Event::StorageFailed);
                        return Err(error);
                    }
                    Err(_) => {
                        let _ = dialog.on(Event::StorageFailed);
                        return Err(invalid("VIDAS commit deadline exceeded; custody uncertain"));
                    }
                }
            }
            Action::Arm(timer, wait) => {
                *timers
                    .get_mut(timer_index(timer))
                    .ok_or_else(|| invalid("VIDAS timer index"))? =
                    Some(start.elapsed().saturating_add(wait));
            }
            Action::Cancel(timer) => {
                *timers
                    .get_mut(timer_index(timer))
                    .ok_or_else(|| invalid("VIDAS timer index"))? = None;
            }
            Action::Park => {
                evidence::park_bytes(store, machine, Vec::new(), "VIDAS transfer incomplete or refused; retained transport journal requires reconciliation").await?;
                return Err(invalid("VIDAS transfer parked; reconcile before reconnect"));
            }
            Action::Complete => return Ok(true),
        }
    }
    Ok(false)
}
