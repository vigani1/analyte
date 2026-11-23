//! Builds alert events exclusively from operational state and persisted identities.
use crate::{AlertKey, AlertState, OperatorError as Error, Undeliverable};
use serde::Serialize;

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum EventKind {
    Raised,
    Reminder,
    Resolved,
}
impl EventKind {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Raised => "raised",
            Self::Reminder => "reminder",
            Self::Resolved => "resolved",
        }
    }
}

#[derive(Serialize)]
struct Event<'a> {
    version: u8,
    event_id: &'a str,
    condition_id: String,
    episode_id: String,
    event: EventKind,
    kind: &'static str,
    machine_id: Option<&'a str>,
    destination_id: Option<&'a str>,
    reason: Option<&'static str>,
    responder: &'a str,
    observed_at: i64,
    episode_at: i64,
    measurement: Measurement,
}
#[derive(Serialize)]
#[serde(tag = "unit", content = "value", rename_all = "snake_case")]
enum Measurement {
    Seconds(Option<u64>),
    Bytes(Option<u64>),
    Count(Option<u64>),
    Paused(Option<u64>),
}

pub(super) fn build(
    namespace: &str,
    id: &str,
    state: &AlertState,
    episode: (i64, i64),
    event: EventKind,
) -> Result<Vec<u8>, Error> {
    let (destination_id, reason) = match &state.key {
        AlertKey::UndeliverableDestination(destination, reason) => (
            Some(destination.as_str()),
            Some(match reason {
                Undeliverable::NotConfigured => "not_configured",
                Undeliverable::CredentialsMissing => "credentials_missing",
            }),
        ),
        AlertKey::OldestUndelivered
        | AlertKey::MachineSilent(_)
        | AlertKey::MachineSoftwareVersionChanged(_)
        | AlertKey::DiskReserve
        | AlertKey::OrderCacheStale
        | AlertKey::UncertainOrders
        | AlertKey::ParkedItems
        | AlertKey::RestorePaused => (None, None),
    };
    let (kind, machine_id, measurement) = match &state.key {
        AlertKey::OldestUndelivered => (
            "oldest_undelivered",
            None,
            Measurement::Seconds(state.value),
        ),
        AlertKey::MachineSilent(machine) => (
            "machine_silent",
            Some(machine.as_str()),
            Measurement::Seconds(state.value),
        ),
        AlertKey::MachineSoftwareVersionChanged(machine) => (
            "machine_software_version_changed",
            Some(machine.as_str()),
            Measurement::Count(state.value),
        ),
        AlertKey::DiskReserve => ("disk_reserve", None, Measurement::Bytes(state.value)),
        AlertKey::OrderCacheStale => ("order_cache_stale", None, Measurement::Seconds(state.value)),
        AlertKey::UncertainOrders => ("uncertain_orders", None, Measurement::Count(state.value)),
        AlertKey::ParkedItems => ("parked_items", None, Measurement::Count(state.value)),
        AlertKey::RestorePaused => ("restore_paused", None, Measurement::Paused(state.value)),
        AlertKey::UndeliverableDestination(..) => (
            "undeliverable_destination",
            None,
            Measurement::Count(state.value),
        ),
    };
    let bytes = serde_json::to_vec(&Event {
        // Version 2 added destination_id and reason; stored version 1 bytes are resent unchanged.
        version: 2,
        event_id: id,
        condition_id: format!("{namespace}:{}", crate::key(&state.key)?),
        episode_id: format!("{namespace}:{}", episode.0),
        event,
        kind,
        machine_id,
        destination_id,
        reason,
        responder: &state.responder,
        observed_at: state.evaluated_at,
        episode_at: episode.1,
        measurement,
    })?;
    if bytes.len() > 16384 {
        return Err(Error::Storage(crate::StorageError::LimitExceeded {
            what: "record_bytes",
        }));
    }
    Ok(bytes)
}
