//! Shared host custody boundary, independent of vendor conversation rules.
use crate::{Error, config::Machine, evidence, invalid};
use analyte_model::ReceiptId;
use analyte_store::Handle;
use std::time::Duration;
use tokio::sync::watch;

pub(super) enum Input {
    Opened,
    Bytes(Vec<u8>),
    Tick,
    End,
}

/// Every transport chunk is durably journaled before a dialog sees any byte.
/// Dropping this wait cannot cancel an already queued store request.
pub(super) async fn input(
    machine: &evidence::Connection<'_>,
    store: &Handle,
    conn: &mut analyte_transport::Connection,
    stop: &mut watch::Receiver<bool>,
    wait: Duration,
) -> Result<Input, Error> {
    let event = tokio::select! {
        biased;
        () = super::stopping(stop) => return Ok(Input::End),
        () = tokio::time::sleep(wait) => return Ok(Input::Tick),
        event = conn.receive() => event,
    };
    match event {
        Ok(Some(analyte_transport::Event::Opened { .. })) => Ok(Input::Opened),
        Ok(Some(analyte_transport::Event::Received { bytes, .. })) => {
            evidence::receive(store, machine, ReceiptId::new(), bytes.clone()).await?;
            Ok(Input::Bytes(bytes))
        }
        Ok(Some(
            analyte_transport::Event::Closed(_)
            | analyte_transport::Event::Failed(_)
            | analyte_transport::Event::RecordingFailed(_),
        ))
        | Ok(None) => Ok(Input::End),
        Err(analyte_transport::Error::Deadline(_)) => Ok(Input::Tick),
        Err(error) => Err(error.into()),
    }
}

pub(super) async fn write(
    conn: &mut analyte_transport::Connection,
    bytes: &[u8],
    budget: Duration,
) -> Result<(), Error> {
    tokio::time::timeout(budget, async {
        for chunk in bytes.chunks(analyte_transport::CHUNK_LIMIT) {
            conn.write(chunk.to_vec()).await?;
        }
        Ok::<_, Error>(())
    })
    .await
    .map_err(|_| invalid("vendor write deadline exceeded; outcome uncertain"))?
}

pub(super) async fn failed(
    machine: &Machine,
    store: &Handle,
    receipt: ReceiptId,
    reason: &str,
) -> Result<(), Error> {
    evidence::failed_reading(store, machine, receipt, reason).await
}

pub(super) fn ms(value: u32) -> Duration {
    Duration::from_millis(u64::from(value))
}
