//! Answers bounded ASTM specimen order queries without returning historical results (LIS02-A2 §11).
use crate::installation::config::Machine;
use crate::orders::OrderError as Error;

use analyte_config::WhenUntrusted;
use analyte_model::SampleId;
use analyte_store::{Handle, Lookup};
use std::{future::Future, pin::Pin, time::Instant};
#[cfg(test)]
mod tests;

type LookupFuture<'a> = Pin<Box<dyn Future<Output = Result<Option<Reply>, Error>> + Send + 'a>>;

use super::Candidate;

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
    if super::settings::astm(machine)
        .and_then(|o| o.query())
        .is_none()
    {
        return Ok(None);
    }
    let limits = crate::machine::astm::settings::limits(machine.snapshot.effective())
        .ok_or_else(|| Error::AstmLimitsMissing)?;
    let messages = analyte_astm::parse(payload, limits)?;
    Ok(
        match analyte_astm::query_intent(&messages)
            .map_err(|_| Error::UnsupportedSpecimenEscapes)?
        {
            analyte_astm::QueryIntent::Absent => None,
            analyte_astm::QueryIntent::Unsupported => Some(Request::Unsupported),
            analyte_astm::QueryIntent::Cancel => Some(Request::Cancel),
            analyte_astm::QueryIntent::Orders(samples) => Some(Request::Orders(
                samples
                    .iter()
                    .map(|sample| SampleId::new(sample))
                    .collect::<Result<_, _>>()?,
            )),
        },
    )
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
        let query = super::settings::astm(machine)
            .and_then(|o| o.query())
            .ok_or_else(|| Error::AstmQueryCapabilityAbsent)?;
        let until = started
            .checked_add(std::time::Duration::from_millis(u64::from(
                query.deadline_ms,
            )))
            .ok_or_else(|| Error::AstmQueryDeadlineOverflow)?;
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
    let limits = machine.snapshot.effective().stack().limits();
    for sample in samples {
        let answer = super::lookup(store, machine, sample, until).await?;
        match answer {
            Lookup::NotTrusted => return Ok(failure(machine)),
            Lookup::NoOrders => {}
            Lookup::Orders(orders) => {
                for cached in orders {
                    bytes = bytes
                        .checked_add(cached.document.len())
                        .filter(|n| *n <= limits.bytes())
                        .ok_or_else(|| Error::AstmQueryResponseTooLarge)?;
                    match super::admission::prepare(
                        store,
                        machine,
                        cached,
                        until,
                        |error| format!("rejected: {error}"),
                        |order| {
                            let order = super::app_order(order)?;
                            Ok(analyte_mapping::astm::map_order(
                                &order,
                                machine
                                    .snapshot
                                    .effective()
                                    .stack()
                                    .astm_syntax()
                                    .ok_or(Error::AstmLimitsMissing)?
                                    .limits(),
                                super::settings::astm(machine)
                                    .ok_or(Error::AstmQueryCapabilityAbsent)?,
                            )?)
                        },
                    )
                    .await?
                    {
                        super::admission::Prepared::Skipped => continue,
                        super::admission::Prepared::Rejected(_) => return Ok(failure(machine)),
                        super::admission::Prepared::Mapped(candidate, mapped) => {
                            candidates.push(candidate);
                            records.push(mapped.encode());
                        }
                    }
                    if records
                        .len()
                        .checked_mul(2)
                        .and_then(|n| n.checked_add(2))
                        .is_none_or(|n| n > limits.records())
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
    let code = if records.is_empty() {
        analyte_astm::OrderTermination::NoInformation
    } else {
        analyte_astm::OrderTermination::Normal
    };
    Ok(Some(Reply {
        records: envelope(machine, records, code),
        candidates,
    }))
}
/// Applies the explicit failure policy without manufacturing an empty answer.
pub(crate) fn failure(machine: &Machine) -> Option<Reply> {
    match super::settings::astm(machine)?.query()?.when_untrusted {
        WhenUntrusted::Error => Some(Reply {
            records: envelope(
                machine,
                Vec::new(),
                analyte_astm::OrderTermination::QueryError,
            ),
            candidates: Vec::new(),
        }),
        WhenUntrusted::NoAnswer => None,
    }
}

/// Wraps mapped orders with independent patient groups and explicit termination.
pub(crate) fn envelope(
    machine: &Machine,
    orders: Vec<Vec<u8>>,
    termination: analyte_astm::OrderTermination,
) -> Vec<Vec<u8>> {
    let receiver = super::settings::astm(machine)
        .and_then(|s| s.layout().receiver.as_ref())
        .map_or("", |id| id.as_str());
    let timestamp = jiff::Timestamp::now()
        .strftime("%Y%m%d%H%M%S%z")
        .to_string();
    analyte_astm::encode_order_envelope(receiver, &timestamp, orders, termination)
}

/// Reserves query orders immediately before link transmission, with no app event.
/// The connection owns every reservation even if this future fails or is cancelled.
pub(crate) async fn reserve_query(
    store: &Handle,
    machine: &Machine,
    reply: Reply,
    until: Instant,
    active: &mut Vec<analyte_store::MachineOrder>,
) -> Result<Option<Vec<Vec<u8>>>, Error> {
    if !super::sending::reserve_candidates(
        store,
        reply.candidates,
        until,
        active,
        None,
        machine.destination_limits.freshness.as_secs(),
    )
    .await?
    {
        return Ok(failure(machine).map(|reply| reply.records));
    }
    Ok(Some(reply.records))
}
