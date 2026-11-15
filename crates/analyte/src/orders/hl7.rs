//! Host-owned legacy display-query state; HL7 2.3.1 section 2.18 / 2.5.1 section 5.10.
use crate::{Error, config::Machine, deadline, evidence, invalid};
use analyte_framing::mllp;
use analyte_hl7::{
    AckCode, Message, Msh, Options,
    query::{self, CacheAnswer},
};
use analyte_model::{ReceiptId, SampleId};
use analyte_store::{Handle, Lookup, MachineOrder};
use std::time::{Duration, Instant};

/// Owns partial input, the immutable query budget and at most one pending DSR.
/// Cancellation leaves reservations owned here for disconnect cleanup; restart
/// recovery owns any store call whose completion was lost during cancellation.
pub(crate) struct Connection {
    decoder: mllp::Decoder,
    options: Options,
    started: Option<Instant>,
    awaiting: Option<(String, Instant)>,
    active: Vec<MachineOrder>,
    reserving: Vec<analyte_store::MachineOrderKey>,
    input_evidence: Vec<u8>,
    owner: analyte_model::Identifier,
    exchange: Option<(super::hl7_send::Exchange, Instant)>,
    cursor: String,
}
impl Connection {
    pub(crate) fn new(options: Options) -> Result<Self, Error> {
        Ok(Self {
            decoder: mllp::Decoder::new(mllp::Limit::new(options.limits.message_bytes)?),
            options,
            started: None,
            awaiting: None,
            active: Vec::new(),
            reserving: Vec::new(),
            input_evidence: Vec::new(),
            owner: analyte_model::Identifier::new(&uuid::Uuid::now_v7().to_string())?,
            exchange: None,
            cursor: String::new(),
        })
    }
    pub(crate) fn wake(&self, machine: &Machine) -> Option<Instant> {
        let partial = self.started.and_then(|start| {
            start.checked_add(Duration::from_millis(u64::from(
                machine
                    .snapshot
                    .effective()
                    .stack()
                    .timeouts()
                    .receive_timeout_ms,
            )))
        });
        let outbound = self.exchange.as_ref().map(|(_, until)| *until);
        let waiting = match (self.awaiting.as_ref().map(|(_, until)| *until), outbound) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, None) => a,
            (None, b) => b,
        };
        match (waiting, partial) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, None) => a,
            (None, b) => b,
        }
    }
    pub(crate) async fn idle(
        &mut self,
        store: &Handle,
        machine: &evidence::Connection<'_>,
        conn: &mut analyte_transport::Connection,
    ) -> Result<(), Error> {
        if self.awaiting.is_some() || self.exchange.is_some() || !self.decoder.pending().is_empty()
        {
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
                .map(|settings| settings.orders)
                .and_then(|orders| orders.send.as_ref())
                .ok_or("send settings")?
                .settings
                .response_ms;
            let until = Instant::now()
                .checked_add(Duration::from_millis(u64::from(wait)))
                .ok_or("HL7 response deadline overflow")?;
            self.exchange = Some((exchange, until));
            self.reserving.clear();
            let message = self
                .exchange
                .as_ref()
                .ok_or("exchange absent")?
                .0
                .message
                .message();
            write(conn, message.raw(), self.options, until).await?;
        }
        Ok(())
    }
    pub(crate) fn consumed(&mut self) {
        self.input_evidence.clear();
    }
    pub(crate) async fn tick(
        &mut self,
        store: &Handle,
        machine: &evidence::Connection<'_>,
    ) -> Result<(), Error> {
        if self
            .exchange
            .as_ref()
            .is_some_and(|(_, until)| Instant::now() >= *until)
        {
            if let Some((exchange, _)) = &self.exchange {
                super::hl7_send::finish(store, exchange, None, "HL7-order-response-timeout")
                    .await?;
            }
            self.exchange = None;
        }
        if self
            .awaiting
            .as_ref()
            .is_some_and(|(_, until)| Instant::now() >= *until)
        {
            self.outcome(store, "HL7-DSR-ACK-timeout", false).await?;
        }
        if self.started.is_some_and(|start| {
            start.elapsed()
                >= Duration::from_millis(u64::from(
                    machine
                        .snapshot
                        .effective()
                        .stack()
                        .timeouts()
                        .receive_timeout_ms,
                ))
        }) {
            self.partial(store, machine).await?;
        }
        Ok(())
    }
    pub(crate) async fn close(
        &mut self,
        store: &Handle,
        machine: &evidence::Connection<'_>,
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
        if let Some((exchange, _)) = &self.exchange {
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
        if !self.input_evidence.is_empty() {
            evidence::park_bytes(
                store,
                machine,
                std::mem::take(&mut self.input_evidence),
                "HL7 input processing interrupted",
            )
            .await?;
        }
        self.partial(store, machine).await
    }
    async fn partial(
        &mut self,
        store: &Handle,
        machine: &evidence::Connection<'_>,
    ) -> Result<(), Error> {
        if let Some(mllp::Event::Rejected { raw, error }) = self.decoder.finish() {
            evidence::park_bytes(store, machine, raw, &error.to_string()).await?;
        }
        self.started = None;
        Ok(())
    }
    async fn outcome(&mut self, store: &Handle, code: &str, sent: bool) -> Result<(), Error> {
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
        self.awaiting = None;
        Ok(())
    }
    /// Demultiplexes complete blocks; result blocks still use the existing dialog.
    pub(crate) async fn input(
        &mut self,
        store: &Handle,
        machine: &evidence::Connection<'_>,
        conn: &mut analyte_transport::Connection,
        bytes: Vec<u8>,
    ) -> Result<Vec<u8>, Error> {
        self.input_evidence = bytes.clone();
        let mut results = Vec::new();
        for byte in bytes {
            if self.started.is_none() {
                self.started = Some(Instant::now());
            }
            if let Some(event) = self.decoder.push(byte) {
                let started = self.started.take().unwrap_or_else(Instant::now);
                match event {
                    mllp::Event::Rejected { raw, error } => {
                        evidence::park_bytes(store, machine, raw, &error.to_string()).await?;
                    }
                    mllp::Event::Block(raw) => {
                        let payload = raw
                            .len()
                            .checked_sub(2)
                            .and_then(|end| raw.get(1..end))
                            .ok_or_else(|| invalid("HL7 block boundary"))?;
                        let parsed = Message::parse(payload, self.options);
                        if let Ok(message) = parsed
                            && let Ok(header) = Msh::new(&message)
                        {
                            let kind = header.message_type().next();
                            if matches!(kind, Some(b"ORR" | b"ORL")) && self.exchange.is_some() {
                                evidence::receive(store, machine, ReceiptId::new(), raw.clone())
                                    .await?;
                                let (exchange, until) =
                                    self.exchange.as_ref().ok_or("HL7 order exchange absent")?;
                                super::hl7_send::finish(
                                    store,
                                    exchange,
                                    if Instant::now() < *until {
                                        Some(&message)
                                    } else {
                                        None
                                    },
                                    "HL7-order-response-timeout",
                                )
                                .await?;
                                self.exchange = None;
                                continue;
                            }
                            if kind == Some(b"QRY") {
                                let receipt = ReceiptId::new();
                                evidence::receive(store, machine, receipt, raw.clone()).await?;
                                if !edition_matches(machine, header.version()) {
                                    evidence::failed_reading(
                                        store,
                                        machine,
                                        receipt,
                                        "HL7 query edition differs from configured edition",
                                    )
                                    .await?;
                                    continue;
                                }
                                self.answer(store, machine, conn, &message, started, receipt)
                                    .await?;
                                continue;
                            }
                            if kind == Some(b"ACK") && self.awaiting.is_some() {
                                let receipt = ReceiptId::new();
                                evidence::receive(store, machine, receipt, raw.clone()).await?;
                                let (id, until) = self
                                    .awaiting
                                    .as_ref()
                                    .ok_or_else(|| invalid("DSR correlation absent"))?;
                                if Instant::now() >= *until {
                                    self.outcome(store, "HL7-DSR-ACK-timeout", false).await?;
                                } else {
                                    if !edition_matches(machine, header.version()) {
                                        evidence::failed_reading(
                                            store,
                                            machine,
                                            receipt,
                                            "HL7 DSR ACK edition differs from configured edition",
                                        )
                                        .await?;
                                        self.outcome(store, "HL7-DSR-ACK-invalid-edition", false)
                                            .await?;
                                        continue;
                                    }
                                    match query::DisplayAck::new(
                                        &message,
                                        analyte_hl7::ControlId::new(id.as_bytes())?,
                                    ) {
                                        Ok(ack) => match ack.code() {
                                            AckCode::Aa => {
                                                self.outcome(store, "HL7-DSR-ACK-AA", true).await?;
                                            }
                                            AckCode::Ae => {
                                                self.outcome(store, "HL7-DSR-ACK-AE", false)
                                                    .await?;
                                            }
                                            AckCode::Ar => {
                                                self.outcome(store, "HL7-DSR-ACK-AR", false)
                                                    .await?;
                                            }
                                            AckCode::Ca | AckCode::Ce | AckCode::Cr => {
                                                return Err(invalid("invalid DSR application ACK"));
                                            }
                                        },
                                        Err(error) => {
                                            evidence::failed_reading(
                                                store,
                                                machine,
                                                receipt,
                                                &error.to_string(),
                                            )
                                            .await?;
                                            self.outcome(store, "HL7-DSR-ACK-invalid", false)
                                                .await?;
                                        }
                                    }
                                }
                                continue;
                            }
                        }
                        results.extend_from_slice(&raw);
                    }
                }
            }
        }
        Ok(results)
    }
    async fn answer(
        &mut self,
        store: &Handle,
        machine: &Machine,
        conn: &mut analyte_transport::Connection,
        message: &Message,
        started: Instant,
        receipt: ReceiptId,
    ) -> Result<(), Error> {
        let freshness = machine.destination_limits.freshness.as_secs();
        let request = match query::Request::new(message) {
            Ok(request) => request,
            Err(error) => {
                evidence::failed_reading(store, machine, receipt, &error.to_string()).await?;
                return Ok(());
            }
        };
        let settings = super::settings::hl7(machine)
            .map(|settings| settings.orders)
            .ok_or_else(|| invalid("HL7 orders missing"))?;
        let query = settings
            .query
            .as_ref()
            .ok_or_else(|| invalid("HL7 query policy missing"))?;
        let policy = &query.policy;
        let layout = &query.layout;
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
        if self.awaiting.is_none()
            && self.exchange.is_none()
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
                        let mapped = super::app_order(&order).and_then(|app| {
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
                                let rejected = store
                                    .request(until, move |store| {
                                        store.reject_unsent(
                                            &analyte_store::OrderReservation {
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
        let qck_id = control_id();
        let dsr_id = control_id();
        let timestamp = jiff::Timestamp::now()
            .strftime("%Y%m%d%H%M%S%z")
            .to_string();
        let mut dsr = None;
        if answer == CacheAnswer::Found {
            let refs = lines.iter().map(Vec::as_slice).collect::<Vec<_>>();
            match query::display(
                request,
                response(&dsr_id, &timestamp)?,
                analyte_hl7::ControlId::new(qck_id.as_bytes())?,
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
        let qck = query::acknowledgment(
            request,
            response(&qck_id, &timestamp)?,
            answer,
            self.options,
        )?;
        if let Err(error) = write(
            conn,
            qck.raw(),
            self.options,
            if answer == CacheAnswer::Found {
                until
            } else {
                deadline()
            },
        )
        .await
        {
            self.outcome(store, "HL7-QCK-write-failed", false).await?;
            return Err(error);
        }
        if let Some(dsr) = dsr {
            if Instant::now() >= until {
                self.outcome(store, "HL7-query-deadline-before-DSR", false)
                    .await?;
                return Ok(());
            }
            self.awaiting = Some((
                dsr_id,
                Instant::now()
                    .checked_add(Duration::from_millis(u64::from(layout.acknowledgment_ms)))
                    .ok_or_else(|| invalid("DSR ACK deadline overflow"))?,
            ));
            if let Err(error) = write(conn, dsr.raw(), self.options, until).await {
                self.outcome(store, "HL7-DSR-write-failed", false).await?;
                return Err(error);
            }
        }
        Ok(())
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
fn control_id() -> String {
    uuid::Uuid::now_v7()
        .simple()
        .to_string()
        .chars()
        .skip(12)
        .collect()
}
fn response<'a>(id: &'a str, time: &'a str) -> Result<analyte_hl7::Response<'a>, Error> {
    Ok(analyte_hl7::Response {
        control_id: analyte_hl7::ControlId::new(id.as_bytes())?,
        timestamp: time.as_bytes(),
    })
}
async fn write(
    conn: &mut analyte_transport::Connection,
    payload: &[u8],
    options: Options,
    until: Instant,
) -> Result<(), Error> {
    let wire = mllp::encode(payload, mllp::Limit::new(options.limits.message_bytes)?)?;
    tokio::time::timeout_at(tokio::time::Instant::from_std(until), async {
        for chunk in wire.chunks(analyte_transport::CHUNK_LIMIT) {
            conn.write(chunk.to_vec()).await?;
        }
        Ok::<_, Error>(())
    })
    .await??;
    Ok(())
}

fn edition_matches(machine: &Machine, version: analyte_hl7::Version) -> bool {
    matches!(
        (
            super::settings::hl7(machine).map(|settings| settings.syntax.edition),
            version
        ),
        (
            Some(analyte_config::Edition::V231),
            analyte_hl7::Version::V231
        ) | (
            Some(analyte_config::Edition::V251),
            analyte_hl7::Version::V251
        )
    )
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
        let machine = Machine {
            capture: false,
            destination_limits: crate::delivery::limits(30),
            #[cfg(feature = "test-hooks")]
            law_committed: None,
            id: Identifier::new("synthetic")?,
            snapshot,
            listener: None,
            #[cfg(feature = "test-hooks")]
            astm_committed: None,
        };
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
                    .reserve_send_owned(
                        &analyte_store::OrderReservation {
                            identity: &identity,
                            version: std::num::NonZeroU64::MIN,
                            now,
                            freshness: 30,
                        },
                        &original_owner,
                    )?
                    .map_err(|_| analyte_store::Error::Conflict)
            })
            .await?;
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
}
