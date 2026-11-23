//! Reserves a synthetic order through the same cache and lifecycle as the engine.
use analyte_model::{Identifier, SampleId};
use analyte_store::{
    CachedOrder, MachineOrderKey, OrderError, OrderPage, OrderReservation, ReserveOptions,
    StorageError, Store,
};
use std::num::NonZeroU64;

/// Seeds a requested order and returns its durable sending reservation.
/// # Errors
/// Returns cache or reservation failures from the store.
pub fn reserve(store: &mut Store, key: &MachineOrderKey, now: i64) -> Result<i64, OrderError> {
    let text = |value| Identifier::new(value).map_err(StorageError::Text);
    store.order_page(&OrderPage {
        destination: key.destination.clone(),
        cursor: text("synthetic")?,
        complete: true,
        polled: now,
        orders: vec![CachedOrder {
            id: key.order.clone(),
            sample: SampleId::new("001").map_err(StorageError::Text)?,
            document: r#"{"version":1,"state":"Requested","tombstone":false,"tests":[{"namespace":"local","value":"synthetic"}]}"#.into(),
            tombstone: false,
            expiry: i64::MAX,
        }],
    })?;
    let reservation = store.reserve_send(
        &OrderReservation {
            identity: key,
            model: &text("example/synthetic")?,
            version: NonZeroU64::MIN,
            now,
            freshness: 3600,
        },
        ReserveOptions::default(),
    )?;
    reservation
        .map(|order| order.id)
        .map_err(|refusal| OrderError::DeliveryBuild(Box::new(refusal)))
}
