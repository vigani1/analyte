//! Drives HL7 order lookups, reservations and delivery outcomes (HL7 2.3.1 §2.18 and 2.5.1 §5.10).
//! Kept whole: HL7 order exchange driving keeps cancellation, peer replies and durable transitions together.
pub(crate) mod response;
mod send;
use crate::orders::OrderError as Error;
use crate::orders::outcome::Code;
use crate::{deadline, installation::config::Machine, intake};
use analyte_framing::hl7 as mllp;
use analyte_hl7::{
    Message, Options,
    query::{self, CacheAnswer},
};
use analyte_model::{ReceiptId, SampleId};
use analyte_store::{Handle, Lookup, MachineOrder};
use std::time::{Duration, Instant};

/// Owns order reservations; framing, DSR correlation and reply waits belong to the dialog.
/// Cancellation leaves reservations owned here for disconnect cleanup; restart
/// recovery owns any store call whose completion was lost during cancellation.
pub(crate) struct Connection {
    options: Options,
    active: Vec<MachineOrder>,
    reserving: Vec<analyte_store::MachineOrderKey>,
    owner: analyte_model::Identifier,
    exchange: Option<send::Exchange>,
    cursor: String,
}
pub(crate) struct Request<'a> {
    pub(crate) message: &'a Message,
    pub(crate) started: Instant,
    pub(crate) receipt: ReceiptId,
}
impl Connection {
    pub(crate) fn new(options: Options) -> Result<Self, Error> {
        Ok(Self {
            options,
            active: Vec::new(),
            reserving: Vec::new(),
            owner: analyte_model::Identifier::new(&uuid::Uuid::now_v7().to_string())?,
            exchange: None,
            cursor: String::new(),
        })
    }
    pub(crate) async fn idle(
        &mut self,
        store: &Handle,
        machine: &intake::Connection<'_>,
        conn: &mut analyte_transport::Connection,
        dialog: &mut analyte_dialog::hl7::Dialog,
        clock: Instant,
    ) -> Result<(), Error> {
        if dialog.orders_busy() {
            return Ok(());
        }
        if let Some(exchange) = send::next(
            store,
            machine,
            &self.owner,
            self.options,
            &mut self.cursor,
            &mut self.reserving,
        )
        .await?
        {
            let wait = crate::orders::settings::hl7(machine)
                .and_then(|orders| orders.send().as_ref())
                .ok_or(Error::SendSettings)?
                .settings()
                .response_ms;
            let until = Instant::now()
                .checked_add(Duration::from_millis(u64::from(wait)))
                .ok_or(Error::Hl7ResponseDeadlineOverflow)?;
            self.exchange = Some(exchange);
            dialog.await_order_response(clock.elapsed(), Duration::from_millis(u64::from(wait)))?;
            self.reserving.clear();
            let message = self
                .exchange
                .as_ref()
                .ok_or(Error::ExchangeAbsent)?
                .message
                .message();
            write(conn, message.raw(), self.options, until).await?;
        }
        Ok(())
    }
    pub(crate) async fn order_response(
        &mut self,
        store: &Handle,
        message: Option<&Message>,
    ) -> Result<(), Error> {
        if let Some(exchange) = &self.exchange {
            send::finish(
                store,
                exchange,
                message,
                Code::Hl7OrderResponseTimeout.as_str(),
            )
            .await?;
        }
        self.exchange = None;
        Ok(())
    }
    pub(crate) async fn close(
        &mut self,
        store: &Handle,
        _machine: &intake::Connection<'_>,
        code: &str,
    ) -> Result<(), Error> {
        let mut result = Ok(());
        let cleanup = crate::orders::sending::settle_owned(
            store,
            std::mem::take(&mut self.reserving),
            &self.owner,
            code,
            crate::orders::sending::Settlement::Order,
        )
        .await;
        crate::machine::custody::retain_cleanup(&mut result, cleanup);
        if let Some(exchange) = &self.exchange {
            let cleanup = send::finish(
                store,
                exchange,
                None,
                if code == Code::Hl7DsrDisconnect.as_str() {
                    Code::Hl7OrderDisconnect.as_str()
                } else {
                    Code::Hl7OrderSessionError.as_str()
                },
            )
            .await;
            crate::machine::custody::retain_cleanup(&mut result, cleanup);
        }
        self.exchange = None;
        let cleanup = self.outcome(store, code, false).await;
        crate::machine::custody::retain_cleanup(&mut result, cleanup);
        result
    }
    pub(crate) async fn outcome(
        &mut self,
        store: &Handle,
        code: &str,
        sent: bool,
    ) -> Result<(), Error> {
        let mut result = Ok(());
        for order in self.active.drain(..) {
            let identity = order.key.clone();
            let code = analyte_model::SourceText::new(code)?;
            let cleanup = store
                .request(deadline()?, move |store| {
                    let current = store
                        .machine_order(&identity)?
                        .ok_or(analyte_store::Error::Corruption)?;
                    // A cancelled wait may have committed its outcome. Do not emit a
                    // second event or turn an already confirmed Sent back into uncertainty.
                    if current.state == analyte_model::OrderState::CancelRequested {
                        let outcomes = current
                            .items
                            .iter()
                            .map(|item| analyte_store::ItemOutcome {
                                index: item.index,
                                decision: analyte_store::ItemDecision::Uncertain,
                                code: code.clone(),
                            })
                            .collect::<Vec<_>>();
                        store.finish_order_items(
                            current.id,
                            &outcomes,
                            jiff::Timestamp::now().as_second(),
                            crate::orders::event,
                        )?;
                    } else if current.state == analyte_model::OrderState::Sending {
                        store.transition_order(
                            current.id,
                            if sent {
                                analyte_model::OrderState::Sent
                            } else {
                                analyte_model::OrderState::Uncertain
                            },
                            code,
                            jiff::Timestamp::now().as_second(),
                            crate::orders::event,
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
    pub(crate) async fn answer(
        &mut self,
        store: &Handle,
        machine: &Machine,
        request: Request<'_>,
        dialog: &analyte_dialog::hl7::Dialog,
        clock: Instant,
    ) -> Result<Option<analyte_dialog::hl7::DisplayReply>, Error> {
        let Request {
            message,
            started,
            receipt,
        } = request;
        let request = match query::Request::new(message) {
            Ok(request) => request,
            Err(error) => {
                intake::failed_reading(store, machine, receipt, &error.to_string()).await?;
                return Ok(None);
            }
        };
        let settings =
            crate::orders::settings::hl7(machine).ok_or_else(|| Error::Hl7OrdersMissing)?;
        let query = settings
            .query()
            .as_ref()
            .ok_or_else(|| Error::Hl7QueryPolicyMissing)?;
        let policy = &query.policy();
        let layout = &query.layout();
        let until = started
            .checked_add(Duration::from_millis(u64::from(policy.deadline_ms)))
            .ok_or_else(|| Error::Hl7QueryDeadlineOverflow)?;
        let (mut answer, lines, candidates) = self
            .lookup_display(
                store,
                machine,
                request,
                receipt,
                until,
                dialog.orders_busy(),
            )
            .await?;
        if Instant::now() >= until {
            answer = CacheAnswer::Unavailable;
        }
        let qck_metadata = response::response_metadata(response::Profile::Hl7);
        let mut dsr_metadata = response::response_metadata(response::Profile::Hl7);
        dsr_metadata.timestamp.clone_from(&qck_metadata.timestamp);
        let mut dsr = None;
        if answer == CacheAnswer::Found {
            let refs = lines.iter().map(Vec::as_slice).collect::<Vec<_>>();
            match query::display(
                request,
                dsr_metadata.response()?,
                qck_metadata.response()?.control_id,
                &refs,
                self.options,
            ) {
                Ok(message) => dsr = Some(message),
                Err(_) => answer = CacheAnswer::Unavailable,
            }
        }
        if answer == CacheAnswer::Found {
            self.reserving = candidates
                .iter()
                .map(|candidate| candidate.identity.clone())
                .collect();
            if !crate::orders::sending::reserve_candidates(
                store,
                candidates,
                until,
                &mut self.active,
                Some(self.owner.clone()),
                machine.destination_limits.freshness.as_secs(),
            )
            .await?
            {
                answer = CacheAnswer::Unavailable;
                dsr = None;
            }
            self.reserving.clear();
        }
        let qck = query::acknowledgment(request, qck_metadata.response()?, answer, self.options)?;
        Ok(Some(analyte_dialog::hl7::DisplayReply {
            acknowledgment: qck,
            display: dsr,
            query_deadline: until.duration_since(clock),
            failure_deadline: deadline()?.duration_since(clock),
            acknowledgment_wait: Duration::from_millis(u64::from(layout.acknowledgment_ms)),
        }))
    }
    async fn lookup_display(
        &self,
        store: &Handle,
        machine: &Machine,
        request: query::Request<'_>,
        receipt: ReceiptId,
        until: Instant,
        busy: bool,
    ) -> Result<(CacheAnswer, Vec<Vec<u8>>, Vec<crate::orders::Candidate>), Error> {
        let freshness = machine.destination_limits.freshness.as_secs();
        let settings =
            crate::orders::settings::hl7(machine).ok_or_else(|| Error::Hl7OrdersMissing)?;
        let query = settings
            .query()
            .as_ref()
            .ok_or_else(|| Error::Hl7QueryPolicyMissing)?;
        let layout = &query.layout();
        let mut answer = CacheAnswer::Unavailable;
        let mut lines = Vec::new();
        let mut candidates = Vec::new();
        let selected = select_sample(store, machine, request, receipt, layout).await?;
        if !busy
            && Instant::now() < until
            && let Some(sample) = selected
        {
            let lookup = match crate::orders::lookup(store, machine, sample, until).await {
                Ok(answer) => answer,
                Err(error) => {
                    tracing::error!(error_kind=%error,"HL7 order cache unavailable after raw query storage");
                    Lookup::NotTrusted
                }
            };
            match lookup {
                Lookup::NoOrders => answer = CacheAnswer::Empty,
                Lookup::NotTrusted => {}
                Lookup::Orders(orders) => {
                    answer = CacheAnswer::Empty;
                    for cached in orders {
                        let order: analyte_contract::Order =
                            analyte_contract::decode(cached.document.as_bytes())?;
                        let fields = order.fields();
                        if !crate::orders::eligible(&order, machine) {
                            continue;
                        }
                        let mapped = crate::orders::app_order(&order)
                            .map_err(Error::from)
                            .and_then(|app| {
                                Ok(analyte_mapping::map_display(
                                    &app,
                                    fields.id.as_str(),
                                    &machine.snapshot,
                                )?)
                            });
                        match mapped {
                            Ok(mapped) => {
                                if lines
                                    .len()
                                    .checked_add(mapped.len())
                                    .is_none_or(|n| n > self.options.limits.segments)
                                {
                                    answer = CacheAnswer::Unavailable;
                                    break;
                                }
                                lines.extend(mapped);
                                candidates.push(crate::orders::Candidate {
                                    identity: crate::orders::reservation::key(machine, cached.id),
                                    version: fields.version,
                                });
                                answer = CacheAnswer::Found;
                            }
                            Err(error) => {
                                let identity = crate::orders::reservation::key(machine, cached.id);
                                let version = fields.version;
                                let reason = analyte_model::SourceText::new(
                                    &crate::orders::outcome::Detail::Hl7DisplayMapping(
                                        error.to_string(),
                                    )
                                    .to_string(),
                                )?;
                                let rejected = crate::orders::reservation::reject(
                                    store, until, identity, version, freshness, reason,
                                )
                                .await?;
                                if let Err(refusal) = rejected {
                                    tracing::info!(reason=%refusal,"HL7 mapping rejection superseded");
                                }
                                answer = CacheAnswer::Unavailable;
                                break;
                            }
                        }
                    }
                }
            }
        }
        Ok((answer, lines, candidates))
    }
}

fn sample(
    request: query::Request<'_>,
    layout: &analyte_config::Hl7OrderQuery,
) -> Result<Option<SampleId>, Error> {
    let filters = layout
        .filters
        .iter()
        .map(|filter| (filter.field, filter.value.as_bytes()))
        .collect::<Vec<_>>();
    query::specimen(
        request,
        &query::SpecimenSelector {
            field: layout.sample_field,
            suffix: layout.sample_suffix.as_bytes(),
            filters: &filters,
        },
    )?
    .map(|sample| SampleId::new(&sample))
    .transpose()
    .map_err(Into::into)
}
async fn write(
    conn: &mut analyte_transport::Connection,
    payload: &[u8],
    options: Options,
    until: Instant,
) -> Result<(), Error> {
    let wire = mllp::encode(payload, mllp::Limit::new(options.limits.message_bytes)?)?;
    conn.write_all(&wire, Some(tokio::time::Instant::from_std(until)))
        .await?;
    Ok(())
}

async fn select_sample(
    store: &Handle,
    machine: &Machine,
    request: query::Request<'_>,
    receipt: ReceiptId,
    layout: &analyte_config::Hl7OrderQuery,
) -> Result<Option<SampleId>, Error> {
    let selected = match sample(request, layout) {
        Ok(Some(sample)) => Some(sample),
        Ok(None) => {
            intake::failed_reading(
                store,
                machine,
                receipt,
                "HL7 query selectors are outside the configured scope",
            )
            .await?;
            None
        }
        Err(error) => {
            intake::failed_reading(store, machine, receipt, &error.to_string()).await?;
            None
        }
    };
    Ok(selected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use analyte_fixtures::UnlimitedSpace as Space;
    use analyte_model::Identifier;

    #[tokio::test]
    async fn adr0003_shared_cleanup_settles_later_key_after_store_failure()
    -> Result<(), crate::Error> {
        let machine = query_machine()?;
        let temp = tempfile::tempdir()?;
        let mut store = Handle::start(
            temp.path().join("owned-cleanup.db"),
            analyte_store::Limits {
                reserve_bytes: 1024,
                database_bytes: 100_000_000,
                record_bytes: 65_536,
                page_rows: 100,
                busy_timeout: Duration::from_millis(20),
            },
            Box::new(Space),
            4,
            crate::test_startup_deadline()?,
        )
        .await?;
        let owner = Identifier::new("owner")?;
        let first = crate::orders::reservation::key(&machine, Identifier::new("order-2")?);
        let second = crate::orders::reservation::key(&machine, Identifier::new("order-1")?);
        reserve_original_connection(&store, &first, &owner).await?;
        reserve_original_connection(&store, &second, &owner).await?;
        let database = rusqlite::Connection::open(temp.path().join("owned-cleanup.db"))?;
        database.execute_batch("CREATE TRIGGER synthetic_cleanup_failure BEFORE UPDATE OF state ON machine_orders WHEN OLD.order_id='order-2' BEGIN SELECT RAISE(ABORT,'synthetic cleanup failure'); END;")?;
        let result = crate::orders::sending::settle_owned(
            &store,
            vec![first.clone(), second.clone()],
            &owner,
            "disconnect",
            crate::orders::sending::Settlement::Items,
        )
        .await;
        assert!(result.is_err());
        let (first, second) = store
            .request(deadline()?, move |store| {
                Ok((store.machine_order(&first)?, store.machine_order(&second)?))
            })
            .await?;
        assert_eq!(
            first.ok_or("first")?.state,
            analyte_model::OrderState::Sending
        );
        assert_eq!(
            second.ok_or("second")?.state,
            analyte_model::OrderState::Uncertain
        );
        drop(database);
        store.shutdown(deadline()?).await?;
        Ok(())
    }

    #[tokio::test]
    async fn rule8_cleanup_preserves_session_error_and_attempts_every_reservation()
    -> Result<(), crate::Error> {
        let machine = query_machine()?;
        let temp = tempfile::tempdir()?;
        let mut store = Handle::start(
            temp.path().join("cleanup-failure.db"),
            analyte_store::Limits {
                reserve_bytes: 1024,
                database_bytes: 100_000_000,
                record_bytes: 65_536,
                page_rows: 100,
                busy_timeout: Duration::from_millis(20),
            },
            Box::new(Space),
            4,
            crate::test_startup_deadline()?,
        )
        .await?;
        let key = analyte_store::MachineOrderKey {
            destination: crate::orders::destination(&machine),
            order: Identifier::new("order-1")?,
            machine: machine.id.clone(),
        };
        let owner = Identifier::new("cleanup-owner")?;
        reserve_original_connection(&store, &key, &owner).await?;
        let lookup = key.clone();
        let good = store
            .request(deadline()?, move |store| store.machine_order(&lookup))
            .await?
            .ok_or("order")?;
        let mut failed_key = key.clone();
        failed_key.order = Identifier::new("order-2")?;
        reserve_original_connection(&store, &failed_key, &owner).await?;
        let lookup = failed_key.clone();
        let failed = store
            .request(deadline()?, move |store| store.machine_order(&lookup))
            .await?
            .ok_or("second reservation")?;
        let database = rusqlite::Connection::open(temp.path().join("cleanup-failure.db"))?;
        database.execute_batch("CREATE TRIGGER synthetic_cleanup_failure BEFORE UPDATE OF state ON machine_orders WHEN OLD.order_id='order-2' BEGIN SELECT RAISE(ABORT,'synthetic cleanup failure'); END;")?;
        let options = machine
            .snapshot
            .effective()
            .hl7_options()
            .ok_or("options")?;
        let mut connection = Connection::new(options)?;
        connection.owner = owner;
        connection.active = vec![failed, good];
        let mut result = Err(crate::invalid("original wire failure"));
        crate::machine::custody::retain_cleanup(
            &mut result,
            connection
                .close(
                    &store,
                    &intake::Connection::new(&machine)?,
                    Code::Hl7DsrSessionError.as_str(),
                )
                .await
                .map_err(Into::into),
        );
        let error = result.expect_err("cleanup fails");
        assert!(error.to_string().contains("original wire failure"));
        assert!(error.to_string().contains("dialog effect failed"));
        assert!(error.source().is_some());
        let current = store
            .request(deadline()?, move |store| store.machine_order(&key))
            .await?
            .ok_or("reservation")?;
        assert_eq!(current.state, analyte_model::OrderState::Uncertain);
        let failed = store
            .request(deadline()?, move |store| store.machine_order(&failed_key))
            .await?
            .ok_or("failed reservation retained")?;
        assert_eq!(failed.state, analyte_model::OrderState::Sending);
        assert!(connection.active.is_empty());
        drop(database);
        store.shutdown(deadline()?).await?;
        Ok(())
    }

    #[tokio::test]
    async fn adr0003_rule8_cancelled_query_cleanup_never_claims_another_connection()
    -> Result<(), crate::Error> {
        let machine = query_machine()?;
        let temp = tempfile::tempdir()?;
        let mut store = Handle::start(
            temp.path().join("cleanup.db"),
            analyte_store::Limits {
                reserve_bytes: 1024,
                database_bytes: 100_000_000,
                record_bytes: 65_536,
                page_rows: 100,
                busy_timeout: Duration::from_millis(20),
            },
            Box::new(Space),
            4,
            crate::test_startup_deadline()?,
        )
        .await?;
        let key = analyte_store::MachineOrderKey {
            destination: crate::orders::destination(&machine),
            order: Identifier::new("order-1")?,
            machine: machine.id.clone(),
        };
        let owner = Identifier::new("original-connection")?;
        reserve_original_connection(&store, &key, &owner).await?;
        let options = machine
            .snapshot
            .effective()
            .hl7_options()
            .ok_or("options")?;
        let mut other = Connection::new(options)?;
        other.reserving.push(key.clone());
        other
            .close(
                &store,
                &intake::Connection::new(&machine)?,
                Code::Hl7DsrDisconnect.as_str(),
            )
            .await?;
        let identity = key.clone();
        let current = store
            .request(deadline()?, move |store| store.machine_order(&identity))
            .await?
            .ok_or("reservation")?;
        assert_eq!(current.state, analyte_model::OrderState::Sending);
        assert!(current.reserved_by(&owner));
        let mut original = Connection::new(options)?;
        original.owner = owner;
        original.reserving.push(key.clone());
        original
            .close(
                &store,
                &intake::Connection::new(&machine)?,
                Code::Hl7DsrDisconnect.as_str(),
            )
            .await?;
        let current = store
            .request(deadline()?, move |store| store.machine_order(&key))
            .await?
            .ok_or("reservation")?;
        assert_eq!(current.state, analyte_model::OrderState::Uncertain);
        assert_eq!(current.source_code.as_str(), "HL7-DSR-disconnect");
        store.shutdown(deadline()?).await?;
        Ok(())
    }
    fn query_machine() -> Result<Machine, crate::Error> {
        let definition = format!(
            "{}\n{}",
            include_str!("../../../../core/config/examples/hl7.toml"),
            include_str!("../../../../core/config/examples/hl7_orders.toml")
        );
        let snapshot = analyte_config::Draft::load(
            ("machine", &definition),
            (
                "site",
                include_str!("../../../../core/config/examples/site.toml"),
            ),
            ("overrides", "schema_version=1"),
        )?
        .validate()?;
        Ok(Machine {
            capture: false,
            destination_limits: crate::runtime::delivery::limits(30),
            #[cfg(feature = "test-hooks")]
            hooks: crate::installation::config::MachineHooks::default(),
            id: Identifier::new("synthetic")?,
            snapshot,
            listener: None,
        })
    }

    async fn reserve_original_connection(
        store: &Handle,
        key: &analyte_store::MachineOrderKey,
        owner: &Identifier,
    ) -> Result<(), crate::Error> {
        let identity = key.clone();
        let original_owner = owner.clone();
        store
            .request(deadline()?, move |store| {
                let now = jiff::Timestamp::now().as_second();
                store.order_page(&analyte_store::OrderPage {
                    destination: identity.destination.clone(),
                    cursor: Identifier::new("page").map_err(analyte_store::Error::Text)?,
                    complete: true,
                    polled: now,
                    orders: vec![analyte_store::CachedOrder {
                        id: identity.order.clone(),
                        sample: SampleId::new("000Ab12")
                            .map_err(|_| analyte_store::Error::InvalidHostInput)?,
                        document: include_str!("../../../../../docs/contract/vectors/order.json")
                            .replace("order-1", identity.order.as_str()),
                        tombstone: false,
                        expiry: now
                            .checked_add(100)
                            .ok_or(analyte_store::Error::InvalidHostInput)?,
                    }],
                })?;
                store
                    .reserve_send(
                        &analyte_store::OrderReservation {
                            identity: &identity,
                            version: std::num::NonZeroU64::MIN,
                            now,
                            freshness: 30,
                        },
                        analyte_store::ReserveOptions {
                            owner: Some(&original_owner),
                            ..Default::default()
                        },
                    )?
                    .map_err(|_| analyte_store::Error::Conflict)
            })
            .await?;
        Ok(())
    }
}
