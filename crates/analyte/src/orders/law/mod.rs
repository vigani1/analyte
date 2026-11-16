//! Drives LAW work exchanges with cached orders, reservations and item outcomes (TF-2b 3.27/3.28).
use crate::orders::outcome::Code;
mod sending;
pub(crate) mod validation;
mod work;
use crate::{Error, deadline, installation::config::Machine, intake, invalid};
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

pub(crate) struct Connection {
    owner: Identifier,
    reserving: Vec<MachineOrderKey>,
    exchange: Option<Exchange>,
    cursor: String,
    clock: Instant,
}

pub(crate) use analyte_dialog::law::Reply;

struct Exchange {
    records: Vec<MachineOrder>,
    protocol: analyte_dialog::law::WorkExchange,
    query: Option<ReceiptId>,
}

impl Connection {
    pub(crate) fn new() -> Result<Self, Error> {
        Ok(Self {
            owner: Identifier::new(&uuid::Uuid::now_v7().to_string())?,
            reserving: Vec::new(),
            exchange: None,
            cursor: String::new(),
            clock: Instant::now(),
        })
    }
    pub(crate) fn due(&self) -> Option<Instant> {
        self.exchange
            .as_ref()
            .and_then(|exchange| self.clock.checked_add(exchange.protocol.deadline()))
    }
    pub(crate) async fn tick(&mut self, store: &Handle) -> Result<(), Error> {
        if self
            .exchange
            .as_ref()
            .is_some_and(|exchange| exchange.protocol.expired(self.clock.elapsed()))
        {
            self.close(store, Code::LawOrlTimeout.as_str()).await?;
        }
        Ok(())
    }
    pub(crate) fn issued(&mut self, wait: Duration) -> Result<(), Error> {
        if let Some(exchange) = &mut self.exchange {
            exchange.protocol.issued(self.clock.elapsed(), wait)?;
        }
        Ok(())
    }
    /// Cancellation can leave an owned reservation committed. Cleanup queries every key
    /// registered before submission; the store owner serializes cleanup after that commit.
    pub(crate) async fn close(&mut self, store: &Handle, code: &str) -> Result<(), Error> {
        let result = super::sending::settle_owned(
            store,
            std::mem::take(&mut self.reserving),
            &self.owner,
            code,
            super::sending::Settlement::Items,
        )
        .await;
        self.exchange = None;
        result
    }

    pub(crate) async fn answer(
        &mut self,
        store: &Handle,
        machine: &Machine,
        message: &Message,
        receipt: ReceiptId,
        started: Instant,
        options: Options,
    ) -> Result<Reply, Error> {
        let query = match law::receive_query(message) {
            Ok(query) => query,
            Err(error) => {
                intake::failed_reading(store, machine, receipt, &error.to_string()).await?;
                return reject_query(message, &error, options);
            }
        };
        let stack = super::settings::law(machine)?;
        let settings = &stack.policy();
        let supported = match query.scope() {
            law::Scope::Container(_) => settings
                .query_forms()
                .contains(&analyte_config::LawQueryForm::Container),
            law::Scope::AllPending => settings
                .query_forms()
                .contains(&analyte_config::LawQueryForm::AllPending),
            law::Scope::Rack { .. }
            | law::Scope::Tray { .. }
            | law::Scope::Isolate { .. }
            | law::Scope::InformationalRack { .. } => false,
        };
        if !supported {
            intake::failed_reading(
                store,
                machine,
                receipt,
                "LAW query form outside configured support",
            )
            .await?;
            return reply(
                query,
                law::CacheAnswer::Rejected(AckError::unlocated(b"207", b"LAW-unsupported-query")),
                options,
            );
        }
        if stack.orders().is_none() {
            intake::failed_reading(store, machine, receipt, "LAW query work layout is disabled")
                .await?;
            return reply(
                query,
                law::CacheAnswer::Unavailable(AckError::unlocated(b"207", b"LAW-LAB27-disabled")),
                options,
            );
        }
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
                law::CacheAnswer::Unavailable(AckError::unlocated(b"207", b"LAW-query-busy")),
                options,
            );
        }
        let until = started
            .checked_add(Duration::from_millis(u64::from(
                settings.query_deadline_ms(),
            )))
            .ok_or("LAW query deadline overflow")?;
        match self
            .prepare(store, machine, query, receipt, until, options)
            .await
        {
            Ok(messages) => Ok(messages),
            Err(error) => {
                self.close(store, Code::LawQueryPreparationFailed.as_str())
                    .await?;
                let reason = error
                    .source()
                    .map_or_else(|| error.to_string(), |cause| format!("{error}: {cause}"));
                intake::failed_reading(store, machine, receipt, &reason).await?;
                reply(
                    query,
                    law::CacheAnswer::Unavailable(AckError::unlocated(
                        b"207",
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
        query: law::Query<'_>,
        receipt: ReceiptId,
        until: Instant,
        options: Options,
    ) -> Result<Reply, Error> {
        let cfg = super::settings::law(machine)?;
        let work = Arc::new(
            cfg.orders()
                .as_ref()
                .ok_or("LAW work layout absent")?
                .work()
                .clone(),
        );
        let sample = match query.scope() {
            law::Scope::Container(id) => {
                if !id.is_local_to(work.container_namespace.as_str().as_bytes()) {
                    return Err(invalid(
                        "LAW container assigning authority outside configured SampleId binding",
                    ));
                }
                Some(SampleId::new(std::str::from_utf8(id.value())?)?)
            }
            law::Scope::AllPending => None,
            law::Scope::Rack { .. }
            | law::Scope::Tray { .. }
            | law::Scope::Isolate { .. }
            | law::Scope::InformationalRack { .. } => {
                return Err(invalid("unsupported LAW query scope"));
            }
        };
        let cached = query_orders(store, machine, sample, until, options).await?;
        let prepared = self.reserve(store, machine, cached, until, options).await?;
        #[cfg(feature = "test-hooks")]
        if let Some(gate) = &machine.law_gates.reserved {
            gate.operation().await?;
        }
        if Instant::now() >= until {
            return Err(invalid("LAW query deadline after reservation"));
        }
        let followup = if prepared.is_empty() {
            crate::machine::hl7_response::response_metadata(
                crate::machine::hl7_response::Profile::Law,
            )
            .build(|response| {
                Ok(law::negative_work(
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
            return Err(invalid("unexpected nested LAW followup"));
        };
        if Instant::now() >= until {
            return Err(invalid("LAW query deadline during rendering"));
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
                until.duration_since(self.clock),
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
        if exchange.protocol.expired(self.clock.elapsed()) {
            self.close(store, Code::LawOrlLate.as_str()).await?;
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
        if let Err(error) = &response {
            intake::failed_reading(store, machine, receipt, &error.to_string()).await?;
        }
        if let Some(query) = exchange.query {
            let correlated = response.is_ok();
            let code = response
                .as_ref()
                .map(|r| super::acceptance::acceptance(r.acceptance()).to_owned())
                .unwrap_or_else(|_| "invalid".into());
            let actor = machine.id.clone();
            store
                .request(deadline(), move |store| {
                    store.audit(&analyte_store::Audit {
                        actor,
                        action: Identifier::new(Code::LawNegativeQueryOrl.as_str())
                            .map_err(analyte_store::Error::Text)?,
                        targets: vec![
                            Identifier::new(&uuid::Uuid::from(receipt).to_string())
                                .map_err(analyte_store::Error::Text)?,
                            Identifier::new(&uuid::Uuid::from(query).to_string())
                                .map_err(analyte_store::Error::Text)?,
                        ],
                        time: jiff::Timestamp::now().as_second(),
                        reason: analyte_model::Reason::new(&format!(
                            "negative followup ORL correlated={correlated} acceptance={code}"
                        ))
                        .map_err(analyte_store::Error::Text)?,
                    })
                })
                .await?;
        }
        for record in &exchange.records {
            let outcomes = item_outcomes(record, &response)?;
            let id = record.id;
            let key = record.key.clone();
            store
                .request(deadline(), move |store| {
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
                            super::event,
                        )?;
                    }
                    Ok(())
                })
                .await?;
        }
        self.exchange = None;
        self.reserving.clear();
        Ok(())
    }
}

fn reply(
    query: law::Query<'_>,
    answer: law::CacheAnswer<'_>,
    options: Options,
) -> Result<Reply, Error> {
    crate::machine::hl7_response::response_metadata(crate::machine::hl7_response::Profile::Law)
        .build(|response| {
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

fn reject_query(message: &Message, error: &law::Failure, options: Options) -> Result<Reply, Error> {
    crate::machine::hl7_response::response_metadata(crate::machine::hl7_response::Profile::Law)
        .build(|response| {
            let detail = error.acknowledgment_error(message, b"102", b"LAW-invalid-query");
            law::reject_trigger(
                message.raw(),
                response,
                error
                    .acknowledgment_code()
                    .ok_or("query failure has no ACK outcome")?,
                detail,
                options,
            )?
            .map(Reply::Response)
            .ok_or_else(|| invalid("LAW malformed query lacks recoverable correlation"))
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
    let destination = super::destination(machine);
    let target = machine.id.clone();
    let bound = options.limits.segments;
    #[cfg(feature = "test-hooks")]
    if let Some(gate) = &machine.law_gates.lookup {
        gate.operation().await?;
    }
    #[cfg(feature = "test-hooks")]
    let pages_gate = machine.law_gates.pages.clone();
    let cached = store
        .request(until, move |store| {
            let now = jiff::Timestamp::now().as_second();
            let mut found = Vec::new();
            if let Some(sample) = sample {
                match store.orders(&destination, &sample, now, freshness)? {
                    Lookup::Orders(orders) => {
                        for order in orders {
                            let key = MachineOrderKey {
                                destination: destination.clone(),
                                order: order.id.clone(),
                                machine: target.clone(),
                            };
                            if store.machine_order(&key)?.is_none() {
                                found.push(order);
                            }
                        }
                    }
                    Lookup::NoOrders => {}
                    Lookup::NotTrusted => return Err(analyte_store::Error::Conflict),
                }
            } else {
                let mut after = String::new();
                for page in 0..bound {
                    if Instant::now() >= until {
                        return Err(analyte_store::Error::Timeout);
                    }
                    match store.pending_orders(&destination, &target, &after, now, freshness)? {
                        PendingOrders::NotTrusted => {
                            return Err(analyte_store::Error::Conflict);
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
                                return Err(analyte_store::Error::InvalidHostInput);
                            }
                            found.extend(orders);
                            #[cfg(feature = "test-hooks")]
                            if let Some(gate) = &pages_gate {
                                gate.pause()?;
                            }
                            match cursor {
                                Some(cursor) => after = cursor.as_str().to_owned(),
                                None => return Ok(found),
                            }
                        }
                    }
                    if page.checked_add(1) == Some(bound) {
                        return Err(analyte_store::Error::InvalidHostInput);
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
                        (ItemDecision::Accepted, Code::LawOk.as_str().to_owned())
                    }
                    Some(law::AwosOutcome::UnableToAccept) => {
                        (ItemDecision::Rejected, Code::LawUa.as_str().to_owned())
                    }
                    Some(law::AwosOutcome::CancelAccepted) => (
                        ItemDecision::CancelConfirmed,
                        Code::LawCr.as_str().to_owned(),
                    ),
                    Some(law::AwosOutcome::UnableToCancel) => {
                        (ItemDecision::CancelRefused, Code::LawUc.as_str().to_owned())
                    }
                    None => (
                        ItemDecision::Uncertain,
                        Code::LawOrlItemMissing.as_str().to_owned(),
                    ),
                }
            }
            Ok(response) => (
                ItemDecision::Uncertain,
                crate::orders::outcome::Detail::LawApplication(response.acceptance()).to_string(),
            ),
            Err(error) => (
                ItemDecision::Uncertain,
                crate::orders::outcome::Detail::LawInvalidOrl(error.to_string()).to_string(),
            ),
        };
        let code = if let Ok(response) = response {
            response
                .decisions()
                .iter()
                .find(|d| d.id.raw() == expected.as_bytes())
                .map_or(code.clone(), |d| {
                    format!(
                        "{code}:{}",
                        match d.status {
                            law::OrderStatus::Scheduled => "SC",
                            law::OrderStatus::InProgress => "IP",
                            law::OrderStatus::Complete => "CM",
                            law::OrderStatus::Cancelled => "CA",
                        }
                    )
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
