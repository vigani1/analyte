//! Reads transport outcomes before bytes enter protocol custody.
//! ASTM commits frames with transfer identity; HL7 commits complete messages and
//! interpretation; LAW commits raw results and interpretation. POCT, VIDAS and
//! Autolyser additionally journal chunks before dialog input via `journaled`.
//! Their positive steps remain guarded by their own complete receipt/transfer/poll
//! commits (ADR 0010). Cancelling a receive does not cancel queued store work.
use crate::intake::ByteReason;
use crate::machine::link::error::Error;
use analyte_transport::{Connection, Event, Reason, recording};

pub(crate) enum Input {
    Opened { session: u64 },
    Bytes { bytes: Vec<u8> },
    Tick,
    Closed(Option<Reason>),
    Failed(Reason),
    RecordingFailed(recording::Failure),
}

pub(crate) async fn receive(conn: &mut Connection) -> Result<Input, analyte_transport::Error> {
    Ok(match conn.receive().await? {
        Some(Event::Opened { session }) => Input::Opened { session },
        Some(Event::Received { bytes, .. }) => Input::Bytes { bytes },
        Some(Event::Closed(reason)) => Input::Closed(Some(reason)),
        Some(Event::Failed(reason)) => Input::Failed(reason),
        Some(Event::RecordingFailed(reason)) => Input::RecordingFailed(reason),
        None => Input::Closed(None),
    })
}

use crate::intake;
use analyte_model::ReceiptId;
use analyte_store::Handle;
use std::time::Duration;
use tokio::sync::watch;

/// Every transport chunk is durably journaled before a dialog sees any byte.
/// Dropping this wait cannot cancel an already queued store request.
pub(crate) async fn journaled(
    machine: &intake::Connection<'_>,
    store: &Handle,
    conn: &mut Connection,
    stop: &mut watch::Receiver<bool>,
    wait: Option<Duration>,
) -> Result<Input, Error> {
    let event = tokio::select! {
        biased;
        () = crate::machine::stopping(stop) => return Ok(Input::Closed(None)),
        () = async { match wait {
            Some(wait) => tokio::time::sleep(wait).await,
            None => std::future::pending().await,
        } } => return Ok(Input::Tick),
        event = receive(conn) => event,
    };
    match event {
        Ok(Input::Bytes { bytes }) => {
            intake::receive(store, machine, ReceiptId::new(), bytes.clone()).await?;
            Ok(Input::Bytes { bytes })
        }
        Ok(event) => Ok(event),
        Err(analyte_transport::Error::Deadline(_)) => Ok(Input::Tick),
        Err(error) => Err(error.into()),
    }
}

/// Bounded by one transport read. Fed bytes can overlap dialog receipts after
/// interruption; the untouched tail has never entered the dialog.
#[derive(Default)]
pub(crate) struct Custody {
    bytes: Vec<u8>,
    pub(crate) fed: usize,
}
impl Custody {
    pub(crate) fn begin(&mut self, bytes: &[u8]) {
        self.bytes = bytes.to_vec();
        self.fed = 0;
    }
    pub(crate) fn fed(&mut self) {
        self.fed = self.fed.saturating_add(1);
    }
    pub(crate) fn complete(&mut self) {
        self.bytes.clear();
        self.fed = 0;
    }
    pub(crate) async fn retain(
        &mut self,
        store: &Handle,
        machine: &intake::Connection<'_>,
    ) -> Result<(), Error> {
        let Some((prefix, tail)) = self.bytes.split_at_checked(self.fed) else {
            intake::park_bytes(
                store,
                machine,
                self.bytes.clone(),
                ByteReason::InputUncertain,
            )
            .await?;
            self.complete();
            return Ok(());
        };
        for (bytes, reason) in [
            (prefix, ByteReason::InputPrefix),
            (tail, ByteReason::InputTail),
        ] {
            if !bytes.is_empty() {
                intake::park_bytes(store, machine, bytes.to_vec(), reason).await?;
            }
        }
        self.complete();
        Ok(())
    }
}
