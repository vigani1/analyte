//! POCT01-A2 DML reviewer with durable custody before every acceptance.
use super::connection_input::Input;
use super::custody::{self, ms};
use crate::{Error, evidence, invalid};
use analyte_dialog::poct::{CommitOutcome, Dialog, DialogAction, DialogConfig, DialogEvent};
use analyte_model::ReceiptId;
use analyte_poct::{Body, ControlId, Header, Message, Timestamp};
use analyte_store::Handle;
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};
use tokio::sync::watch;

pub(super) async fn session(
    machine: &evidence::Connection<'_>,
    settings: &analyte_config::PoctStack,
    store: &Handle,
    conn: &mut analyte_transport::Connection,
    mut stop: watch::Receiver<bool>,
) -> Result<(), Error> {
    let policy = &settings.policy();
    let limits = super::interpretation::poct_limits(settings);
    let mut dialog = Dialog::new(
        DialogConfig {
            pending_messages: policy.pending_messages,
            message_bytes: limits.message_bytes,
            response_timeout: ms(policy.response_ms),
            unannounced_ack_timeout: ms(policy.acknowledgment_ms),
        },
        ControlId::new(&uuid::Uuid::now_v7().simple().to_string())?,
    )?;
    let start = Instant::now();
    let mut raw = Vec::new();
    let mut framer = analyte_framing::poct::Decoder::new(limits.message_bytes, policy.depth);
    loop {
        let input = super::connection_input::journaled(
            machine,
            store,
            conn,
            &mut stop,
            Duration::from_millis(10),
        )
        .await?;
        // Incoming fragments must not postpone the absolute dialog deadline.
        let actions = dialog.step(start.elapsed(), DialogEvent::Tick)?;
        if process(actions, &mut dialog, start, machine, store, conn, settings).await? {
            break;
        }
        match input {
            Input::Opened { .. } => {}
            Input::Tick => {
                let actions = dialog.step(start.elapsed(), DialogEvent::Tick)?;
                if process(actions, &mut dialog, start, machine, store, conn, settings).await? {
                    break;
                }
            }
            Input::Closed(_) | Input::Failed(_) | Input::RecordingFailed(_) => {
                let actions = dialog.step(start.elapsed(), DialogEvent::Disconnected)?;
                process(actions, &mut dialog, start, machine, store, conn, settings).await?;
                break;
            }
            Input::Bytes { bytes } => {
                for byte in bytes {
                    if raw.len() >= limits.message_bytes {
                        evidence::park_bytes(
                            store,
                            machine,
                            std::mem::take(&mut raw),
                            "POCT XML message byte limit exceeded",
                        )
                        .await?;
                        return Err(invalid("POCT message limit exceeded"));
                    }
                    raw.push(byte);
                    if *stop.borrow() {
                        evidence::park_bytes(
                            store,
                            machine,
                            std::mem::take(&mut raw),
                            "POCT partial XML on cancellation",
                        )
                        .await?;
                        return Ok(());
                    }
                    if byte != b'>' {
                        continue;
                    }
                    let actions = dialog.step(start.elapsed(), DialogEvent::Tick)?;
                    if process(actions, &mut dialog, start, machine, store, conn, settings).await? {
                        evidence::park_bytes(
                            store,
                            machine,
                            std::mem::take(&mut raw),
                            "POCT partial XML exceeded dialog deadline",
                        )
                        .await?;
                        return Ok(());
                    }
                    let length = match framer.scan(&raw) {
                        Ok(Some(length)) => length,
                        Ok(None) => continue,
                        // Parse the retained bytes through the syntax boundary to
                        // classify a framing failure and recover a Hello header.
                        Err(_) => raw.len(),
                    };
                    let wire = raw
                        .get(..length)
                        .ok_or_else(|| invalid("POCT framing length"))?
                        .to_vec();
                    raw.drain(..length);
                    framer.reset();
                    let message = match Message::parse(&wire, limits) {
                        Ok(message) => message,
                        Err(error) => {
                            let reference = analyte_poct::hello_error_reference(&wire, limits);
                            let ack_error = error.acknowledgement_error();
                            evidence::park_bytes(
                                store,
                                machine,
                                wire,
                                &format!("POCT syntax: {error}"),
                            )
                            .await?;
                            if let Some(reference) = reference {
                                let actions = dialog.step(
                                    start.elapsed(),
                                    DialogEvent::InvalidHello {
                                        reference,
                                        error: ack_error,
                                    },
                                )?;
                                if process(
                                    actions,
                                    &mut dialog,
                                    start,
                                    machine,
                                    store,
                                    conn,
                                    settings,
                                )
                                .await?
                                {
                                    return Ok(());
                                }
                                continue;
                            }
                            return Err(error.into());
                        }
                    };
                    let actions =
                        dialog.step(start.elapsed(), DialogEvent::Received(Box::new(message)))?;
                    if process(actions, &mut dialog, start, machine, store, conn, settings).await? {
                        return Ok(());
                    }
                }
            }
        }
    }
    if !raw.is_empty() {
        evidence::park_bytes(
            store,
            machine,
            raw,
            "POCT incomplete XML on timeout or disconnect",
        )
        .await?;
    }
    Ok(())
}

async fn process(
    actions: Vec<DialogAction>,
    dialog: &mut Dialog,
    start: Instant,
    machine: &evidence::Connection<'_>,
    store: &Handle,
    conn: &mut analyte_transport::Connection,
    settings: &analyte_config::PoctStack,
) -> Result<bool, Error> {
    let limits = super::interpretation::poct_limits(settings);
    let send_timeout_ms = settings.timeouts().send_timeout_ms();
    let mut actions: VecDeque<_> = actions.into();
    while let Some(action) = actions.pop_front() {
        match action {
            DialogAction::InspectHello {
                device_id: _,
                capabilities: _,
            } => {
                actions.extend(dialog.step(start.elapsed(), DialogEvent::HelloDecision(true))?);
            }
            DialogAction::Store { ticket, message } => {
                let id = ReceiptId::new();
                let operation = async {
                    evidence::receive(store, machine, id, message.document().raw().to_vec())
                        .await?;
                    match message.body() {
                        Body::PatientObservations(_) | Body::NonPatientObservations(_) => {
                            let receipt = store
                                .request(crate::deadline(), move |s| {
                                    s.receipt(id)?.ok_or(analyte_store::Error::UnknownReceipt)
                                })
                                .await?;
                            match super::interpretation::map_retained(store, machine, &receipt)
                                .await
                            {
                                Ok(reading) => {
                                    evidence::vendor_reading(store, machine, id, reading).await?;
                                }
                                Err(error) => {
                                    evidence::failed_reading(
                                        store,
                                        machine,
                                        id,
                                        &format!("POCT interpretation: {error}"),
                                    )
                                    .await?;
                                    return Ok(CommitOutcome::ApplicationError(
                                        analyte_poct::ApplicationError::UnsupportedValue,
                                    ));
                                }
                            }
                        }
                        Body::Hello { .. } => {
                            // Mark the complete, parsed Hello as connection context. Transport
                            // journal fragments are also receipts, but are not interpreted Hellos.
                            evidence::vendor_reading(
                                store,
                                machine,
                                id,
                                analyte_mapping::Output::default(),
                            )
                            .await?;
                        }
                        Body::Status { .. }
                        | Body::EndTopic(_)
                        | Body::Terminate(_)
                        | Body::Acknowledgement { .. }
                        | Body::Escape { .. }
                        | Body::Request(_) => {}
                    }
                    Ok::<_, Error>(CommitOutcome::Stored)
                };
                let outcome = match operation.await {
                    Ok(outcome) => outcome,
                    Err(error) => {
                        tracing::error!(machine = machine.id.as_str(), error_kind = %error, "POCT custody failed");
                        CommitOutcome::Failed
                    }
                };
                actions.extend(
                    dialog.step(start.elapsed(), DialogEvent::Committed { ticket, outcome })?,
                );
            }
            DialogAction::Park { message, reason } => {
                evidence::park_bytes(store, machine, message.document().raw().to_vec(), reason)
                    .await?;
            }
            DialogAction::Send { control_id, body } => {
                let mut limits = limits;
                limits.message_bytes = limits.message_bytes.min(dialog.outbound_message_limit());
                let message = analyte_poct::build(
                    &Header {
                        control_id,
                        created: Timestamp::parse(
                            &jiff::Timestamp::now()
                                .strftime("%Y-%m-%dT%H:%M:%S+00:00")
                                .to_string(),
                        )?,
                    },
                    &body,
                    limits,
                )?;
                custody::write(conn, message.document().raw(), ms(send_timeout_ms)).await?;
            }
            DialogAction::ReadEnabled(_) => {}
            DialogAction::Deferred(message) => {
                evidence::park_bytes(
                    store,
                    machine,
                    message.document().raw().to_vec(),
                    "POCT message deferred by bounded custody queue",
                )
                .await?;
                return Err(invalid("POCT backpressure violated"));
            }
            DialogAction::TopicComplete => {
                actions.extend(dialog.step(start.elapsed(), DialogEvent::Finish)?);
            }
            DialogAction::Close { normal, unresolved } => {
                if !normal || !unresolved.is_empty() {
                    evidence::park_bytes(store, machine, Vec::new(), &format!(
                        "POCT dialog closure: normal={normal}; unresolved custody tickets: {unresolved:?}; inspect retained transport journal"
                    )).await?;
                }
                return Ok(true);
            }
        }
    }
    Ok(false)
}
