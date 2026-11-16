//! HL7 store-and-forward ACK decisions follow durable interpretation.
use super::connection_input::{self, Input};
use crate::{Error, evidence, invalid};
use analyte_dialog::hl7::{Action, Config, Dialog, Event};
use analyte_model::ReceiptId;
use analyte_store::Handle;
use std::{
    collections::{BTreeMap, VecDeque},
    time::{Duration, Instant},
};
use tokio::sync::watch;

pub(super) async fn session(
    machine: &evidence::Connection<'_>,
    settings: &analyte_config::Hl7Stack,
    store: &Handle,
    conn: &mut analyte_transport::Connection,
    mut stop: watch::Receiver<bool>,
) -> Result<(), Error> {
    let mut options = settings.syntax().options();
    options.limits.message_bytes = options.limits.message_bytes.min(settings.frame_limit());
    let config = Config::new(
        options,
        Duration::from_millis(u64::from(settings.timeouts().receive_timeout_ms())),
        1,
    )?;
    let mut dialog = Dialog::new(config, 0);
    let start = Instant::now();
    let mut wake = None;
    let mut pending = BTreeMap::new();
    let enabled = settings
        .orders()
        .as_ref()
        .is_some_and(|o| o.query().is_some() || o.send().is_some());
    let mut queries = if enabled {
        Some(crate::orders::hl7::Connection::new(options)?)
    } else {
        None
    };
    if enabled {
        dialog.enable_orders(settings.syntax().edition().into())?;
    }
    let mut input_evidence = Vec::new();
    let mut write_error = None;
    let result = async {
        loop {
            let wait = wake.map_or(Duration::from_secs(1), |d: Duration| {
                d.saturating_sub(start.elapsed())
            });

            let event = tokio::select! {
                biased;
                () = super::stopping(&mut stop) => break,
                () = tokio::time::sleep(wait) => None,
                event = connection_input::receive(conn) => match event {
                    Ok(event) => Some(event),
                    Err(analyte_transport::Error::Deadline(_)) => None,
                    Err(error) => return Err(error.into()),
                },
            };
            let bytes = match event {
                Some(Input::Opened { session }) => {
                    tracing::info!(
                        machine = machine.id.as_str(),
                        session,
                        "HL7 connection opened"
                    );
                    Vec::new()
                }
                Some(Input::Bytes { bytes }) => bytes,
                Some(Input::Closed(_)) => break,
                Some(
                    Input::Failed(_)
                    | Input::RecordingFailed(_),
                )
                | Some(Input::Tick) | None => transport_failure_as_empty_input(),
            };
            input_evidence = bytes.clone();
            let events = if bytes.is_empty() {
                vec![Event::Tick]
            } else {
                bytes.into_iter().map(Event::Byte).collect()
            };
            for event in events {
                let mut actions: VecDeque<_> = dialog.step(start.elapsed(), event)?.into();
                while let Some(action) = actions.pop_front() {
                    match action {
                        Action::ReceiptRetained(token) => {
                            pending.remove(&(token.epoch(), token.ordinal())).ok_or("late receipt absent")?;
                        }
                        Action::Query { receipt: token, message, started, edition_matches } => {
                            let id = pending.remove(&(token.epoch(), token.ordinal())).ok_or("query receipt absent")?;
                            if !edition_matches {
                                evidence::failed_reading(store,machine,id,"HL7 query edition differs from configured edition").await?;
                            } else {
                                let query = queries.as_mut().ok_or("HL7 query services absent")?;
                                let reply = tokio::select! { biased; () = super::stopping(&mut stop) => return Ok(()), result = query.answer(store,machine,crate::orders::hl7::Request { message: &message, started: start.checked_add(started).ok_or("query clock overflow")?, receipt: id },&dialog,start) => result? };
                                if let Some(reply) = reply { actions.extend(dialog.step(start.elapsed(), Event::QueryReply(&reply))?); }
                            }
                            actions.extend(dialog.step(start.elapsed(), Event::Tick)?);
                        }
                        Action::OrderResponse { receipt: token, message } => {
                            pending.remove(&(token.epoch(), token.ordinal())).ok_or("order response receipt absent")?;
                            queries.as_mut().ok_or("HL7 order services absent")?.order_response(store,Some(&message)).await?;
                        }
                        Action::OrderResponseTimeout => {
                            queries.as_mut().ok_or("HL7 order services absent")?.order_response(store,None).await?;
                        }
                        Action::DisplayOutcome { receipt, accepted, code, detail } => {
                            if let Some(token) = receipt {
                                let id = pending.remove(&(token.epoch(), token.ordinal())).ok_or("display receipt absent")?;
                                if let Some(detail) = detail { evidence::failed_reading(store,machine,id,&detail).await?; }
                            }
                            queries.as_mut().ok_or("HL7 display services absent")?.outcome(store,code,accepted).await?;
                        }
                        Action::Commit(token, raw) => {
                            let id = ReceiptId::new();
                            match evidence::receive(store, machine, id, raw).await {
                                Ok(_) => {
                                    if pending.len() >= 2 {
                                        return Err(invalid("HL7 pending receipt limit exceeded"));
                                    }
                                    pending.insert((token.epoch(), token.ordinal()), id);
                                    actions.extend(
                                        dialog.step(start.elapsed(), Event::Committed(token))?,
                                    );
                                }
                                Err(error) => {
                                    tracing::error!(
                                        machine = machine.id.as_str(),
                                        error_kind = %error,
                                        "HL7 durable commit refused"
                                    );
                                    actions.extend(
                                        dialog.step(start.elapsed(), Event::CommitFailed(token))?,
                                    );
                                }
                            }
                        }
                        Action::Interpret(token, _payload) => {
                            let id = pending
                                .remove(&(token.epoch(), token.ordinal()))
                                .ok_or_else(|| invalid("HL7 receipt correlation missing"))?;
                            let (outcome, location) =
                                evidence::interpret_with_location(store, machine, id).await?;
                            let event = if matches!(outcome,evidence::InterpretationOutcome::Accepted) {
                                Event::Interpreted(token)
                            } else {
                                Event::NotInterpretable(
                                    token,
                                    super::error_location::build(location.as_ref(), b"207", b"operator-review"),
                                    false,
                                )
                            };
                            actions.extend(dialog.step(start.elapsed(), event)?);
                        }
                        Action::ResponseNeeded(token, _) => {
                            let metadata = crate::machine::hl7_response::response_metadata(crate::machine::hl7_response::Profile::Hl7);
                            actions.extend(dialog.step(
                                start.elapsed(),
                                Event::Respond(
                                    token,
                                    metadata.response()?,
                                ),
                            )?);
                        }
                        Action::QueryWrite { bytes, until, part } => {
                            match conn.write_all(&bytes, Some(tokio::time::Instant::from_std(start.checked_add(until).ok_or("query write deadline overflow")?))).await {
                                Ok(()) => actions.extend(dialog.step(start.elapsed(), Event::QueryWritten(part))?),
                                Err(error) => {
                                    actions.extend(dialog.step(start.elapsed(), Event::QueryWriteFailed(part))?);
                                    write_error = Some(error);
                                },
                            }
                        }
                        Action::Write(bytes) => {
                            conn.write_all(&bytes, None).await?;
                        }
                        Action::Deadline(next) => wake = next,
                        Action::Rejected(error, bytes) => {
                            evidence::park_bytes(store, machine, bytes, &error.to_string()).await?;
                        }
                        Action::Parked(bytes, error) => {
                            pending.clear();
                            evidence::park_bytes(store, machine, bytes, &error.to_string()).await?;
                        }
                        Action::QueueFull(bytes) | Action::Unmatched(bytes) => {
                            evidence::park_bytes(
                                store,
                                machine,
                                bytes,
                                "HL7 unsolicited message or queue overflow",
                            )
                            .await?;
                            return Err(invalid("HL7 message refused"));
                        }
                        Action::Refused(error) => return Err(error.into()),
                        Action::TimedOut(_) => {
                            return Err(invalid(
                                "HL7 host phase timed out; receipt requires reconciliation",
                            ));
                        }
                        Action::Sent(_, _) | Action::CommitAccepted(_) => {
                            return Err(invalid("unexpected outgoing HL7 outcome"));
                        }
                    }
                }
                if let Some(error) = write_error.take() { return Err(error.into()); }
            }
            input_evidence.clear();
            if let Some(query) = &mut queries {
                tokio::select! {biased; ()=super::stopping(&mut stop)=>break,result=query.idle(store,machine,conn,&mut dialog,start)=>result?,}
            }
            wake = dialog.next_deadline();
        }
        Ok::<_, Error>(())
    }
    .await;
    // A cancelled or failed host operation must expose partial input as well.
    for action in dialog.step(start.elapsed(), Event::Disconnect)? {
        if let Action::Rejected(error, bytes) = action {
            evidence::park_bytes(store, machine, bytes, &error.to_string()).await?;
        }
    }
    if enabled && !input_evidence.is_empty() {
        evidence::park_bytes(
            store,
            machine,
            input_evidence,
            "HL7 input processing interrupted",
        )
        .await?;
    }
    if let Some(query) = &mut queries {
        query
            .close(
                store,
                machine,
                if result.is_err() {
                    "HL7-DSR-session-error"
                } else {
                    "HL7-DSR-disconnect"
                },
            )
            .await?;
    }
    result
}

// Compatibility: a failed transport attempt advances the dialog with empty input.
// It does not imply EOF or durable acceptance of a message.
fn transport_failure_as_empty_input() -> Vec<u8> {
    Vec::new()
}
