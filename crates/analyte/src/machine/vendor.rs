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
    let until = tokio::time::Instant::now()
        .checked_add(budget)
        .ok_or_else(|| invalid("vendor write deadline overflow"))?;
    conn.write_all(bytes, Some(until)).await.map_err(|error| {
        if matches!(error, analyte_transport::Error::Deadline("write all")) {
            invalid("vendor write deadline exceeded; outcome uncertain")
        } else {
            error.into()
        }
    })
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

/// Retains both the triggering failure and a failure to save its dialog effects.
#[derive(Debug, thiserror::Error)]
#[error("{original}; dialog effect failed: {effect}")]
struct EffectFailure {
    #[source]
    original: Error,
    effect: Error,
}

pub(super) fn effect_failure(original: Option<Error>, effect: Error) -> Error {
    match original {
        Some(original) => Box::new(EffectFailure { original, effect }),
        None => effect,
    }
}
