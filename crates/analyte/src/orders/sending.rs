//! Host-owned send-ahead admission. Mapping never reserves or writes commands.
use crate::{Error, config::Machine, deadline};
use analyte_model::{OrderState, SourceText};
use analyte_store::{Handle, MachineOrder, MachineOrderKey, OrderReservation};
#[cfg(test)]
#[path = "sending_tests.rs"]
mod tests;

/// Reserves query orders immediately before link transmission, with no app event.
/// The connection owns every reservation even if this future fails or is cancelled.
pub(crate) async fn reserve_query(
    store: &Handle,
    machine: &Machine,
    reply: super::astm::Reply,
    until: std::time::Instant,
    active: &mut Vec<MachineOrder>,
) -> Result<Option<Vec<Vec<u8>>>, Error> {
    if !reserve_candidates(store, reply.candidates, until, active, None).await? {
        return Ok(super::astm::failure(machine).map(|reply| reply.records));
    }
    Ok(Some(reply.records))
}

/// Shared query admission. Each reservation is owned by the connection before I/O.
pub(crate) async fn reserve_candidates(
    store: &Handle,
    candidates: Vec<super::Candidate>,
    until: std::time::Instant,
    active: &mut Vec<MachineOrder>,
    owner: Option<analyte_model::Identifier>,
) -> Result<bool, Error> {
    for candidate in candidates {
        let owner = owner.clone();
        let reserved = store
            .request(until, move |store| {
                if let Some(owner) = owner {
                    store.reserve_send_owned(
                        &OrderReservation {
                            identity: &candidate.identity,
                            version: candidate.version,
                            now: jiff::Timestamp::now().as_second(),
                            freshness: super::FRESHNESS_SECONDS,
                        },
                        &owner,
                    )
                } else {
                    store.reserve_send(
                        &candidate.identity,
                        candidate.version,
                        jiff::Timestamp::now().as_second(),
                        super::FRESHNESS_SECONDS,
                    )
                }
            })
            .await?;
        match reserved {
            Ok(order) => active.push(order),
            Err(refusal) => {
                tracing::info!(reason=%refusal,"query order reservation refused");
                for order in active.drain(..) {
                    uncertain(store, &order, "query-reservation-refused").await?;
                }
                return Ok(false);
            }
        }
    }
    if std::time::Instant::now() >= until && !active.is_empty() {
        for order in active.drain(..) {
            uncertain(store, &order, "query-deadline-before-send").await?;
        }
        return Ok(false);
    }
    Ok(true)
}

/// A durable reservation paired with its already mapped, bounded wire records.
pub(crate) struct Reserved {
    /// Persisted identity used for every subsequent outcome.
    pub(crate) order: MachineOrder,
    /// Complete records ready for the connection sender.
    pub(crate) payload: analyte_mapping::MachineOrder,
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
    let candidates = candidate_page(store, machine, after).await?;
    for cached in candidates {
        *after = cached.id.as_str().to_owned();
        let order: analyte_contract::Order = analyte_contract::decode(cached.document.as_bytes())?;
        let fields = order.fields();
        if !super::eligible(&order, machine) {
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
                )
            })
            .await?;
        if let Ok(order) = reserved {
            return Ok(Some(Reserved {
                order,
                payload: mapped,
            }));
        }
    }
    Ok(None)
}

/// Shared bounded candidate cursor for unsolicited protocols.
pub(crate) async fn candidate_page(
    store: &Handle,
    machine: &Machine,
    after: &mut String,
) -> Result<Vec<analyte_store::CachedOrder>, Error> {
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
        return Ok(Vec::new());
    }
    Ok(candidates)
}

/// A link failure never proves that the machine did not receive its command.
pub(crate) async fn uncertain(
    store: &Handle,
    order: &MachineOrder,
    code: &str,
) -> Result<(), Error> {
    complete(store, order, OrderState::Uncertain, code).await
}

/// Records a specific failed transfer as uncertain; normal transfer becomes Sent.
pub(crate) async fn finished(
    store: &Handle,
    order: &MachineOrder,
    end: analyte_dialog::astm::End,
) -> Result<(), Error> {
    // LIS01 confirms transfer, not the analyzer application's acceptance.
    // Only explicit application evidence may advance Sent to Accepted.
    let code = match end {
        analyte_dialog::astm::End::Normal => "ASTM-transfer-confirmed",
        analyte_dialog::astm::End::Abort => "ASTM-transfer-aborted",
        analyte_dialog::astm::End::Timeout => "ASTM-transfer-timeout",
        analyte_dialog::astm::End::Interrupt => "ASTM-transfer-interrupted",
        analyte_dialog::astm::End::Incomplete => "ASTM-transfer-incomplete",
    };
    let state = if end == analyte_dialog::astm::End::Normal {
        OrderState::Sent
    } else {
        OrderState::Uncertain
    };
    complete(store, order, state, code).await
}

/// Commits a protocol's definitive transport outcome with exactly one app event.
pub(crate) async fn complete(
    store: &Handle,
    order: &MachineOrder,
    state: OrderState,
    code: &str,
) -> Result<(), Error> {
    let id = order.id;
    let code = SourceText::new(code)?;
    store
        .request(deadline(), move |store| {
            store.transition_order(
                id,
                state,
                code,
                jiff::Timestamp::now().as_second(),
                super::event,
            )
        })
        .await?;
    Ok(())
}

/// Recovers interrupted sends and admitted cancellations without issuing commands.
/// A lost cancel reply is uncertainty too; it never grants permission to repeat CA.
pub(crate) async fn recover(store: &Handle) -> Result<(), Error> {
    let until = deadline();
    for state in [OrderState::Sending, OrderState::CancelRequested] {
        let mut after = 0;
        loop {
            let orders = store
                .request(until, move |store| {
                    store.machine_orders(&analyte_store::MachineOrderFilter {
                        after,
                        machine: None,
                        state: Some(state),
                    })
                })
                .await?;
            if orders.is_empty() {
                break;
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
    Ok(())
}
