//! Admits send-ahead orders before reserving and writing machine commands.
use crate::orders::OrderError as Error;
use crate::{deadline, installation::config::Machine};
use analyte_dialog::outcome::Host as HostEnd;
use analyte_model::{OrderState, SourceText};
use analyte_store::{Handle, MachineOrder, MachineOrderKey};
mod work;
pub(crate) use work::Work;

/// Idle send-ahead candidate polling cadence.
pub(crate) const ORDER_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);

#[cfg(test)]
mod tests;
#[cfg(test)]
use super::astm::finished;

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
            .request::<_, analyte_store::OrderError>(until, move |store| {
                store.reserve_send(
                    &super::reservation::candidate(
                        &candidate.identity,
                        candidate.version,
                        freshness,
                    ),
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
                    uncertain(store, &order, HostEnd::QueryReservationRefused.as_str()).await?;
                }
                return Ok(false);
            }
        }
    }
    if std::time::Instant::now() >= until && !active.is_empty() {
        for order in active.drain(..) {
            uncertain(store, &order, HostEnd::QueryDeadlineBeforeSend.as_str()).await?;
        }
        return Ok(false);
    }
    Ok(true)
}

/// A durable reservation paired with its already mapped, bounded wire records.
pub(crate) struct Reserved<T> {
    /// Persisted identity used for every subsequent outcome.
    pub(crate) order: MachineOrder,
    /// Complete records ready for the connection sender.
    pub(crate) payload: T,
}

/// One bounded candidate page per idle interval; the cursor prevents starvation.
pub(crate) async fn next<T, E>(
    store: &Handle,
    machine: &Machine,
    after: &mut String,
    map: impl Fn(&analyte_mapping::AppOrder) -> Result<T, E>,
) -> Result<Option<Reserved<T>>, Error>
where
    Error: From<E>,
{
    let freshness = machine.destination_limits.freshness.as_secs();
    if !super::settings::send_ahead(machine) {
        return Ok(None);
    }
    let candidates = candidate_page(store, machine, after).await?;
    for cached in candidates {
        *after = cached.id.as_str().to_owned();
        let super::admission::Prepared::Mapped(candidate, mapped) = super::admission::prepare(
            store,
            machine,
            cached,
            deadline()?,
            |error| format!("rejected: {error}"),
            |order| {
                let order = super::app_order(order)?;
                map(&order).map_err(Error::from)
            },
        )
        .await?
        else {
            continue;
        };
        let identity = candidate.identity;
        let version = candidate.version;
        let reserved = store
            .request::<_, analyte_store::OrderError>(deadline()?, move |store| {
                store.reserve_send(
                    &super::reservation::candidate(&identity, version, freshness),
                    analyte_store::ReserveOptions::default(),
                )
            })
            .await?;
        match reserved {
            Ok(order) => {
                return Ok(Some(Reserved {
                    order,
                    payload: mapped,
                }));
            }
            Err(refusal) => tracing::info!(reason=%refusal, "order reservation refused"),
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
        .request::<_, analyte_store::OrderError>(deadline()?, move |store| {
            if !store
                .order_trust(&destination, jiff::Timestamp::now().as_second(), freshness)?
                .is_trusted()
            {
                return Ok::<_, analyte_store::OrderError>(Vec::new());
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
        .request::<_, analyte_store::OrderError>(deadline()?, move |store| {
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
    let until = deadline()?;
    for state in [OrderState::Sending, OrderState::CancelRequested] {
        let mut after = 0;
        loop {
            let orders = store
                .request::<_, analyte_store::OrderError>(until, move |store| {
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
                    .request::<_, analyte_store::OrderError>(until, move |store| {
                        store.transition_order(
                            id,
                            OrderState::Uncertain,
                            SourceText::new(HostEnd::HostRestart.as_str()).map_err(|cause| {
                                analyte_store::OrderError::Storage(
                                    analyte_store::StorageError::Text(cause),
                                )
                            })?,
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
    summary: Settlement,
) -> Result<(), Error> {
    let code = SourceText::new(code)?;
    let mut result = Ok(());
    for key in keys {
        let owner = owner.clone();
        let code = code.clone();
        let cleanup = store
            .request::<_, analyte_store::OrderError>(deadline()?, move |store| {
                if let Some(record) = store.machine_order(&key)?
                    && (record.reserved_by(&owner) || record.cancel_requested_by(&owner))
                {
                    if record.state == OrderState::Sending && matches!(summary, Settlement::Order) {
                        store.transition_order(
                            record.id,
                            OrderState::Uncertain,
                            code,
                            jiff::Timestamp::now().as_second(),
                            super::event,
                        )?;
                        return Ok::<_, analyte_store::OrderError>(());
                    }
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
        crate::cleanup::retain_cleanup(&mut result, cleanup);
    }
    result
}

/// Preserves the protocol's existing audit representation of an uncertain send.
#[derive(Clone, Copy)]
pub(crate) enum Settlement {
    Order,
    Items,
}
