//! Reads transport outcomes before bytes enter protocol custody.
//! ASTM commits frames with transfer identity; HL7 commits complete messages and
//! interpretation; LAW commits raw results and interpretation. POCT, VIDAS and
//! Autolyser additionally journal chunks in the shared driver before dialog input.
//! Their positive steps remain guarded by their own complete receipt/transfer/poll
//! commits (ADR 0010). Cancelling a receive does not cancel queued store work.
use crate::intake::ByteReason;
use crate::machine::link::error::Error;
use analyte_transport::{Connection, Event, Reason};

pub(crate) enum Input {
    Opened { session: u64 },
    Bytes { bytes: Vec<u8> },
    Tick,
    Closed(Option<Reason>),
    Failed(Reason),
}

/// Waits for the next event a protocol host must handle.
pub(crate) async fn receive(conn: &mut Connection) -> Result<Input, analyte_transport::Error> {
    loop {
        if let Some(input) = host_input(conn.receive().await?) {
            return Ok(input);
        }
    }
}

/// The recorder is an optional tap (ADR 0009): when it stops, the transport keeps the
/// session, so its failure is logged without payload and never reaches a protocol host.
fn host_input(event: Option<Event>) -> Option<Input> {
    Some(match event {
        Some(Event::Opened { session }) => Input::Opened { session },
        Some(Event::Received { bytes, .. }) => Input::Bytes { bytes },
        Some(Event::Closed(reason)) => Input::Closed(Some(reason)),
        Some(Event::Failed(reason)) => Input::Failed(reason),
        Some(Event::RecordingFailed(failure)) => {
            tracing::warn!(error_kind = %failure, "optional recorder stopped; transport continues");
            return None;
        }
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
    /// Feeds the next byte while retaining the original read for interruption evidence.
    pub(crate) fn next(&mut self) -> Option<u8> {
        let byte = self.bytes.get(self.fed).copied()?;
        self.fed();
        Some(byte)
    }
    /// LAW returns the just-fed byte when it first closes the preceding envelope.
    #[expect(
        clippy::expect_used,
        reason = "LAW Unconsumed returns the byte just fed through next"
    )]
    pub(crate) fn replay_last(&mut self) {
        self.fed = self
            .fed
            .checked_sub(1)
            .expect("Unconsumed follows a fed byte");
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

#[cfg(test)]
mod tests {
    use super::*;
    use analyte_transport::recording::Failure;

    #[test]
    fn adr0003_law_unconsumed_byte_replays_before_untouched_tail() {
        let mut input = Custody::default();
        input.begin(b"abc");
        assert_eq!(input.next(), Some(b'a'));
        input.replay_last();
        assert_eq!(input.next(), Some(b'a'));
        assert_eq!(input.fed, 1);
        assert_eq!(input.next(), Some(b'b'));
        assert_eq!(input.next(), Some(b'c'));
        assert_eq!(input.next(), None);
    }

    #[test]
    fn adr0009_recorder_failure_never_reaches_a_protocol_host() {
        for failure in [Failure::QueueFull, Failure::Closed, Failure::Oversized] {
            assert!(host_input(Some(Event::RecordingFailed(failure))).is_none());
        }
        assert!(matches!(
            host_input(Some(Event::Opened { session: 7 })),
            Some(Input::Opened { session: 7 })
        ));
        assert!(matches!(host_input(None), Some(Input::Closed(None))));
    }
}
