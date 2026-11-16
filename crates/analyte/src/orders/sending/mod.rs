//! Admits send-ahead orders before reserving and writing machine commands.
use crate::{Error, deadline, installation::config::Machine};
use analyte_model::{OrderState, SourceText};
use analyte_store::{Handle, MachineOrder, MachineOrderKey, OrderReservation};
#[cfg(test)]
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
    if !reserve_candidates(
        store,
        reply.candidates,
        until,
        active,
        None,
        machine.destination_limits.freshness.as_secs(),
    )
    .await?
    {
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
    freshness: u64,
) -> Result<bool, Error> {
    for candidate in candidates {
        let owner = owner.clone();
        let reserved = store
            .request(until, move |store| {
                store.reserve_send(
                    &OrderReservation {
                        identity: &candidate.identity,
                        version: candidate.version,
                        now: jiff::Timestamp::now().as_second(),
                        freshness,
                    },
                    analyte_store::ReserveOptions {
                        owner: owner.as_ref(),
                        ..Default::default()
                    },
                )
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
    let freshness = machine.destination_limits.freshness.as_secs();
    if !super::settings::send_ahead(machine) {
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
            .map_err(Error::from)
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
                                freshness,
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
                    &OrderReservation {
                        identity: &identity,
                        version,
                        now: jiff::Timestamp::now().as_second(),
                        freshness,
                    },
                    analyte_store::ReserveOptions::default(),
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
    let freshness = machine.destination_limits.freshness.as_secs();
    let destination = super::destination(machine);
    let machine_id = machine.id.clone();
    let cursor = after.clone();
    let candidates = store
        .request(deadline(), move |store| {
            if !store
                .order_trust(&destination, jiff::Timestamp::now().as_second(), freshness)?
                .is_trusted()
            {
                return Ok(Vec::new());
            }
            store.order_candidates(
                &destination,
                &machine_id,
                &cursor,
                jiff::Timestamp::now().as_second(),
                freshness,
            )
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

/// Settles every owned in-flight reservation, preserving every independent failure.
/// Cleanup never issues a command and never changes an already confirmed outcome.
pub(crate) async fn settle_owned(
    store: &Handle,
    keys: Vec<MachineOrderKey>,
    owner: &analyte_model::Identifier,
    code: &str,
) -> Result<(), Error> {
    let code = SourceText::new(code)?;
    let mut result = Ok(());
    for key in keys {
        let owner = owner.clone();
        let code = code.clone();
        let cleanup = store
            .request(deadline(), move |store| {
                if let Some(record) = store.machine_order(&key)?
                    && (record.reserved_by(&owner) || record.cancel_requested_by(&owner))
                {
                    let outcomes = record
                        .items
                        .iter()
                        .map(|item| analyte_store::ItemOutcome {
                            index: item.index,
                            decision: analyte_store::ItemDecision::Uncertain,
                            code: code.clone(),
                        })
                        .collect::<Vec<_>>();
                    store.finish_order_items(
                        record.id,
                        &outcomes,
                        jiff::Timestamp::now().as_second(),
                        super::event,
                    )?;
                }
                Ok(())
            })
            .await
            .map_err(Into::into);
        crate::machine::custody::retain_cleanup(&mut result, cleanup);
    }
    result
}
