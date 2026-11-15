//! Transport outcomes before protocol custody. Receiving bytes here promises no ACK.
//! ASTM commits frames with transfer identity; HL7 commits complete messages and
//! interpretation; LAW commits raw results and interpretation. POCT, VIDAS and
//! Autolyser additionally journal chunks before dialog input via `journaled`.
//! Their positive steps remain guarded by their own complete receipt/transfer/poll
//! commits (ADR 0010). Cancelling a receive does not cancel queued store work.
use analyte_transport::{Connection, Event, Reason, recording};

pub(super) enum Input {
    Opened { session: u64 },
    Bytes { bytes: Vec<u8> },
    Tick,
    Closed(Option<Reason>),
    Failed(Reason),
    RecordingFailed(recording::Failure),
}

pub(super) async fn receive(conn: &mut Connection) -> Result<Input, analyte_transport::Error> {
    Ok(match conn.receive().await? {
        Some(Event::Opened { session }) => Input::Opened { session },
        Some(Event::Received { bytes, .. }) => Input::Bytes { bytes },
        Some(Event::Closed(reason)) => Input::Closed(Some(reason)),
        Some(Event::Failed(reason)) => Input::Failed(reason),
        Some(Event::RecordingFailed(reason)) => Input::RecordingFailed(reason),
        None => Input::Closed(None),
    })
}

use crate::{Error, evidence};
use analyte_model::ReceiptId;
use analyte_store::Handle;
use std::time::Duration;
use tokio::sync::watch;

/// Every transport chunk is durably journaled before a dialog sees any byte.
/// Dropping this wait cannot cancel an already queued store request.
pub(super) async fn journaled(
    machine: &evidence::Connection<'_>,
    store: &Handle,
    conn: &mut Connection,
    stop: &mut watch::Receiver<bool>,
    wait: Duration,
) -> Result<Input, Error> {
    let event = tokio::select! {
        biased;
        () = super::stopping(stop) => return Ok(Input::Closed(None)),
        () = tokio::time::sleep(wait) => return Ok(Input::Tick),
        event = receive(conn) => event,
    };
    match event {
        Ok(Input::Bytes { bytes }) => {
            evidence::receive(store, machine, ReceiptId::new(), bytes.clone()).await?;
            Ok(Input::Bytes { bytes })
        }
        Ok(event) => Ok(event),
        Err(analyte_transport::Error::Deadline(_)) => Ok(Input::Tick),
        Err(error) => Err(error.into()),
    }
}
