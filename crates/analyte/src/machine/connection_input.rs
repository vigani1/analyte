//! Transport outcomes before protocol custody. Receiving bytes here promises no ACK.
//! ASTM commits frames with transfer identity; HL7 commits complete messages and
//! interpretation; LAW commits raw results and interpretation. POCT, VIDAS and
//! Autolyser additionally journal chunks before dialog input via `custody::input`.
//! Their positive steps remain guarded by their own complete receipt/transfer/poll
//! commits (ADR 0010). Cancelling a receive does not cancel queued store work.
use analyte_transport::{Connection, Error, Event, Reason, recording};

pub(super) enum Input {
    Opened { session: u64 },
    Bytes { bytes: Vec<u8> },
    Tick,
    Closed(Option<Reason>),
    Failed(Reason),
    RecordingFailed(recording::Failure),
}

pub(super) async fn receive(conn: &mut Connection) -> Result<Input, Error> {
    Ok(match conn.receive().await? {
        Some(Event::Opened { session }) => Input::Opened { session },
        Some(Event::Received { bytes, .. }) => Input::Bytes { bytes },
        Some(Event::Closed(reason)) => Input::Closed(Some(reason)),
        Some(Event::Failed(reason)) => Input::Failed(reason),
        Some(Event::RecordingFailed(reason)) => Input::RecordingFailed(reason),
        None => Input::Closed(None),
    })
}
