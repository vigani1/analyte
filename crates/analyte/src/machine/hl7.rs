//! HL7 store-and-forward ACK decisions follow durable interpretation.
use crate::{Error, config::Machine, evidence, invalid};
use analyte_dialog::hl7::{Action, Config, Dialog, Event};
use analyte_model::ReceiptId;
use analyte_store::Handle;
use std::{
    collections::{BTreeMap, VecDeque},
    time::{Duration, Instant},
};
use tokio::sync::watch;

pub(super) async fn session(
    machine: &Machine,
    store: &Handle,
    conn: &mut analyte_transport::Connection,
    mut stop: watch::Receiver<bool>,
) -> Result<(), Error> {
    let mut options = machine
        .snapshot
        .effective()
        .hl7_options()
        .ok_or_else(|| invalid("HL7 options absent"))?;
    options.limits.message_bytes = options
        .limits
        .message_bytes
        .min(machine.snapshot.effective().machine().framing.limit);
    let config = Config::new(
        options,
        Duration::from_millis(u64::from(
            machine
                .snapshot
                .effective()
                .machine()
                .dialog
                .receive_timeout_ms,
        )),
        1,
    )?;
    let mut dialog = Dialog::new(config, 0);
    let start = Instant::now();
    let mut wake = None;
    let mut pending = BTreeMap::new();
    loop {
        let event = tokio::select! {
            biased;
            () = super::stopping(&mut stop) => break,
            () = tokio::time::sleep(wake.map_or(Duration::from_secs(1), |d: Duration| d.saturating_sub(start.elapsed()))) => None,
            event = conn.receive() => match event {
                Ok(Some(event)) => Some(event),
                Ok(None) => break,
                Err(analyte_transport::Error::Deadline(_)) => None,
                Err(error) => return Err(error.into()),
            },
        };
        let bytes = match event {
            Some(analyte_transport::Event::Opened { session }) => {
                tracing::info!(
                    machine = machine.id.as_str(),
                    session,
                    "HL7 connection opened"
                );
                Vec::new()
            }
            Some(analyte_transport::Event::Received { bytes, .. }) => bytes,
            Some(analyte_transport::Event::Closed(_)) => break,
            Some(
                analyte_transport::Event::Failed(_) | analyte_transport::Event::RecordingFailed(_),
            )
            | None => Vec::new(),
        };
        let events = if bytes.is_empty() {
            vec![Event::Tick]
        } else {
            bytes.into_iter().map(Event::Byte).collect()
        };
        for event in events {
            let mut actions: VecDeque<_> = dialog.step(start.elapsed(), event)?.into();
            while let Some(action) = actions.pop_front() {
                match action {
                    Action::Commit(token, raw) => {
                        let id = ReceiptId::new();
                        match evidence::receive(store, machine, id, raw).await {
                            Ok(time) => {
                                if pending.len() >= 2 {
                                    return Err(invalid("HL7 pending receipt limit exceeded"));
                                }
                                pending.insert((token.epoch(), token.ordinal()), (id, time));
                                actions
                                    .extend(dialog.step(start.elapsed(), Event::Committed(token))?);
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
                    Action::Interpret(token, payload) => {
                        let (id, time) = pending
                            .remove(&(token.epoch(), token.ordinal()))
                            .ok_or_else(|| invalid("HL7 receipt correlation missing"))?;
                        let success =
                            evidence::interpret(store, machine, id, time, &payload).await?;
                        let event = if success {
                            Event::Interpreted(token)
                        } else {
                            Event::NotInterpretable(
                                token,
                                analyte_hl7::AckError {
                                    segment: b"OBX",
                                    occurrence: 1,
                                    field: 1,
                                    code: b"207",
                                    application_code: b"operator-review",
                                },
                                false,
                            )
                        };
                        actions.extend(dialog.step(start.elapsed(), event)?);
                    }
                    Action::ResponseNeeded(token, _) => {
                        let id = uuid::Uuid::now_v7().simple().to_string();
                        let id = id.get(12..).ok_or_else(|| invalid("response ID bound"))?;
                        let timestamp = jiff::Timestamp::now()
                            .strftime("%Y%m%d%H%M%S%z")
                            .to_string();
                        actions.extend(dialog.step(
                            start.elapsed(),
                            Event::Respond(
                                token,
                                analyte_hl7::Response {
                                    control_id: analyte_hl7::ControlId::new(id.as_bytes())?,
                                    timestamp: timestamp.as_bytes(),
                                },
                            ),
                        )?);
                    }
                    Action::Write(bytes) => {
                        for chunk in bytes.chunks(analyte_transport::CHUNK_LIMIT) {
                            conn.write(chunk.to_vec()).await?;
                        }
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
                    Action::Sent(_, _) => return Err(invalid("unexpected outgoing HL7 outcome")),
                }
            }
        }
    }
    // Disconnect exposes any partial block as protected rejection evidence.
    for action in dialog.step(start.elapsed(), Event::Disconnect)? {
        if let Action::Rejected(error, bytes) = action {
            evidence::park_bytes(store, machine, bytes, &error.to_string()).await?;
        }
    }
    Ok(())
}
