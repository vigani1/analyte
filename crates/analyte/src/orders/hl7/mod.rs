//! Drives HL7 order lookups, reservations and delivery outcomes (HL7 2.3.1 §2.18 and 2.5.1 §5.10).
//! Kept whole: HL7 order exchange driving keeps cancellation, peer replies and durable transitions together.
#[cfg(test)]
use analyte_fixtures::ConfigEdit as _;
pub(crate) mod response;
mod send;
use crate::orders::OrderError as Error;
use crate::orders::outcome::Hl7End;
use crate::{deadline, installation::config::Machine, intake};
use analyte_hl7::{
    Message, Options,
    query::{self, CacheAnswer},
};
use analyte_model::{ReceiptId, SampleId};
use analyte_store::{Handle, Lookup, MachineOrder};
use std::time::{Duration, Instant};

/// Owns order reservations; framing, issuance, DSR correlation and reply waits belong to
/// the dialog. Cancellation leaves reservations owned here for disconnect cleanup; restart
/// recovery owns any store call whose completion was lost during cancellation.
pub(crate) struct Hl7OrderLink {
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
/// One reserved order for the dialog's `Send` event, with its response budget.
pub(crate) struct Outbound<'a> {
    pub(crate) message: &'a [u8],
    pub(crate) wait: Duration,
}
impl Hl7OrderLink {
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
    /// Reserves the next order while the dialog is idle. The returned message is sent
    /// once; the exchange stays owned here until its response, timeout or disconnect.
    pub(crate) async fn next(
        &mut self,
        store: &Handle,
        machine: &intake::ConnectionEvidence<'_>,
        dialog: &analyte_dialog::hl7::Dialog,
        stack: &analyte_config::Hl7Stack,
    ) -> Result<Option<Outbound<'_>>, Error> {
        if dialog.orders_busy() {
            return Ok(None);
        }
        let Some(exchange) = send::next(
            store,
            machine.machine(),
            stack,
            &self.owner,
            self.options,
            &mut self.cursor,
            &mut self.reserving,
        )
        .await?
        else {
            return Ok(None);
        };
        let wait = stack
            .orders()
            .and_then(|orders| orders.send())
            .ok_or(Error::Hl7SendSettingsMissing)?
            .settings()
            .response_ms;
        self.reserving.clear();
        let exchange = self.exchange.insert(exchange);
        Ok(Some(Outbound {
            message: exchange.message.message().raw(),
            wait: Duration::from_millis(u64::from(wait)),
        }))
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
                Hl7End::OrderResponseTimeout.as_str(),
            )
            .await?;
        }
        self.exchange = None;
        Ok(())
    }
    pub(crate) async fn close(
        &mut self,
        store: &Handle,
        _machine: &intake::ConnectionEvidence<'_>,
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
        crate::cleanup::retain_cleanup(&mut result, cleanup);
        if let Some(exchange) = &self.exchange {
            let cleanup = send::finish(
                store,
                exchange,
                None,
                if code == Hl7End::DsrDisconnect.as_str() {
                    Hl7End::OrderDisconnect.as_str()
                } else {
                    Hl7End::OrderSessionError.as_str()
                },
            )
            .await;
            crate::cleanup::retain_cleanup(&mut result, cleanup);
        }
        self.exchange = None;
        let cleanup = self.outcome(store, code, false).await;
        crate::cleanup::retain_cleanup(&mut result, cleanup);
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
                .request::<_, analyte_store::OrderError>(deadline()?, move |store| {
                    let current = store
                        .machine_order(&identity)?
                        .ok_or(analyte_store::OrderError::Corruption)?;
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
                    Ok::<_, analyte_store::OrderError>(())
                })
                .await
                .map_err(Into::into);
            crate::cleanup::retain_cleanup(&mut result, cleanup);
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
        let settings =
            crate::orders::settings::hl7(machine).ok_or_else(|| Error::Hl7OrdersMissing)?;
        let query = settings
            .query()
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
                        match crate::orders::admission::prepare(
                            store,
                            machine,
                            cached,
                            until,
                            |error| {
                                crate::orders::outcome::Hl7Detail::DisplayMapping(error.to_string())
                                    .to_string()
                            },
                            |order| {
                                let app = crate::orders::app_order(order)?;
                                Ok(analyte_mapping::map_display(
                                    &app,
                                    order.fields().id.as_str(),
                                    settings,
                                    machine.snapshot.effective().stack().limits(),
                                )?)
                            },
                        )
                        .await?
                        {
                            crate::orders::admission::Prepared::Skipped => continue,
                            crate::orders::admission::Prepared::Mapped(candidate, mapped) => {
                                if lines
                                    .len()
                                    .checked_add(mapped.len())
                                    .is_none_or(|n| n > self.options.limits.segments)
                                {
                                    answer = CacheAnswer::Unavailable;
                                    break;
                                }
                                lines.extend(mapped);
                                candidates.push(candidate);
                                answer = CacheAnswer::Found;
                            }
                            crate::orders::admission::Prepared::Rejected(_) => {
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
            {
                let mut limits = analyte_fixtures::store_limits();
                limits.busy_timeout = Duration::from_millis(20);
                limits
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
            .request::<_, analyte_store::OrderError>(deadline()?, move |store| {
                Ok::<_, analyte_store::OrderError>((
                    store.machine_order(&first)?,
                    store.machine_order(&second)?,
                ))
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
            {
                let mut limits = analyte_fixtures::store_limits();
                limits.busy_timeout = Duration::from_millis(20);
                limits
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
        let options =
            crate::machine::hl7::settings::effective_options(machine.snapshot.effective())
                .ok_or("options")?;
        let mut connection = Hl7OrderLink::new(options)?;
        connection.owner = owner;
        connection.active = vec![failed, good];
        let mut result = Err(crate::invalid("original wire failure"));
        crate::cleanup::retain_cleanup(
            &mut result,
            connection
                .close(
                    &store,
                    &intake::ConnectionEvidence::new(&machine)?,
                    Hl7End::DsrSessionError.as_str(),
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
            {
                let mut limits = analyte_fixtures::store_limits();
                limits.busy_timeout = Duration::from_millis(20);
                limits
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
        let options =
            crate::machine::hl7::settings::effective_options(machine.snapshot.effective())
                .ok_or("options")?;
        let mut other = Hl7OrderLink::new(options)?;
        other.reserving.push(key.clone());
        other
            .close(
                &store,
                &intake::ConnectionEvidence::new(&machine)?,
                Hl7End::DsrDisconnect.as_str(),
            )
            .await?;
        let identity = key.clone();
        let current = store
            .request(deadline()?, move |store| store.machine_order(&identity))
            .await?
            .ok_or("reservation")?;
        assert_eq!(current.state, analyte_model::OrderState::Sending);
        assert!(current.reserved_by(&owner));
        let mut original = Hl7OrderLink::new(options)?;
        original.owner = owner;
        original.reserving.push(key.clone());
        original
            .close(
                &store,
                &intake::ConnectionEvidence::new(&machine)?,
                Hl7End::DsrDisconnect.as_str(),
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
            destination_limits: crate::runtime::delivery::limits(
                &snapshot.effective().site().destination,
            ),

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
            .request::<_, analyte_store::OrderError>(deadline()?, move |store| {
                let now = jiff::Timestamp::now().as_second();
                store.order_page(&analyte_store::OrderPage {
                    destination: identity.destination.clone(),
                    cursor: Identifier::new("page").map_err(|cause| {
                        analyte_store::OrderError::Storage(analyte_store::StorageError::Text(cause))
                    })?,
                    complete: true,
                    polled: now,
                    orders: vec![analyte_store::CachedOrder {
                        id: identity.order.clone(),
                        sample: SampleId::new("000Ab12").map_err(|cause| {
                            analyte_store::OrderError::Storage(analyte_store::StorageError::Text(
                                cause,
                            ))
                        })?,
                        document: include_str!("../../../../../docs/contract/vectors/order.json")
                            .rewrite("order-1", identity.order.as_str()),
                        tombstone: false,
                        expiry: now.checked_add(100).ok_or_else(|| {
                            std::io::Error::other("order expiry timestamp overflow")
                        })?,
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
                    .map_err(|_| analyte_store::OrderError::Conflict)
            })
            .await?;
        Ok(())
    }
}
