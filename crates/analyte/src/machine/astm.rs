//! ASTM journaling precedes every data ACK (ADR 0003).
use crate::{Error, config::Machine, deadline, evidence, invalid};
use analyte_dialog::astm::{Action, Config, Dialog, End, Epoch, Event, Role, Timers};
use analyte_model::{Identifier, ReceiptId};
use analyte_store::Handle;
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};
use tokio::sync::watch;

pub(super) async fn session(
    machine: &Machine,
    store: &Handle,
    conn: &mut analyte_transport::Connection,
    mut stop: watch::Receiver<bool>,
) -> Result<(), Error> {
    let settings = machine.snapshot.effective().machine();
    let timers = Timers {
        establish: Duration::from_millis(u64::from(settings.dialog.send_timeout_ms)),
        frame: Duration::from_millis(u64::from(settings.dialog.send_timeout_ms)),
        receive: Duration::from_millis(u64::from(settings.dialog.receive_timeout_ms)),
        commit: Duration::from_secs(5),
        ..Timers::default()
    };
    let config = Config::new(
        analyte_framing::astm::Limit::new(settings.framing.limit)?,
        timers,
        match settings.dialog.contention_priority {
            Some(analyte_config::Priority::Host) => Role::Instrument,
            Some(analyte_config::Priority::Instrument) | None => Role::Computer,
        },
        settings.syntax.bytes,
        settings.syntax.records,
        true,
    )?;
    let epoch = Epoch(uuid::Uuid::now_v7().as_u128());
    let mut dialog = Dialog::new(config.clone(), epoch);
    let start = Instant::now();
    let mut wake = None;
    let mut transfer: Option<Identifier> = None;
    let mut raw = Vec::new();
    let mut payload = Vec::new();
    let mut partial = Vec::new();
    let result = async {
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
                    tracing::info!(machine = machine.id.as_str(), session, "ASTM connection opened");
                    dialog = Dialog::new(config.clone(), Epoch(uuid::Uuid::now_v7().as_u128()));
                    Vec::new()
                }
                Some(analyte_transport::Event::Received { bytes, .. }) => bytes,
                Some(analyte_transport::Event::Closed(_)) => break,
                Some(analyte_transport::Event::Failed(_) | analyte_transport::Event::RecordingFailed(_)) | None => Vec::new(),
            };
            let events = if bytes.is_empty() { vec![Event::Tick] } else { bytes.into_iter().map(Event::Byte).collect() };
            for event in events {
                if let Event::Byte(byte) = &event {
                    if *byte == analyte_framing::astm::STX { partial.clear(); }
                    if *byte == analyte_framing::astm::STX || !partial.is_empty() {
                        if partial.len() > settings.framing.limit { return Err(invalid("ASTM partial frame bound exceeded")); }
                        partial.push(*byte);
                    }
                }
                let mut actions = VecDeque::new();
                if matches!(event, Event::Byte(analyte_framing::astm::ENQ)) {
                    let expected = settings.framing.limit.checked_add(machine.snapshot.canonical().len()).ok_or_else(|| invalid("ASTM admission size overflow"))?;
                    let capacity = store.request(deadline(), move |s| s.admission(expected)).await;
                    if let Err(error) = &capacity { tracing::error!(machine = machine.id.as_str(), error_kind = %error, "storage admission refused"); }
                    actions.extend(dialog.step(start.elapsed(), Event::Capacity(capacity.is_ok()))?);
                }
                actions.extend(dialog.step(start.elapsed(), event)?);
                while let Some(action) = actions.pop_front() {
                    match action {
                        Action::Write(bytes) => {
                            if bytes.as_slice() == [analyte_framing::astm::ACK] { partial.clear(); }
                            conn.write(bytes).await?;
                        }
                        Action::Commit(id, bytes) => {
                            let transfer_id = Identifier::new(&format!("{:032x}-{}", id.epoch().0, id.transfer()))?;
                            transfer = Some(transfer_id.clone());
                            let number = bytes.get(1).and_then(|b| b.checked_sub(b'0')).ok_or_else(|| invalid("ASTM frame number missing"))?;
                            if raw.len().checked_add(bytes.len()).is_none_or(|n| n > settings.syntax.bytes) { return Err(invalid("ASTM transfer wire-byte bound exceeded")); }
                            raw.extend_from_slice(&bytes);
                            let provenance = analyte_store::FrameProvenance { machine: machine.id.clone(), driver: evidence::driver(machine)?, config: machine.snapshot.canonical().as_bytes().to_vec() };
                            let committed = store.request(deadline(), move |s| s.journal_frame(&transfer_id, number, &bytes, &provenance)).await;
                            if let Err(error) = &committed { tracing::error!(machine = machine.id.as_str(), error_kind = %error, "ASTM frame commit refused"); }
                            actions.extend(dialog.step(start.elapsed(), if committed.is_ok() { Event::Committed(id) } else { Event::CommitFailed(id) })?);
                        }
                        Action::Record(record) => {
                            if payload.len().checked_add(record.len()).is_none_or(|n| n > settings.syntax.bytes) { return Err(invalid("ASTM message bound exceeded")); }
                            payload.extend_from_slice(&record);
                        }
                        Action::Ended(end) => {
                            if let Some(transfer_id) = transfer.take() {
                                if end == End::Normal {
                                    let id = ReceiptId::new();
                                    let time = evidence::receive(store, machine, id, std::mem::take(&mut raw)).await?;
                                    evidence::interpret(store, machine, id, time, &payload).await?;
                                    store.request(deadline(), move |s| s.finish_transfer(&transfer_id, Some(id))).await?;
                                } else {
                                    store.request(deadline(), move |s| s.finish_transfer(&transfer_id, None)).await?;
                                }
                            }
                            raw.clear(); payload.clear();
                        }
                        Action::Deadline(next) => wake = next,
                        Action::Refused(error) => return Err(error.into()),
                        Action::Conflict(bytes) => evidence::park_bytes(store, machine, bytes, "same ASTM frame number carried conflicting bytes").await?,
                        Action::UnexpectedFrame(bytes) => evidence::park_bytes(store, machine, bytes, "unsolicited ASTM frame").await?,
                        Action::Rejected(error, bytes) => {
                            if partial.len() != 1 { partial.clear(); }
                            evidence::park_bytes(store, machine, bytes, &error.to_string()).await?;
                        }
                        Action::CommitFailed(_) | Action::LimitReached => return Err(invalid("ASTM storage or resource limit refused transfer")),
                        Action::Sent(_) => return Err(invalid("receive service received an unexpected sender outcome")),
                    }
                }
            }
        }
        Ok(())
    }.await;
    if let Some(transfer) = transfer {
        store
            .request(deadline(), move |s| s.finish_transfer(&transfer, None))
            .await?;
    }
    if !partial.is_empty() {
        evidence::park_bytes(
            store,
            machine,
            partial,
            "incomplete ASTM frame when session stopped",
        )
        .await?;
    }
    result
}
