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
use crate::machine::astm::orders::finished;

/// Tracks candidate keys before storage I/O. A cancelled or timed-out request
/// may still commit; the owner check lets disconnect cleanup settle it safely.
pub(crate) struct Reservations {
    /// Identity stored with each reservation assigned to this connection.
    pub(crate) owner: analyte_model::Identifier,
    /// Attempts awaiting a store reply or a definitive outcome.
    pub(crate) keys: Vec<MachineOrderKey>,
}

impl Reservations {
    /// Creates an empty tracker with a fresh owner for this connection.
    pub(crate) fn new() -> Result<Self, analyte_model::TextError> {
        Ok(Self {
            owner: analyte_model::Identifier::new(&uuid::Uuid::now_v7().to_string())?,
            keys: Vec::new(),
        })
    }

    async fn reserve(
        &mut self,
        store: &Handle,
        machine: &Machine,
        candidate: super::Candidate,
        until: std::time::Instant,
    ) -> Result<Result<MachineOrder, analyte_store::SendRefusal>, Error> {
        let model = super::stored_model(&machine.model)?;
        let freshness = machine.destination_limits.freshness.as_secs();
        let owner = self.owner.clone();
        self.keys.push(candidate.identity.clone());
        let reserved = store
            .request::<_, analyte_store::OrderError>(until, move |store| {
                store.reserve_send(
                    &super::reservation::candidate(
                        &candidate.identity,
                        &model,
                        candidate.version,
                        freshness,
                    ),
                    analyte_store::ReserveOptions {
                        owner: Some(&owner),
                        ..Default::default()
                    },
                )
            })
            .await?;
        if reserved.is_err() {
            self.keys.pop();
        }
        Ok(reserved)
    }

    /// Reconciles all tracked keys without issuing any machine commands.
    pub(crate) async fn settle(&self, store: &Handle, code: &str) -> Result<(), Error> {
        settle_owned(store, &self.keys, &self.owner, code, Settlement::Order).await
    }
}

/// Reserves a complete query reply before allowing its records onto the link.
pub(crate) async fn reserve_candidates(
    store: &Handle,
    machine: &Machine,
    candidates: Vec<super::Candidate>,
    until: std::time::Instant,
    active: &mut Vec<MachineOrder>,
    reservations: &mut Reservations,
) -> Result<bool, Error> {
    for candidate in candidates {
        match reservations
            .reserve(store, machine, candidate, until)
            .await?
        {
            Ok(order) => active.push(order),
            Err(refusal) => {
                tracing::info!(reason=%refusal,"query order reservation refused");
                return abandon_query(
                    store,
                    active,
                    reservations,
                    HostEnd::QueryReservationRefused,
                )
                .await;
            }
        }
    }
    if std::time::Instant::now() >= until && !active.is_empty() {
        return abandon_query(
            store,
            active,
            reservations,
            HostEnd::QueryDeadlineBeforeSend,
        )
        .await;
    }
    Ok(true)
}

async fn abandon_query(
    store: &Handle,
    active: &mut Vec<MachineOrder>,
    reservations: &mut Reservations,
    reason: HostEnd,
) -> Result<bool, Error> {
    active.clear();
    reservations.settle(store, reason.as_str()).await?;
    reservations.keys.clear();
    Ok(false)
}

/// A durable reservation paired with its already mapped, bounded wire records.
pub(crate) struct Reserved<T> {
    /// Persisted identity used for every subsequent outcome.
    pub(crate) order: MachineOrder,
    /// Complete records ready for the connection sender.
    pub(crate) payload: T,
}

/// One bounded candidate page per idle interval; the cursor prevents starvation.
pub(crate) async fn next<T, E: From<Error> + std::fmt::Display>(
    store: &Handle,
    machine: &Machine,
    after: &mut String,
    reservations: &mut Reservations,
    map: impl Fn(&analyte_mapping::AppOrder) -> Result<T, E>,
) -> Result<Option<Reserved<T>>, E> {
    let candidates = candidate_page(store, machine, after).await?;
    for cached in candidates {
        *after = cached.id.as_str().to_owned();
        let super::admission::Prepared::Mapped(candidate, mapped) = super::admission::prepare(
            store,
            machine,
            cached,
            deadline().map_err(Error::from)?,
            |error| format!("rejected: {error}"),
            |order| map(&super::app_order(order).map_err(Error::from)?),
        )
        .await?
        else {
            continue;
        };
        let reserved = reservations
            .reserve(store, machine, candidate, deadline().map_err(Error::from)?)
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
    let destination = super::destination(machine)?;
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
/// It needs no program file: each event names the model stored with its order
/// (ADR 0033), so an order of a machine removed since it was sent still becomes
/// uncertain and visible.
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
/// Using the same worker queues cleanup behind any reservation still committing.
pub(crate) async fn settle_owned(
    store: &Handle,
    keys: &[MachineOrderKey],
    owner: &analyte_model::Identifier,
    code: &str,
    summary: Settlement,
) -> Result<(), Error> {
    let code = SourceText::new(code)?;
    let mut result = Ok(());
    for key in keys {
        let key = key.clone();
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
