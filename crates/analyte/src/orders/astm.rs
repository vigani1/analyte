//! LIS02-A2 §11: bounded specimen-only order queries, never historical results.
use crate::{Error, config::Machine, invalid};
use analyte_astm::{Kind, Message, Query};
use analyte_config::WhenUntrusted;
use analyte_model::SampleId;
use analyte_store::{Handle, Lookup};
use std::{future::Future, pin::Pin, time::Instant};
#[cfg(test)]
#[path = "query_tests.rs"]
mod tests;

type LookupFuture<'a> = Pin<Box<dyn Future<Output = Result<Option<Reply>, Error>> + Send + 'a>>;

/// Order identity and version mapped for a pending query response.
pub(crate) struct Candidate {
    /// Stable app order and machine identity to recheck immediately before sending.
    pub(crate) identity: analyte_store::MachineOrderKey,
    /// Version used by the mapper.
    pub(crate) version: std::num::NonZeroU64,
}

/// A reply is not a send: candidates are reserved only when the link is ready.
pub(crate) struct Reply {
    /// Complete mapped records, or an explicit error/no-information response.
    pub(crate) records: Vec<Vec<u8>>,
    /// Orders requiring durable Sending reservations before any bytes leave.
    pub(crate) candidates: Vec<Candidate>,
}
/// Owned by one connection; dropping cancels lookup without issuing machine work.
pub(crate) struct Pending<'a> {
    future: LookupFuture<'a>,
    until: Instant,
}

/// Parsed query intent, distinct from a trusted cache answer.
pub(crate) enum Request {
    /// Exact specimen identifiers requested by the machine.
    Orders(Vec<SampleId>),
    /// Cancel the pending lookup without sending machine work.
    Cancel,
    /// Selectors outside the declared specimen-only subset.
    Unsupported,
}

/// Recognizes supported query intent without normalizing specimen identifiers.
pub(crate) fn request(machine: &Machine, payload: &[u8]) -> Result<Option<Request>, Error> {
    if machine
        .snapshot
        .effective()
        .machine()
        .orders
        .as_ref()
        .and_then(|o| o.query.as_ref())
        .is_none()
    {
        return Ok(None);
    }
    let limits = machine
        .snapshot
        .effective()
        .astm_limits()
        .ok_or_else(|| invalid("ASTM limits missing"))?;
    let messages = analyte_astm::parse(payload, limits)?;
    let queries: Vec<_> = messages
        .iter()
        .flat_map(Message::records)
        .filter_map(Query::new)
        .collect();
    if queries.is_empty() {
        return Ok(None);
    }
    if queries.len() != 1
        || messages
            .iter()
            .flat_map(Message::records)
            .any(|r| !matches!(r.kind(), Kind::Header | Kind::Query | Kind::Terminator))
    {
        return Ok(Some(Request::Unsupported));
    }
    let Some(query) = queries.first() else {
        return Ok(Some(Request::Unsupported));
    };
    if query.field(13).is_some_and(|f| f.raw() == b"A") {
        return Ok(Some(Request::Cancel));
    }
    if query.field(13).is_none_or(|f| f.raw() != b"O")
        || (4..=12)
            .filter(|f| *f != 5)
            .any(|f| query.field(f).is_some_and(|f| !f.raw().is_empty()))
        || query
            .test_id()
            .is_some_and(|f| !matches!(f.raw(), b"" | b"ALL"))
    {
        return Ok(Some(Request::Unsupported));
    }
    let Some(ids) = query.range_start() else {
        return Ok(Some(Request::Unsupported));
    };
    let mut samples = Vec::new();
    for repetition in ids.repeats() {
        if repetition
            .iter()
            .enumerate()
            .any(|(index, part)| index != 1 && !part.raw().is_empty())
        {
            return Ok(Some(Request::Unsupported));
        }
        let Some(sample) = repetition.get(1) else {
            return Ok(Some(Request::Unsupported));
        };
        if sample.raw() == b"ALL" || sample.raw().is_empty() {
            return Ok(Some(Request::Unsupported));
        }
        samples.push(SampleId::new(sample.text())?);
    }
    if samples.is_empty() {
        return Ok(Some(Request::Unsupported));
    }
    Ok(Some(Request::Orders(samples)))
}

impl<'a> Pending<'a> {
    /// Returns the immutable total budget for this query.
    pub(crate) fn deadline(&self) -> Instant {
        self.until
    }
    /// Creates one owned lookup future measured from transfer start.
    pub(crate) fn new(
        store: &'a Handle,
        machine: &'a Machine,
        samples: Vec<SampleId>,
        started: Instant,
    ) -> Result<Self, Error> {
        let query = machine
            .snapshot
            .effective()
            .machine()
            .orders
            .as_ref()
            .and_then(|o| o.query.as_ref())
            .ok_or_else(|| invalid("query capability absent"))?;
        let until = started
            .checked_add(std::time::Duration::from_millis(u64::from(
                query.deadline_ms,
            )))
            .ok_or_else(|| invalid("query deadline overflow"))?;
        let future = Box::pin(answer(store, machine, samples, until));
        Ok(Self { future, until })
    }

    /// Polls the same lookup; late answers produce only error or silence.
    pub(crate) async fn finish(&mut self, machine: &Machine) -> Result<Option<Reply>, Error> {
        match tokio::time::timeout_at(tokio::time::Instant::from_std(self.until), &mut self.future)
            .await
        {
            Ok(Ok(records)) if Instant::now() < self.until => Ok(records),
            Ok(Ok(_)) | Err(_) => Ok(failure(machine)),
            Ok(Err(error)) => Err(error),
        }
    }
}

async fn answer(
    store: &Handle,
    machine: &Machine,
    samples: Vec<SampleId>,
    until: Instant,
) -> Result<Option<Reply>, Error> {
    let mut records = Vec::new();
    let mut candidates = Vec::new();
    let mut bytes = 0usize;
    let limits = &machine.snapshot.effective().machine().syntax;
    for sample in samples {
        let destination = super::destination(machine);
        let answer = match store
            .request(until, move |s| {
                s.orders(
                    &destination,
                    &sample,
                    jiff::Timestamp::now().as_second(),
                    super::FRESHNESS_SECONDS,
                )
            })
            .await
        {
            Ok(answer) => answer,
            Err(analyte_store::Error::Timeout) => return Ok(failure(machine)),
            Err(error) => return Err(error.into()),
        };
        match answer {
            Lookup::NotTrusted => return Ok(failure(machine)),
            Lookup::NoOrders => {}
            Lookup::Orders(orders) => {
                for cached in orders {
                    bytes = bytes
                        .checked_add(cached.document.len())
                        .filter(|n| *n <= limits.bytes)
                        .ok_or_else(|| invalid("query response byte limit exceeded"))?;
                    let order: analyte_contract::Order =
                        analyte_contract::decode(cached.document.as_bytes())?;
                    let fields = order.fields();
                    if fields.state != analyte_contract::evidence::OrderState::Requested
                        || fields.tombstone
                    {
                        continue;
                    }
                    if fields.target_analyzers.as_ref().is_some_and(|targets| {
                        !targets
                            .iter()
                            .any(|target| target.as_str() == machine.id.as_str())
                    }) {
                        continue;
                    }
                    let mapped = match super::app_order(&order).and_then(|order| {
                        Ok(analyte_mapping::map_order(&order, &machine.snapshot)?)
                    }) {
                        Ok(mapped) => mapped,
                        Err(error) => {
                            let reason =
                                analyte_model::SourceText::new(&format!("rejected: {error}"))?;
                            let identity = analyte_store::MachineOrderKey {
                                destination: super::destination(machine),
                                order: cached.id,
                                machine: machine.id.clone(),
                            };
                            let version = fields.version;
                            let rejected = store
                                .request(until, move |store| {
                                    store.reject_unsent(
                                        &analyte_store::OrderReservation {
                                            identity: &identity,
                                            version,
                                            now: jiff::Timestamp::now().as_second(),
                                            freshness: super::FRESHNESS_SECONDS,
                                        },
                                        reason,
                                        super::event,
                                    )
                                })
                                .await?;
                            if let Err(refusal) = rejected {
                                tracing::info!(reason=%refusal,"query mapping rejection was superseded before recording");
                            }
                            return Ok(failure(machine));
                        }
                    };
                    candidates.push(Candidate {
                        identity: analyte_store::MachineOrderKey {
                            destination: super::destination(machine),
                            order: cached.id,
                            machine: machine.id.clone(),
                        },
                        version: fields.version,
                    });
                    records.push(mapped.record().to_vec());
                    if records
                        .len()
                        .checked_mul(2)
                        .and_then(|n| n.checked_add(2))
                        .is_none_or(|n| n > limits.records)
                    {
                        return Ok(failure(machine));
                    }
                }
            }
        }
    }
    if Instant::now() >= until {
        return Ok(failure(machine));
    }
    let code = if records.is_empty() { b'I' } else { b'N' };
    Ok(Some(Reply {
        records: envelope(records, code),
        candidates,
    }))
}
/// Applies the explicit failure policy without manufacturing an empty answer.
pub(crate) fn failure(machine: &Machine) -> Option<Reply> {
    match machine
        .snapshot
        .effective()
        .machine()
        .orders
        .as_ref()?
        .query
        .as_ref()?
        .when_untrusted
    {
        WhenUntrusted::Error => Some(Reply {
            records: envelope(Vec::new(), b'Q'),
            candidates: Vec::new(),
        }),
        WhenUntrusted::NoAnswer => None,
    }
}

/// Wraps mapped orders with independent patient groups and explicit termination.
pub(crate) fn envelope(mut orders: Vec<Vec<u8>>, termination: u8) -> Vec<Vec<u8>> {
    let mut records = vec![b"H|\\^&|||Analyte|||||||P|1\r".to_vec()];
    for (index, order) in orders.drain(..).enumerate() {
        records.push(format!("P|{}\r", index.saturating_add(1)).into_bytes());
        records.push(order);
    }
    records.push(vec![b'L', b'|', b'1', b'|', termination, b'\r']);
    records
}
