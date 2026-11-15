//! Host-owned send-ahead admission. Mapping never reserves or writes commands.
use crate::{Error, config::Machine, deadline};
use analyte_model::{OrderState, SourceText};
use analyte_store::{Handle, MachineOrder, MachineOrderKey, OrderReservation};

pub(crate) struct Reserved {
    pub(crate) order: MachineOrder,
    pub(crate) records: Vec<Vec<u8>>,
}

/// One bounded candidate page per idle interval; the cursor prevents starvation.
pub(crate) async fn next(
    store: &Handle,
    machine: &Machine,
    after: &mut String,
) -> Result<Option<Reserved>, Error> {
    if !machine
        .snapshot
        .effective()
        .machine()
        .orders
        .as_ref()
        .is_some_and(|orders| orders.send_ahead)
    {
        return Ok(None);
    }
    let destination = super::destination(machine);
    let machine_id = machine.id.clone();
    let cursor = after.clone();
    let candidates = store
        .request(deadline(), move |store| {
            store.order_candidates(&destination, &machine_id, &cursor)
        })
        .await?;
    if candidates.is_empty() {
        after.clear();
        return Ok(None);
    }
    for cached in candidates {
        *after = cached.id.as_str().to_owned();
        let order: analyte_contract::Order = analyte_contract::decode(cached.document.as_bytes())?;
        let fields = order.fields();
        if fields.tombstone
            || fields.state != analyte_contract::evidence::OrderState::Requested
            || fields
                .target_analyzers
                .as_ref()
                .is_some_and(|targets| !targets.iter().any(|t| t.as_str() == machine.id.as_str()))
        {
            continue;
        }
        let identity = MachineOrderKey {
            destination: super::destination(machine),
            order: cached.id,
            machine: machine.id.clone(),
        };
        let version = fields.version;
        let mapped = super::app_order(&order)
            .and_then(|order| Ok(analyte_mapping::map_order(&order, &machine.snapshot)?));
        let mapped = match mapped {
            Ok(mapped) => mapped,
            Err(error) => {
                let reason = SourceText::new(&format!("rejected: {error}"))?;
                let rejected = store
                    .request(deadline(), move |store| {
                        store.reject_unsent(
                            &OrderReservation {
                                identity: &identity,
                                version,
                                now: jiff::Timestamp::now().as_second(),
                                freshness: super::FRESHNESS_SECONDS,
                            },
                            reason,
                            super::event,
                        )
                    })
                    .await?;
                if let Err(refusal) = rejected {
                    tracing::info!(reason = %refusal, "mapping rejection was superseded before recording");
                }
                continue;
            }
        };
        let reserved = store
            .request(deadline(), move |store| {
                store.reserve_send(
                    &identity,
                    version,
                    jiff::Timestamp::now().as_second(),
                    super::FRESHNESS_SECONDS,
                    super::event,
                )
            })
            .await?;
        if let Ok(order) = reserved {
            return Ok(Some(Reserved {
                order,
                records: super::astm::envelope(vec![mapped.record().to_vec()], b'N'),
            }));
        }
    }
    Ok(None)
}

/// A link failure never proves that the machine did not receive its command.
pub(crate) async fn uncertain(
    store: &Handle,
    order: &MachineOrder,
    code: &str,
) -> Result<(), Error> {
    let id = order.id;
    let code = SourceText::new(code)?;
    store
        .request(deadline(), move |store| {
            store.transition_order(
                id,
                OrderState::Uncertain,
                code,
                jiff::Timestamp::now().as_second(),
                super::event,
            )
        })
        .await?;
    Ok(())
}

pub(crate) async fn recover(store: &Handle) -> Result<(), Error> {
    let until = deadline();
    let mut after = 0;
    loop {
        let orders = store
            .request(until, move |store| {
                store.machine_orders(&analyte_store::MachineOrderFilter {
                    after,
                    machine: None,
                    state: Some(OrderState::Sent),
                })
            })
            .await?;
        if orders.is_empty() {
            return Ok(());
        }
        for order in orders {
            after = order.id;
            let id = order.id;
            store
                .request(until, move |store| {
                    store.transition_order(
                        id,
                        OrderState::Uncertain,
                        SourceText::new("host-restart")
                            .map_err(|_| analyte_store::Error::InvalidInput)?,
                        jiff::Timestamp::now().as_second(),
                        super::event,
                    )
                })
                .await?;
        }
    }
}
