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

/// Owns the typed inputs to atomic send admission and protocol preflight.
pub(super) struct Admission {
    pub(super) identity: MachineOrderKey,
    pub(super) version: NonZeroU64,
    pub(super) freshness: u64,
    pub(super) owner: analyte_model::Identifier,
    pub(super) groups: Vec<Vec<analyte_model::NamespacedId>>,
    pub(super) namespace: analyte_model::Identifier,
    pub(super) max_identity_bytes: usize,
}

/// Runs protocol rendering inside reservation admission, before any command can be sent.
/// The store refuses previously sent work; a lost completion stays owned for cleanup.
pub(super) async fn admit<T: Send + 'static>(
    store: &Handle,
    until: Instant,
    admission: Admission,
    encode: impl FnOnce(&MachineOrder) -> Result<T, analyte_store::Error> + Send + 'static,
) -> Result<Result<(MachineOrder, T), SendRefusal>, analyte_store::Error> {
    store
        .request(until, move |store| {
            let mut rendered = None;
            let mut encode = Some(encode);
            let reserved = store.reserve_send(
                &candidate(&admission.identity, admission.version, admission.freshness),
                analyte_store::ReserveOptions {
                    items: analyte_store::OrderItems::Groups {
                        groups: &admission.groups,
                        namespace: Some(&admission.namespace),
                        max_identity_bytes: admission.max_identity_bytes,
                    },
                    owner: Some(&admission.owner),
                    prepare: Some(&mut |record| {
                        rendered = Some(encode
                            .take()
                            .ok_or(analyte_store::Error::InvalidHostInput)?(
                            record
                        )?);
                        Ok(())
                    }),
                },
            )?;
            match reserved {
                Ok(record) => Ok(Ok((
                    record,
                    rendered.ok_or(analyte_store::Error::InvalidHostInput)?,
                ))),
                Err(refusal) => Ok(Err(refusal)),
            }
        })
        .await
}

/// Builds the durable machine-order identity in one place for both HL7 profiles.
pub(super) fn key(
    machine: &crate::installation::config::Machine,
    order: analyte_model::Identifier,
) -> MachineOrderKey {
    MachineOrderKey {
        destination: super::destination(machine),
        order,
        machine: machine.id.clone(),
    }
}

/// Lists cancellation candidates only while the cached order feed is trusted.
pub(super) async fn cancellations(
    store: &Handle,
    machine: &crate::installation::config::Machine,
    until: Instant,
) -> Result<Vec<(MachineOrder, analyte_store::CachedOrder)>, Error> {
    let destination = super::destination(machine);
    let target = machine.id.clone();
    let freshness = machine.destination_limits.freshness.as_secs();
    Ok(store
        .request(until, move |store| {
            let now = jiff::Timestamp::now().as_second();
            if !store
                .order_trust(&destination, now, freshness)?
                .is_trusted()
            {
                return Ok(Vec::new());
            }
            store.cancel_candidates(&destination, &target, now, freshness)
        })
        .await?)
}
