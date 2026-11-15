//! Query-triggered LAW work, TF-2b 3.27/3.28; shared cache, reservations and item outcomes.
mod work;
use crate::{Error, config::Machine, deadline, evidence, invalid};
use analyte_hl7::{AckCode, AckError, Message, Options, Responsibility, law};
use analyte_model::{Identifier, OrderState, ReceiptId, SampleId, SourceText};
use analyte_store::{
    Handle, ItemDecision, ItemOutcome, Lookup, MachineOrder, MachineOrderKey, OrderItems,
    OrderReservation, PendingOrders,
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use work::{Command, address, render};

pub(crate) struct Connection {
    owner: Identifier,
    reserving: Vec<MachineOrderKey>,
    exchange: Option<Exchange>,
}

/// A positive availability response always owns its required LAB-28 followup.
pub(crate) enum Reply {
    Response(Message),
    Followup { response: Message, work: Message },
}

struct Exchange {
    records: Vec<MachineOrder>,
    sent: Message,
    until: Instant,
    query: ReceiptId,
}

impl Connection {
    pub(crate) fn new() -> Result<Self, Error> {
        Ok(Self {
            owner: Identifier::new(&uuid::Uuid::now_v7().to_string())?,
            reserving: Vec::new(),
            exchange: None,
        })
    }
    pub(crate) fn due(&self) -> Option<Instant> {
        self.exchange.as_ref().map(|exchange| exchange.until)
    }
    pub(crate) async fn tick(&mut self, store: &Handle) -> Result<(), Error> {
        if self.due().is_some_and(|until| Instant::now() >= until) {
            self.close(store, "LAW-ORL-timeout").await?;
        }
        Ok(())
    }
    pub(crate) fn issued(&mut self, wait: Duration) -> Result<(), Error> {
        if let Some(exchange) = &mut self.exchange {
            exchange.until = Instant::now()
                .checked_add(wait)
                .ok_or("LAW ORL deadline overflow")?;
        }
        Ok(())
    }
    /// Cancellation can leave an owned reservation committed. Cleanup queries every key
    /// registered before submission; the store owner serializes cleanup after that commit.
    pub(crate) async fn close(&mut self, store: &Handle, code: &str) -> Result<(), Error> {
        for key in &self.reserving {
            let key = key.clone();
            let owner = self.owner.clone();
            let code = SourceText::new(code)?;
            store
                .request(deadline(), move |store| {
                    if let Some(record) = store.machine_order(&key)?
                        && record.state == OrderState::Sending
                        && record.reserved_by(&owner)
                    {
                        let outcomes = record
                            .items
                            .iter()
                            .map(|item| ItemOutcome {
                                index: item.index,
                                decision: ItemDecision::Uncertain,
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
                .await?;
        }
        self.reserving.clear();
        self.exchange = None;
        Ok(())
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
                evidence::failed_reading(store, machine, receipt, &error.to_string()).await?;
                return Err(error.into());
            }
        };
        let settings = machine
            .snapshot
            .effective()
            .machine()
            .law
            .as_ref()
            .ok_or("LAW policy absent")?;
        let supported = match query.scope() {
            law::Scope::Container(_) => settings
                .query_forms
                .contains(&analyte_config::LawQueryForm::Container),
            law::Scope::AllPending => settings
                .query_forms
                .contains(&analyte_config::LawQueryForm::AllPending),
            law::Scope::Rack { .. }
            | law::Scope::Tray { .. }
            | law::Scope::Isolate { .. }
            | law::Scope::InformationalRack { .. } => false,
        };
        if !supported {
            evidence::failed_reading(
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
        if settings.work.is_none() {
            evidence::failed_reading(store, machine, receipt, "LAW query work layout is disabled")
                .await?;
            return reply(
                query,
                law::CacheAnswer::Unavailable(AckError::unlocated(b"207", b"LAW-LAB27-disabled")),
                options,
            );
        }
        if self.exchange.is_some() {
            evidence::failed_reading(
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
            .checked_add(Duration::from_millis(u64::from(settings.query_deadline_ms)))
            .ok_or("LAW query deadline overflow")?;
        match self
            .prepare(store, machine, query, receipt, until, options)
            .await
        {
            Ok(messages) => Ok(messages),
            Err(error) => {
                self.close(store, "LAW-query-preparation-failed").await?;
                let reason = error
                    .source()
                    .map_or_else(|| error.to_string(), |cause| format!("{error}: {cause}"));
                evidence::failed_reading(store, machine, receipt, &reason).await?;
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
        let cfg = machine.snapshot.effective().machine();
        let work = Arc::new(
            cfg.law
                .as_ref()
                .and_then(|law| law.work.as_ref())
                .ok_or("LAW work layout absent")?
                .clone(),
        );
        let settings = Arc::new(
            cfg.orders
                .as_ref()
                .ok_or("LAW order mappings absent")?
                .clone(),
        );
        let sample = match query.scope() {
            law::Scope::Container(id) => {
                let mut parts = id.raw().split(|byte| *byte == b'^');
                let value = parts.next().ok_or("LAW container ID missing")?;
                let namespace = parts.next().unwrap_or_default();
                if (!namespace.is_empty()
                    && namespace != work.container_namespace.as_str().as_bytes())
                    || parts.any(|part| !part.is_empty())
                {
                    return Err(invalid(
                        "LAW container assigning authority outside configured SampleId binding",
                    ));
                }
                Some(SampleId::new(std::str::from_utf8(value)?)?)
            }
            law::Scope::AllPending => None,
            law::Scope::Rack { .. }
            | law::Scope::Tray { .. }
            | law::Scope::Isolate { .. }
            | law::Scope::InformationalRack { .. } => {
                return Err(invalid("unsupported LAW query scope"));
            }
        };
        let destination = super::destination(machine);
        let target = machine.id.clone();
        let bound = options.limits.segments;
        let cached = store
            .request(until, move |store| {
                let now = jiff::Timestamp::now().as_second();
                let mut found = Vec::new();
                if let Some(sample) = sample {
                    match store.orders(&destination, &sample, now, super::FRESHNESS_SECONDS)? {
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
                        match store.pending_orders(
                            &destination,
                            &target,
                            &after,
                            now,
                            super::FRESHNESS_SECONDS,
                        )? {
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
                                    return Err(analyte_store::Error::InvalidInput);
                                }
                                found.extend(orders);
                                match cursor {
                                    Some(cursor) => after = cursor.as_str().to_owned(),
                                    None => return Ok(found),
                                }
                            }
                        }
                        if page.checked_add(1) == Some(bound) {
                            return Err(analyte_store::Error::InvalidInput);
                        }
                    }
                }
                Ok(found)
            })
            .await?;
        let mut prepared = Vec::new();
        let peer = cfg.law.as_ref().ok_or("LAW")?.peer.capabilities();
        for cached in cached {
            let app: analyte_contract::Order =
                analyte_contract::decode(cached.document.as_bytes())?;
            if !super::eligible(&app, machine) {
                continue;
            }
            if Instant::now() >= until {
                return Err(invalid("LAW query deadline before reservation"));
            }
            let command = Command {
                app: super::app_order(&app)?,
                settings: settings.clone(),
                work: work.clone(),
                priority: peer.priority,
                specimen: peer.specimen,
            };
            let key = MachineOrderKey {
                destination: super::destination(machine),
                order: cached.id,
                machine: machine.id.clone(),
            };
            let version = app.fields().version;
            let identity = key.clone();
            let owner = self.owner.clone();
            let namespace = settings
                .placer_namespace
                .clone()
                .ok_or("LAW placer namespace absent")?;
            self.reserving.push(key);
            let reserved = store
                .request(until, move |store| {
                    store.reserve_send_items_with(
                        &OrderReservation {
                            identity: &identity,
                            version,
                            now: jiff::Timestamp::now().as_second(),
                            freshness: super::FRESHNESS_SECONDS,
                        },
                        OrderItems::PerTest {
                            namespace: &namespace,
                            identity_bytes: 50,
                        },
                        &owner,
                        |record| {
                            render(&[(&command, record)], options)
                                .map_err(analyte_store::Error::DeliveryBuild)?;
                            Ok(command)
                        },
                    )
                })
                .await?
                .map_err(std::io::Error::other)?;
            prepared.push((reserved.1, reserved.0));
        }
        if Instant::now() >= until {
            return Err(invalid("LAW query deadline after reservation"));
        }
        let followup = if prepared.is_empty() {
            response_metadata(|response| {
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
            render(&refs, options)?
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
        self.exchange = Some(Exchange {
            records: prepared.into_iter().map(|(_, record)| record).collect(),
            sent: followup,
            until,
            query: receipt,
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
            evidence::failed_reading(store, machine, receipt, "LAW ORL has no pending followup")
                .await?;
            return Ok(());
        };
        if Instant::now() >= exchange.until {
            self.close(store, "LAW-ORL-late").await?;
            evidence::failed_reading(
                store,
                machine,
                receipt,
                "LAW ORL arrived after response deadline",
            )
            .await?;
            return Ok(());
        }
        let response = law::receive_work_response(message, &exchange.sent);
        if let Err(error) = &response {
            evidence::failed_reading(store, machine, receipt, &error.to_string()).await?;
        }
        if exchange.records.is_empty() {
            let query = exchange.query;
            let correlated = response.is_ok();
            let code = response
                .as_ref()
                .map(|r| format!("{:?}", r.acceptance()))
                .unwrap_or_else(|_| "invalid".into());
            let actor = machine.id.clone();
            store
                .request(deadline(), move |store| {
                    store.audit(&analyte_store::Audit {
                        actor,
                        action: Identifier::new("LAW-negative-query-ORL")
                            .map_err(|_| analyte_store::Error::InvalidInput)?,
                        targets: vec![
                            Identifier::new(&uuid::Uuid::from(receipt).to_string())
                                .map_err(|_| analyte_store::Error::InvalidInput)?,
                            Identifier::new(&uuid::Uuid::from(query).to_string())
                                .map_err(|_| analyte_store::Error::InvalidInput)?,
                        ],
                        time: jiff::Timestamp::now().as_second(),
                        reason: analyte_model::Reason::new(&format!(
                            "negative followup ORL correlated={correlated} acceptance={code}"
                        ))
                        .map_err(|_| analyte_store::Error::InvalidInput)?,
                    })
                })
                .await?;
        }
        for record in &exchange.records {
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
                                (ItemDecision::Accepted, "LAW-OK".to_owned())
                            }
                            Some(law::AwosOutcome::UnableToAccept) => {
                                (ItemDecision::Rejected, "LAW-UA".to_owned())
                            }
                            Some(
                                law::AwosOutcome::CancelAccepted | law::AwosOutcome::UnableToCancel,
                            ) => (
                                ItemDecision::Uncertain,
                                "LAW-ORL-unexpected-cancel".to_owned(),
                            ),
                            None => (ItemDecision::Uncertain, "LAW-ORL-item-missing".to_owned()),
                        }
                    }
                    Ok(response) => (
                        ItemDecision::Uncertain,
                        format!("LAW-ORL-application-{:?}", response.acceptance()),
                    ),
                    Err(error) => (ItemDecision::Uncertain, format!("LAW-invalid-ORL:{error}")),
                };
                outcomes.push(ItemOutcome {
                    index: item.index,
                    decision,
                    code: SourceText::new(&code)?,
                });
            }
            let id = record.id;
            let key = record.key.clone();
            store
                .request(deadline(), move |store| {
                    if store
                        .machine_order(&key)?
                        .is_some_and(|current| current.state == OrderState::Sending)
                    {
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
    response_metadata(|response| {
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

pub(crate) fn response_metadata<T>(
    build: impl FnOnce(analyte_hl7::Response<'_>) -> Result<T, Error>,
) -> Result<T, Error> {
    let id = uuid::Uuid::now_v7().simple().to_string();
    let timestamp = jiff::Timestamp::now()
        .strftime("%Y%m%d%H%M%S%z")
        .to_string();
    build(analyte_hl7::Response {
        control_id: law::control_id(id.as_bytes())?,
        timestamp: timestamp.as_bytes(),
    })
}
