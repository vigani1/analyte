//! Reads transport outcomes before bytes enter protocol custody.
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

use crate::{Error, intake};
use analyte_model::ReceiptId;
use analyte_store::Handle;
use std::time::Duration;
use tokio::sync::watch;

/// Every transport chunk is durably journaled before a dialog sees any byte.
/// Dropping this wait cannot cancel an already queued store request.
pub(super) async fn journaled(
    machine: &intake::Connection<'_>,
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
pub(super) struct Custody {
    bytes: Vec<u8>,
    pub(super) fed: usize,
}
impl Custody {
    pub(super) fn begin(&mut self, bytes: &[u8]) {
        self.bytes = bytes.to_vec();
        self.fed = 0;
    }
    pub(super) fn fed(&mut self) {
        self.fed = self.fed.saturating_add(1);
    }
    pub(super) fn complete(&mut self) {
        self.bytes.clear();
        self.fed = 0;
    }
    pub(super) async fn retain(
        &mut self,
        store: &Handle,
        machine: &intake::Connection<'_>,
    ) -> Result<(), Error> {
        let Some((prefix, tail)) = self.bytes.split_at_checked(self.fed) else {
            intake::park_bytes(
                store,
                machine,
                self.bytes.clone(),
                "Interrupted input: ownership uncertain; possible duplicates",
            )
            .await?;
            self.complete();
            return Ok(());
        };
        for (bytes, reason) in [
            (
                prefix,
                "Interrupted input: fed prefix; possible duplicate of dialog receipts",
            ),
            (tail, "Interrupted input: untouched transport tail"),
        ] {
            if !bytes.is_empty() {
                intake::park_bytes(store, machine, bytes.to_vec(), reason).await?;
            }
        }
        self.complete();
        Ok(())
    }
}
