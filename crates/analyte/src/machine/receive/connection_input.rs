//! Reads transport outcomes before bytes enter protocol custody.
//! ASTM commits frames with transfer identity; HL7 commits complete messages and
//! interpretation; LAW commits raw results and interpretation. POCT, VIDAS and
//! Autolyser additionally journal chunks in the shared driver before dialog input.
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
use analyte_store::Handle;

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
        machine: &intake::ConnectionEvidence<'_>,
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
