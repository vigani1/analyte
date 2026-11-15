//! The same freshness and lifecycle admission rule for query and pushed orders.
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
