//! Host-owned cache lookups, reservations and delivery outcomes; HL7 2.3.1 section 2.18 / 2.5.1 section 5.10.
use crate::{Error, deadline, evidence, installation::config::Machine, invalid};
use analyte_framing::mllp;
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
    exchange: Option<super::hl7_send::Exchange>,
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
        machine: &evidence::Connection<'_>,
        conn: &mut analyte_transport::Connection,
        dialog: &mut analyte_dialog::hl7::Dialog,
        clock: Instant,
    ) -> Result<(), Error> {
        if dialog.orders_busy() {
            return Ok(());
        }
        if let Some(exchange) = super::hl7_send::next(
            store,
            machine,
            &self.owner,
            self.options,
            &mut self.cursor,
            &mut self.reserving,
        )
        .await?
        {
            let wait = super::settings::hl7(machine)
                .and_then(|orders| orders.send().as_ref())
                .ok_or("send settings")?
                .settings()
                .response_ms;
            let until = Instant::now()
                .checked_add(Duration::from_millis(u64::from(wait)))
                .ok_or("HL7 response deadline overflow")?;
            self.exchange = Some(exchange);
            dialog.await_order_response(clock.elapsed(), Duration::from_millis(u64::from(wait)))?;
            self.reserving.clear();
            let message = self
                .exchange
                .as_ref()
                .ok_or("exchange absent")?
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
            super::hl7_send::finish(store, exchange, message, "HL7-order-response-timeout").await?;
        }
        self.exchange = None;
        Ok(())
    }
    pub(crate) async fn close(
        &mut self,
        store: &Handle,
        _machine: &evidence::Connection<'_>,
        code: &str,
    ) -> Result<(), Error> {
        for identity in self.reserving.drain(..) {
            if let Some(order) = store
                .request(deadline(), move |store| store.machine_order(&identity))
                .await?
                && (order.reserved_by(&self.owner) || order.cancel_requested_by(&self.owner))
                && !self.active.iter().any(|active| active.id == order.id)
            {
                self.active.push(order);
            }
        }
        if let Some(exchange) = &self.exchange {
            super::hl7_send::finish(
                store,
                exchange,
                None,
                if code == "HL7-DSR-disconnect" {
                    "HL7-order-disconnect"
                } else {
                    "HL7-order-session-error"
                },
            )
            .await?;
        }
        self.exchange = None;
        self.outcome(store, code, false).await?;
        Ok(())
    }
    pub(crate) async fn outcome(
        &mut self,
        store: &Handle,
        code: &str,
        sent: bool,
    ) -> Result<(), Error> {
        while let Some(order) = self.active.first() {
            let identity = order.key.clone();
            let code = analyte_model::SourceText::new(code)?;
            store
                .request(deadline(), move |store| {
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
                            super::event,
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
                            super::event,
                        )?;
                    }
                    Ok(())
                })
                .await?;
            self.active.remove(0);
        }
        self.active.clear();
        Ok(())
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
        let freshness = machine.destination_limits.freshness.as_secs();
        let request = match query::Request::new(message) {
            Ok(request) => request,
            Err(error) => {
                evidence::failed_reading(store, machine, receipt, &error.to_string()).await?;
                return Ok(None);
            }
        };
        let settings =
            super::settings::hl7(machine).ok_or_else(|| invalid("HL7 orders missing"))?;
        let query = settings
            .query()
            .as_ref()
            .ok_or_else(|| invalid("HL7 query policy missing"))?;
        let policy = &query.policy();
        let layout = &query.layout();
        let until = started
            .checked_add(Duration::from_millis(u64::from(policy.deadline_ms)))
            .ok_or_else(|| invalid("HL7 query deadline overflow"))?;
        let mut answer = CacheAnswer::Unavailable;
        let mut lines = Vec::new();
        let mut candidates = Vec::new();
        let selected = match sample(request, layout) {
            Ok(Some(sample)) => Some(sample),
            Ok(None) => {
                evidence::failed_reading(
                    store,
                    machine,
                    receipt,
                    "HL7 query selectors are outside the configured scope",
                )
                .await?;
                None
            }
            Err(error) => {
                evidence::failed_reading(store, machine, receipt, &error.to_string()).await?;
                None
            }
        };
        if !dialog.orders_busy()
            && Instant::now() < until
            && let Some(sample) = selected
        {
            let lookup = match super::lookup(store, machine, sample, until).await {
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
                        if !super::eligible(&order, machine) {
                            continue;
                        }
                        let mapped =
                            super::app_order(&order)
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
                                candidates.push(super::Candidate {
                                    identity: analyte_store::MachineOrderKey {
                                        destination: super::destination(machine),
                                        order: cached.id,
                                        machine: machine.id.clone(),
                                    },
                                    version: fields.version,
                                });
                                answer = CacheAnswer::Found;
                            }
                            Err(error) => {
                                let identity = analyte_store::MachineOrderKey {
                                    destination: super::destination(machine),
                                    order: cached.id,
                                    machine: machine.id.clone(),
                                };
                                let version = fields.version;
                                let reason = analyte_model::SourceText::new(&format!(
                                    "HL7-display-mapping: {error}"
                                ))?;
                                let rejected = super::reservation::reject(
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
        if Instant::now() >= until {
            answer = CacheAnswer::Unavailable;
        }
        let qck_metadata = crate::machine::hl7_response::response_metadata(
            crate::machine::hl7_response::Profile::Hl7,
        );
        let mut dsr_metadata = crate::machine::hl7_response::response_metadata(
            crate::machine::hl7_response::Profile::Hl7,
        );
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
            if !super::sending::reserve_candidates(
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
            failure_deadline: deadline().duration_since(clock),
            acknowledgment_wait: Duration::from_millis(u64::from(layout.acknowledgment_ms)),
        }))
    }
}

fn sample(
    request: query::Request<'_>,
    layout: &analyte_config::Hl7OrderQuery,
) -> Result<Option<SampleId>, Error> {
    if request.filter().is_some() {
        return Ok(None);
    }
    let definition = request.definition();
    for number in 5..=12 {
        if number == 7 || number == usize::from(layout.sample_field) {
            continue;
        }
        let value = definition.field(number).map_or(b"".as_slice(), |f| f.raw());
        if let Some(expected) = layout
            .filters
            .iter()
            .find(|filter| usize::from(filter.field) == number)
        {
            if value != expected.value.as_bytes() {
                return Ok(None);
            }
        } else if !value.is_empty() {
            return Ok(None);
        }
    }
    let Some(field) = definition.field(usize::from(layout.sample_field)) else {
        return Ok(None);
    };
    let Some(raw) = field.raw().strip_suffix(layout.sample_suffix.as_bytes()) else {
        return Ok(None);
    };
    let [_, component, repetition, _, subcomponent] = request.message().delimiters().bytes();
    if raw
        .iter()
        .any(|b| [component, repetition, subcomponent].contains(b))
    {
        return Ok(None);
    }
    let text = analyte_hl7::decode_text(raw, request.message())?;
    if text == "ALL" || text.is_empty() {
        return Ok(None);
    }
    Ok(Some(SampleId::new(&text)?))
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

#[cfg(test)]
mod tests {
    use super::*;
    use analyte_model::Identifier;
    struct Space;
    impl analyte_store::FreeSpace for Space {
        fn available(&self, _: &std::path::Path) -> std::io::Result<u64> {
            Ok(u64::MAX)
        }
    }
    #[tokio::test]
    async fn adr0003_rule8_cancelled_query_cleanup_never_claims_another_connection()
    -> Result<(), Error> {
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
            deadline(),
        )
        .await?;
        let key = analyte_store::MachineOrderKey {
            destination: super::super::destination(&machine),
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
                &evidence::Connection::new(&machine)?,
                "HL7-DSR-disconnect",
            )
            .await?;
        let identity = key.clone();
        let current = store
            .request(deadline(), move |store| store.machine_order(&identity))
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
                &evidence::Connection::new(&machine)?,
                "HL7-DSR-disconnect",
            )
            .await?;
        let current = store
            .request(deadline(), move |store| store.machine_order(&key))
            .await?
            .ok_or("reservation")?;
        assert_eq!(current.state, analyte_model::OrderState::Uncertain);
        assert_eq!(current.source_code.as_str(), "HL7-DSR-disconnect");
        store.shutdown(deadline()).await?;
        Ok(())
    }
    fn query_machine() -> Result<Machine, Error> {
        let definition = format!(
            "{}\n{}",
            include_str!("../../../config/examples/hl7.toml"),
            include_str!("../../../config/examples/hl7_orders.toml")
        );
        let snapshot = analyte_config::Draft::load(
            ("machine", &definition),
            ("site", include_str!("../../../config/examples/site.toml")),
            ("overrides", "schema_version=1"),
        )?
        .validate()?;
        Ok(Machine {
            capture: false,
            destination_limits: crate::runtime::delivery::limits(30),
            #[cfg(feature = "test-hooks")]
            law_committed: None,
            id: Identifier::new("synthetic")?,
            snapshot,
            listener: None,
            #[cfg(feature = "test-hooks")]
            astm_committed: None,
            #[cfg(feature = "test-hooks")]
            connection_closing: None,
        })
    }

    async fn reserve_original_connection(
        store: &Handle,
        key: &analyte_store::MachineOrderKey,
        owner: &Identifier,
    ) -> Result<(), Error> {
        let identity = key.clone();
        let original_owner = owner.clone();
        store
            .request(deadline(), move |store| {
                let now = jiff::Timestamp::now().as_second();
                store.order_page(&analyte_store::OrderPage {
                    destination: identity.destination.clone(),
                    cursor: Identifier::new("page")
                        .map_err(|_| analyte_store::Error::InvalidInput)?,
                    complete: true,
                    polled: now,
                    orders: vec![analyte_store::CachedOrder {
                        id: identity.order.clone(),
                        sample: SampleId::new("000Ab12")
                            .map_err(|_| analyte_store::Error::InvalidInput)?,
                        document: include_str!("../../../../docs/contract/vectors/order.json")
                            .to_owned(),
                        tombstone: false,
                        expiry: now
                            .checked_add(100)
                            .ok_or(analyte_store::Error::InvalidInput)?,
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
