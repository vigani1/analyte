//! ASTM journaling precedes every data ACK (ADR 0003).
use super::connection_input::{self, Input};
use crate::orders::astm::{Pending, Request};
use crate::{Error, deadline, evidence, invalid};
use analyte_dialog::astm::{Action, Config, Dialog, End, Epoch, Event, Role, Timers};
use analyte_model::{Identifier, ReceiptId};
use analyte_store::Handle;
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};
use tokio::sync::watch;

pub(super) async fn session(
    machine: &evidence::Connection<'_>,
    settings: &analyte_config::AstmStack,
    store: &Handle,
    conn: &mut analyte_transport::Connection,
    mut stop: watch::Receiver<bool>,
) -> Result<(), Error> {
    let timers = Timers {
        establish: Duration::from_millis(u64::from(settings.timeouts.send_timeout_ms)),
        frame: Duration::from_millis(u64::from(settings.timeouts.send_timeout_ms)),
        receive: Duration::from_millis(u64::from(settings.timeouts.receive_timeout_ms)),
        commit: Duration::from_secs(5),
        ..Timers::default()
    };
    let config = Config::new(
        analyte_framing::astm::Limit::new(settings.frame_limit)?,
        timers,
        match settings.contention_priority {
            analyte_config::Priority::Host => Role::Instrument,
            analyte_config::Priority::Instrument => Role::Computer,
        },
        settings.syntax.limits.bytes,
        settings.syntax.limits.records,
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
    let mut pending: Option<Pending<'_>> = None;
    let mut response: Option<crate::orders::astm::Reply> = None;
    let mut response_deadline = None;
    let mut query_started = None;
    let mut active_orders = Vec::new();
    let mut order_cursor = String::new();
    let mut next_order_poll = Instant::now();
    let result = async {
        loop {
            let event = tokio::select! {
                biased;
                () = super::stopping(&mut stop) => break,
                () = std::future::ready(()), if response.is_some() && wake.is_none() => None,
                () = tokio::time::sleep(wake.map_or(Duration::from_secs(1), |d: Duration| d.saturating_sub(start.elapsed()))) => None,
                event = connection_input::receive(conn) => match event {
                    Ok(event) => Some(event),
                    Err(analyte_transport::Error::Deadline(_)) => None,
                    Err(error) => return Err(error.into()),
                },
                answer = async { match pending.as_mut() { Some(query) => query.finish(machine).await, None => std::future::pending().await } }, if pending.is_some() => {
                    response_deadline = pending.as_ref().map(Pending::deadline);
                    pending = None;
                    response = answer?;
                    None
                },
            };
            let bytes = match event {
                Some(Input::Opened { session }) => {
                    tracing::info!(machine = machine.id.as_str(), session, "ASTM connection opened");
                    dialog = Dialog::new(config.clone(), Epoch(uuid::Uuid::now_v7().as_u128()));
                    Vec::new()
                }
                Some(Input::Bytes { bytes }) => bytes,
                Some(Input::Closed(_)) => break,
                Some(Input::Failed(_) | Input::RecordingFailed(_)) | Some(Input::Tick) | None => Vec::new(),
            };
            let outgoing = if bytes.is_empty() && wake.is_none() && response.is_none() && pending.is_none() && Instant::now() >= next_order_poll {
                next_order_poll = Instant::now().checked_add(Duration::from_secs(1)).ok_or_else(||invalid("order poll deadline overflow"))?;
                crate::orders::sending::next(store,machine,&mut order_cursor).await?
            } else { None };
            let events = if let Some(reserved) = outgoing {
                active_orders.push(reserved.order);
                vec![Event::Send(crate::orders::astm::envelope(vec![reserved.payload.record()?.to_vec()], b'N'))]
            } else if bytes.is_empty() && response.is_some() && wake.is_none() {
                if response_deadline.is_some_and(|until| Instant::now() >= until) { response = crate::orders::astm::failure(machine); }
                match response.take() {
                    Some(reply) => match crate::orders::sending::reserve_query(store,machine,reply,response_deadline.unwrap_or_else(Instant::now),&mut active_orders).await? {
                        Some(records) => vec![Event::Send(records)], None => vec![Event::Tick]
                    }, None => vec![Event::Tick]
                }
            } else if bytes.is_empty() { vec![Event::Tick] } else { bytes.into_iter().map(Event::Byte).collect() };
            for event in events {
                if let Event::Byte(byte) = &event {
                    if *byte == analyte_framing::astm::STX { partial.clear(); }
                    if *byte == analyte_framing::astm::STX || !partial.is_empty() {
                        if partial.len() > settings.frame_limit { return Err(invalid("ASTM partial frame bound exceeded")); }
                        partial.push(*byte);
                    }
                }
                let mut actions = VecDeque::new();
                if matches!(event, Event::Byte(analyte_framing::astm::ENQ)) {
                    // Start the query budget before admission and journaling.
                    // A new transfer supersedes a pending lookup, so no cached
                    // order response can wait behind another machine message.
                    query_started = Some(Instant::now());
                    if pending.is_some() || response.is_some() {
                        pending = None;
                        response = crate::orders::astm::failure(machine);
                        response_deadline = None;
                    }
                    let expected = settings.frame_limit.checked_add(machine.snapshot.canonical().len()).ok_or_else(|| invalid("ASTM admission size overflow"))?;
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
                        Action::Commit(id, number, bytes) => {
                            let transfer_id = Identifier::new(&format!("{:032x}-{}", id.epoch().0, id.transfer()))?;
                            transfer = Some(transfer_id.clone());
                            if raw.len().checked_add(bytes.len()).is_none_or(|n| n > settings.syntax.limits.bytes) { return Err(invalid("ASTM transfer wire-byte bound exceeded")); }
                            raw.extend_from_slice(&bytes);
                            let provenance = analyte_store::FrameProvenance { machine: machine.id.clone(), driver: evidence::driver(machine)?, config: machine.snapshot.canonical().as_bytes().to_vec() };
                            let sequence = analyte_store::FrameSequence::new(number.value(), analyte_framing::astm::FrameNumber::MAX.value())?;
                            let committed = store.request(deadline(), move |s| s.journal_frame(&transfer_id, sequence, &bytes, &provenance)).await;
                            if let Err(error) = &committed { tracing::error!(machine = machine.id.as_str(), error_kind = %error, "ASTM frame commit refused"); }
                            #[cfg(feature = "test-hooks")]
                            if committed.is_ok() && let Some(hook) = &machine.astm_committed {
                                hook.pause(&mut stop).await?;
                            }
                            actions.extend(dialog.step(start.elapsed(), if committed.is_ok() { Event::Committed(id) } else { Event::CommitFailed(id) })?);
                        }
                        Action::Record(record) => {
                            if record.first() == Some(&b'Q') && query_started.is_none() { query_started = Some(Instant::now()); }
                            if payload.len().checked_add(record.len()).is_none_or(|n| n > settings.syntax.limits.bytes) { return Err(invalid("ASTM message bound exceeded")); }
                            payload.extend_from_slice(&record);
                        }
                        Action::Ended(end) => {
                            if let Some(transfer_id) = transfer.take() {
                                if end == End::Normal {
                                    let id = ReceiptId::new();
                                    evidence::receive(store, machine, id, std::mem::take(&mut raw)).await?;
                                    match crate::orders::astm::request(machine,&payload)? {
                                        None => { evidence::interpret(store, machine, id).await?; },
                                        Some(Request::Cancel) => { pending = None; response = None; response_deadline = None; },
                                        Some(Request::Unsupported) => { pending = None; response = crate::orders::astm::failure(machine); response_deadline = None; },
                                        Some(Request::Orders(samples)) => {
                                            if pending.is_some() || response.is_some() {
                                                pending = None; response = crate::orders::astm::failure(machine); response_deadline = None;
                                            } else {
                                                pending = Some(Pending::new(store,machine,samples,query_started.unwrap_or_else(Instant::now))?);
                                            }
                                        },
                                    }
                                    store.request(deadline(), move |s| s.finish_transfer(&transfer_id, analyte_store::TransferOutcome::Complete(id))).await?;
                                } else {
                                    finish_incomplete(store, transfer_id).await?;
                                }
                            }
                            raw.clear(); payload.clear(); query_started = None;
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
                        Action::Sent(end) => {
                            for order in active_orders.drain(..) {
                                crate::orders::sending::finished(store,&order,end).await?;
                            }
                        },
                    }
                }
            }
        }
        Ok(())
    }.await;
    for order in active_orders.drain(..) {
        crate::orders::sending::uncertain(
            store,
            &order,
            "connection-ended-before-transfer-confirmation",
        )
        .await?;
    }
    if let Some(transfer) = transfer {
        finish_incomplete(store, transfer).await?;
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

async fn finish_incomplete(store: &Handle, transfer: Identifier) -> Result<(), Error> {
    let reason =
        analyte_model::Reason::new("ASTM transfer ended before a complete receipt was retained")?;
    store
        .request(deadline(), move |store| {
            store.finish_transfer(
                &transfer,
                analyte_store::TransferOutcome::Incomplete(reason),
            )
        })
        .await?;
    Ok(())
}
