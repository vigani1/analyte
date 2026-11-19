//! Drives LAW work exchanges with cached orders, reservations and item outcomes (TF-2b 3.27/3.28).
use crate::machine::hl7::orders::response::{Profile, response_metadata};
use crate::machine::law::error::OrderError as Error;
use crate::orders::reservation;
use crate::runtime::hooks::Operation;
use analyte_dialog::law::outcome::{self, Code as LawEnd, Detail as LawDetail};
use std::error::Error as _;
mod sending;
#[cfg(test)]
mod tests;
pub(crate) mod validation;
mod work;
use crate::{deadline, installation::config::Machine, intake};
pub(crate) use analyte_dialog::law::Reply;
use analyte_hl7::{AckCode, AckError, Message, Options, Responsibility, law};
use analyte_model::{Identifier, OrderState, ReceiptId, SampleId, SourceText};
use analyte_store::{
    Handle, ItemDecision, ItemOutcome, Lookup, MachineOrder, MachineOrderKey, PendingOrders,
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use work::{address, render};

pub(crate) struct LawOrderLink {
    owner: Identifier,
    reserving: Vec<MachineOrderKey>,
    exchange: Option<Exchange>,
    cursor: String,
    start: Instant,
}

/// LAW settings an order exchange needs, built only when work orders are configured.
#[derive(Clone, Copy)]
struct Settings<'a> {
    stack: &'a analyte_config::LawStack,
    orders: &'a analyte_config::LawOrders,
    options: Options,
}

struct Exchange {
    records: Vec<MachineOrder>,
    protocol: analyte_dialog::law::WorkExchange,
    query: Option<ReceiptId>,
}

impl LawOrderLink {
    pub(crate) fn new() -> Result<Self, Error> {
        Ok(Self {
            owner: Identifier::new(&uuid::Uuid::now_v7().to_string())?,
            reserving: Vec::new(),
            exchange: None,
            cursor: String::new(),
            start: Instant::now(),
        })
    }
    pub(crate) fn due(&self) -> Option<Instant> {
        self.exchange
            .as_ref()
            .and_then(|exchange| self.start.checked_add(exchange.protocol.deadline()))
    }
    pub(crate) async fn tick(&mut self, store: &Handle) -> Result<(), Error> {
        if self
            .exchange
            .as_ref()
            .is_some_and(|exchange| exchange.protocol.expired(self.start.elapsed()))
        {
            self.close(store, LawEnd::OrlTimeout.as_str()).await?;
        }
        Ok(())
    }
    pub(crate) fn issued(&mut self, wait: Duration) -> Result<(), Error> {
        if let Some(exchange) = &mut self.exchange {
            exchange.protocol.issued(self.start.elapsed(), wait)?;
        }
        Ok(())
    }
    /// Cancellation can leave an owned reservation committed. Cleanup queries every key
    /// registered before submission; the store owner serializes cleanup after that commit.
    pub(crate) async fn close(&mut self, store: &Handle, code: &str) -> Result<(), Error> {
        let result = crate::orders::sending::settle_owned(
            store,
            std::mem::take(&mut self.reserving),
            &self.owner,
            code,
            crate::orders::sending::Settlement::Items,
        )
        .await;
        self.exchange = None;
        Ok(result?)
    }

    pub(crate) async fn answer(
        &mut self,
        store: &Handle,
        machine: &Machine,
        stack: &analyte_config::LawStack,
        message: &Message,
        receipt: ReceiptId,
        started: Instant,
    ) -> Result<Reply, Error> {
        let site = machine.snapshot.effective().site();
        let options = crate::machine::law::settings::options(stack, site);
        let query = match law::receive_query(message) {
            Ok(query) => query,
            Err(error) => {
                intake::failed_reading(store, machine, receipt, &error.to_string()).await?;
                return reject_query(message, &error, options);
            }
        };
        let policy = stack.policy();
        if !supported(policy, query.scope()) {
            intake::failed_reading(
                store,
                machine,
                receipt,
                "LAW query form outside configured support",
            )
            .await?;
            return reply(
                query,
                law::CacheAnswer::Rejected(AckError::unlocated(
                    analyte_hl7::APPLICATION_INTERNAL_ERROR,
                    b"LAW-unsupported-query",
                )),
                options,
            );
        }
        let Some(orders) = stack.orders() else {
            intake::failed_reading(store, machine, receipt, "LAW query work layout is disabled")
                .await?;
            return reply(
                query,
                law::CacheAnswer::Unavailable(AckError::unlocated(
                    analyte_hl7::APPLICATION_INTERNAL_ERROR,
                    b"LAW-LAB27-disabled",
                )),
                options,
            );
        };
        if self.exchange.is_some() {
            intake::failed_reading(
                store,
                machine,
                receipt,
                "LAW query arrived while an ORL is pending",
            )
            .await?;
            return reply(
                query,
                law::CacheAnswer::Unavailable(AckError::unlocated(
                    analyte_hl7::APPLICATION_INTERNAL_ERROR,
                    b"LAW-query-busy",
                )),
                options,
            );
        }
        let until = started
            .checked_add(Duration::from_millis(u64::from(policy.query_deadline_ms())))
            .ok_or(Error::QueryDeadlineOverflow)?;
        let settings = Settings {
            stack,
            orders,
            options,
        };
        match self
            .prepare(store, machine, settings, query, receipt, until)
            .await
        {
            Ok(messages) => {
                // The reservation and rendered reply exist; record the query as handled
                // before any reply byte so recovery does not read it as a result.
                intake::mark_handled(store, machine, receipt).await?;
                Ok(messages)
            }
            Err(error) => {
                self.close(store, LawEnd::QueryPreparationFailed.as_str())
                    .await?;
                let reason = error
                    .source()
                    .map_or_else(|| error.to_string(), |cause| format!("{error}: {cause}"));
                intake::failed_reading(store, machine, receipt, &reason).await?;
                reply(
                    query,
                    law::CacheAnswer::Unavailable(AckError::unlocated(
                        analyte_hl7::APPLICATION_INTERNAL_ERROR,
                        b"LAW-query-unavailable",
                    )),
                    options,
                )
            }
        }
    }

    async fn prepare(
        &mut self,
        store: &Handle,
        machine: &Machine,
        settings: Settings<'_>,
        query: law::Query<'_>,
        receipt: ReceiptId,
        until: Instant,
    ) -> Result<Reply, Error> {
        let options = settings.options;
        let work = Arc::new(settings.orders.work().clone());
        let sample = match query.scope() {
            law::Scope::Container(id) => {
                if !id.is_local_to(work.container_namespace.as_str().as_bytes()) {
                    return Err(Error::ContainerAuthorityMismatch);
                }
                Some(SampleId::new(std::str::from_utf8(id.value())?)?)
            }
            law::Scope::AllPending => None,
            law::Scope::Rack { .. }
            | law::Scope::Tray { .. }
            | law::Scope::Isolate { .. }
            | law::Scope::InformationalRack { .. } => {
                return Err(Error::UnsupportedQueryScope);
            }
        };
        let cached = query_orders(store, machine, sample, until, options).await?;
        let prepared = self
            .reserve(store, machine, settings, cached, until, false)
            .await?;
        machine
            .hooks
            .operation(Operation::Reserved)
            .await
            .map_err(Error::TestHook)?;
        if Instant::now() >= until {
            return Err(Error::QueryDeadlineAfterReservation);
        }
        let followup = if prepared.is_empty() {
            response_metadata(Profile::Law).build(|response| {
                Ok::<_, Error>(law::negative_work(
                    query,
                    address(&work),
                    response,
                    options,
                )?)
            })?
        } else {
            let refs = prepared
                .iter()
                .map(|(command, record)| (command, record))
                .collect::<Vec<_>>();
            render(&refs, law::WorkAction::New, options)?
        };
        let answer = if prepared.is_empty() {
            law::CacheAnswer::NoWork
        } else {
            law::CacheAnswer::WorkExists
        };
        let Reply::Response(response) = reply(query, answer, options)? else {
            return Err(Error::UnexpectedNestedFollowup);
        };
        if Instant::now() >= until {
            return Err(Error::QueryDeadlineDuringRendering);
        }
        let outgoing = Reply::Followup {
            response,
            work: followup.clone(),
        };
        let prepared_query = prepared.is_empty().then_some(receipt);
        self.exchange = Some(Exchange {
            records: prepared.into_iter().map(|(_, record)| record).collect(),
            protocol: analyte_dialog::law::WorkExchange::new(
                followup,
                until.duration_since(self.start),
            ),
            query: prepared_query,
        });
        Ok(outgoing)
    }

    pub(crate) async fn orl(
        &mut self,
        store: &Handle,
        machine: &Machine,
        message: &Message,
        receipt: ReceiptId,
    ) -> Result<(), Error> {
        let Some(exchange) = self.exchange.as_ref() else {
            intake::failed_reading(store, machine, receipt, "LAW ORL has no pending followup")
                .await?;
            return Ok(());
        };
        if exchange.protocol.expired(self.start.elapsed()) {
            self.close(store, LawEnd::OrlLate.as_str()).await?;
            intake::failed_reading(
                store,
                machine,
                receipt,
                "LAW ORL arrived after response deadline",
            )
            .await?;
            return Ok(());
        }
        let response = exchange.protocol.response(message);
        let correlated = response.is_ok();
        if let Err(error) = &response {
            intake::failed_reading(store, machine, receipt, &error.to_string()).await?;
        }
        if let Some(query) = exchange.query {
            let code = response
                .as_ref()
                .map(|r| outcome::acceptance(r.acceptance()).to_owned())
                .unwrap_or_else(|_| "invalid".into());
            let actor = machine.id.clone();
            store
                .request::<_, analyte_store::OrderError>(deadline()?, move |store| {
                    Ok::<_, analyte_store::OrderError>(
                        store.audit(&analyte_store::Audit {
                            actor,
                            action: Identifier::new(LawEnd::NegativeQueryOrl.as_str()).map_err(
                                |cause| {
                                    analyte_store::OrderError::Storage(
                                        analyte_store::StorageError::Text(cause),
                                    )
                                },
                            )?,
                            targets: vec![
                                Identifier::new(&uuid::Uuid::from(receipt).to_string()).map_err(
                                    |cause| {
                                        analyte_store::OrderError::Storage(
                                            analyte_store::StorageError::Text(cause),
                                        )
                                    },
                                )?,
                                Identifier::new(&uuid::Uuid::from(query).to_string()).map_err(
                                    |cause| {
                                        analyte_store::OrderError::Storage(
                                            analyte_store::StorageError::Text(cause),
                                        )
                                    },
                                )?,
                            ],
                            time: jiff::Timestamp::now().as_second(),
                            reason: analyte_model::Reason::new(&format!(
                                "negative followup ORL correlated={correlated} acceptance={code}"
                            ))
                            .map_err(|cause| {
                                analyte_store::OrderError::Storage(
                                    analyte_store::StorageError::Text(cause),
                                )
                            })?,
                        })?,
                    )
                })
                .await?;
        }
        for record in &exchange.records {
            let outcomes = item_outcomes(record, &response)?;
            let id = record.id;
            let key = record.key.clone();
            store
                .request::<_, analyte_store::OrderError>(deadline()?, move |store| {
                    if store.machine_order(&key)?.is_some_and(|current| {
                        matches!(
                            current.state,
                            OrderState::Sending | OrderState::CancelRequested
                        )
                    }) {
                        store.finish_order_items(
                            id,
                            &outcomes,
                            jiff::Timestamp::now().as_second(),
                            crate::orders::event,
                        )?;
                    }
                    Ok(())
                })
                .await?;
        }
        self.exchange = None;
        self.reserving.clear();
        // An uncorrelated ORL is already parked above. A correlated one is handled
        // only now that its audit and item outcomes are committed.
        if correlated {
            intake::mark_handled(store, machine, receipt).await?;
        }
        Ok(())
    }
}

fn reply(
    query: law::Query<'_>,
    answer: law::CacheAnswer<'_>,
    options: Options,
) -> Result<Reply, Error> {
    response_metadata(Profile::Law).build(|response| {
        Ok(Reply::Response(
            law::query_response(
                query,
                answer,
                response,
                Responsibility::StoredAndInterpreted,
                options,
            )?
            .message()
            .clone(),
        ))
    })
}

/// True when the configured LAW policy answers this query form.
fn supported(policy: &analyte_config::LawPolicy, scope: law::Scope<'_>) -> bool {
    match scope {
        law::Scope::Container(_) => policy
            .query_forms()
            .contains(&analyte_config::LawQueryForm::Container),
        law::Scope::AllPending => policy
            .query_forms()
            .contains(&analyte_config::LawQueryForm::AllPending),
        law::Scope::Rack { .. }
        | law::Scope::Tray { .. }
        | law::Scope::Isolate { .. }
        | law::Scope::InformationalRack { .. } => false,
    }
}

fn reject_query(message: &Message, error: &law::Failure, options: Options) -> Result<Reply, Error> {
    response_metadata(Profile::Law).build(|response| {
        let detail = error.acknowledgment_error(message, b"102", b"LAW-invalid-query");
        law::reject_trigger(
            message.raw(),
            response,
            error
                .acknowledgment_code()
                .ok_or(Error::QueryFailureHasNoAckOutcome)?,
            detail,
            options,
        )?
        .map(Reply::Response)
        .ok_or_else(|| Error::QueryCorrelationMissing)
    })
}

async fn query_orders(
    store: &Handle,
    machine: &Machine,
    sample: Option<SampleId>,
    until: Instant,
    options: Options,
) -> Result<Vec<analyte_store::CachedOrder>, Error> {
    let freshness = machine.destination_limits.freshness.as_secs();
    let destination = crate::orders::destination(machine);
    let target = machine.id.clone();
    let bound = options.limits.segments;
    machine
        .hooks
        .operation(Operation::Lookup)
        .await
        .map_err(Error::TestHook)?;
    let pages_gate = machine.hooks.pages();
    let cached = store
        .request::<_, Error>(until, move |store| {
            let now = jiff::Timestamp::now().as_second();
            let mut found = Vec::new();
            if let Some(sample) = sample {
                match store.orders(&destination, &sample, now, freshness)? {
                    Lookup::Orders(orders) => {
                        for order in orders {
                            let key = reservation::key_parts(
                                destination.clone(),
                                order.id.clone(),
                                target.clone(),
                            );
                            if store.machine_order(&key)?.is_none() {
                                found.push(order);
                            }
                        }
                    }
                    Lookup::NoOrders => {}
                    Lookup::NotTrusted => {
                        return Err(analyte_store::OrderError::Conflict.into());
                    }
                }
            } else {
                let mut after = String::new();
                for page in 0..bound {
                    if Instant::now() >= until {
                        return Err(Error::LookupDeadline);
                    }
                    match store.pending_orders(&destination, &target, &after, now, freshness)? {
                        PendingOrders::NotTrusted => {
                            return Err(analyte_store::OrderError::Conflict.into());
                        }
                        PendingOrders::Page {
                            orders,
                            after: cursor,
                        } => {
                            if found
                                .len()
                                .checked_add(orders.len())
                                .is_none_or(|count| count > bound)
                            {
                                return Err(analyte_store::OrderError::Storage(
                                    analyte_store::StorageError::LimitExceeded {
                                        what: "pending_order_rows",
                                    },
                                )
                                .into());
                            }
                            found.extend(orders);
                            pages_gate.pause().map_err(Error::TestHook)?;
                            match cursor {
                                Some(cursor) => after = cursor.as_str().to_owned(),
                                None => return Ok(found),
                            }
                        }
                    }
                    if page.checked_add(1) == Some(bound) {
                        return Err(analyte_store::OrderError::Storage(
                            analyte_store::StorageError::LimitExceeded {
                                what: "pending_order_pages",
                            },
                        )
                        .into());
                    }
                }
            }
            Ok(found)
        })
        .await?;
    Ok(cached)
}

fn item_outcomes(
    record: &MachineOrder,
    response: &Result<law::WorkResponse<'_>, law::Failure>,
) -> Result<Vec<ItemOutcome>, Error> {
    let mut outcomes = Vec::new();
    for item in &record.items {
        let expected = &item.placer;
        let (decision, code) = match &response {
            Ok(response) if response.acceptance() == AckCode::Aa => {
                match response
                    .decisions()
                    .iter()
                    .find(|decision| decision.id.raw() == expected.as_bytes())
                    .map(|decision| decision.outcome)
                {
                    Some(law::AwosOutcome::Accepted) => {
                        (ItemDecision::Accepted, LawEnd::Ok.as_str().to_owned())
                    }
                    Some(law::AwosOutcome::UnableToAccept) => {
                        (ItemDecision::Rejected, LawEnd::Ua.as_str().to_owned())
                    }
                    Some(law::AwosOutcome::CancelAccepted) => (
                        ItemDecision::CancelConfirmed,
                        LawEnd::Cr.as_str().to_owned(),
                    ),
                    Some(law::AwosOutcome::UnableToCancel) => {
                        (ItemDecision::CancelRefused, LawEnd::Uc.as_str().to_owned())
                    }
                    None => (
                        ItemDecision::Uncertain,
                        LawEnd::OrlItemMissing.as_str().to_owned(),
                    ),
                }
            }
            Ok(response) => (
                ItemDecision::Uncertain,
                LawDetail::Application(response.acceptance()).to_string(),
            ),
            Err(error) => (
                ItemDecision::Uncertain,
                LawDetail::InvalidOrl(error.to_string()).to_string(),
            ),
        };
        let code = if let Ok(response) = response {
            response
                .decisions()
                .iter()
                .find(|d| d.id.raw() == expected.as_bytes())
                .map_or(code.clone(), |d| {
                    format!("{code}:{}", d.status.as_bytes().escape_ascii())
                })
        } else {
            code
        };
        outcomes.push(ItemOutcome {
            index: item.index,
            decision,
            code: SourceText::new(&code)?,
        });
    }
    Ok(outcomes)
}
