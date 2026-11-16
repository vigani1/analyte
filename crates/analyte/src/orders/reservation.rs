//! Rechecks order version and freshness for reservation candidates, mapping rejection and cancellation in the store.
use crate::Error;
use analyte_store::{Handle, MachineOrder, MachineOrderKey, OrderReservation, SendRefusal};
use std::{num::NonZeroU64, time::Instant};

/// Recheck the current version and freshness at the store operation, never at rendering.
pub(super) fn candidate(
    identity: &MachineOrderKey,
    version: NonZeroU64,
    freshness: u64,
) -> OrderReservation<'_> {
    OrderReservation {
        identity,
        version,
        now: jiff::Timestamp::now().as_second(),
        freshness,
    }
}

/// Mapping rejection changes only still-unsent, current, trusted work.
/// A superseding update returns its refusal without overwriting its lifecycle.
pub(super) async fn reject(
    store: &Handle,
    until: Instant,
    identity: MachineOrderKey,
    version: NonZeroU64,
    freshness: u64,
    reason: analyte_model::SourceText,
) -> Result<Result<MachineOrder, SendRefusal>, Error> {
    Ok(store
        .request(until, move |store| {
            store.reject_unsent(
                &candidate(&identity, version, freshness),
                reason,
                super::event,
            )
        })
        .await?)
}

/// Admit cancellation of the current version without resending the original order.
/// The caller registers the key before awaiting so disconnect cleanup owns it.
pub(super) async fn cancel(
    store: &Handle,
    until: Instant,
    id: i64,
    version: NonZeroU64,
    owner: analyte_model::Identifier,
    freshness: u64,
) -> Result<MachineOrder, analyte_store::Error> {
    store
        .request(until, move |store| {
            store.request_order_cancel(
                id,
                version,
                jiff::Timestamp::now().as_second(),
                &owner,
                freshness,
                super::event,
            )
        })
        .await
}
