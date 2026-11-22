//! Answers bounded ASTM specimen order queries without returning historical results (LIS02-A2 §11).
use crate::installation::config::Machine;
use crate::machine::astm::Error;
use crate::orders::admission::{self, Prepared};

use analyte_config::{AstmOrders, AstmStack, OrderQuery, WhenUntrusted};
use analyte_model::SampleId;
use analyte_store::{Handle, Lookup};
use std::{future::Future, pin::Pin, time::Instant};
mod completion;
#[cfg(test)]
mod tests;
pub(crate) use completion::finished;

type LookupFuture<'a> = Pin<Box<dyn Future<Output = Result<Option<Reply>, Error>> + Send + 'a>>;

use crate::orders::Candidate;

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
    queries: Queries<'a>,
}

/// Query settings, present only when the machine is configured to ask for orders.
#[derive(Clone, Copy)]
pub(crate) struct Queries<'a> {
    stack: &'a AstmStack,
    orders: &'a AstmOrders,
    policy: &'a OrderQuery,
}

impl<'a> Queries<'a> {
    pub(crate) fn new(stack: &'a AstmStack) -> Option<Self> {
        let orders = stack.orders()?;
        let policy = orders.query()?;
        Some(Self {
            stack,
            orders,
            policy,
        })
    }
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
pub(crate) fn request(queries: Queries<'_>, payload: &[u8]) -> Result<Option<Request>, Error> {
    let limits = crate::machine::astm::settings::parser_limits(queries.stack.settings().syntax());
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
                    .collect::<Result<_, _>>()
                    .map_err(crate::orders::OrderError::from)?,
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
        queries: Queries<'a>,
        samples: Vec<SampleId>,
        started: Instant,
    ) -> Result<Self, Error> {
        let until = started
            .checked_add(std::time::Duration::from_millis(u64::from(
                queries.policy.deadline_ms,
            )))
            .ok_or_else(|| Error::QueryDeadlineOverflow)?;
        let future = Box::pin(answer(store, machine, queries, samples, until));
        Ok(Self {
            future,
            until,
            queries,
        })
    }

    /// Polls the same lookup; late answers produce only error or silence.
    pub(crate) async fn finish(&mut self) -> Result<Option<Reply>, Error> {
        match tokio::time::timeout_at(tokio::time::Instant::from_std(self.until), &mut self.future)
            .await
        {
            Ok(Ok(records)) if Instant::now() < self.until => Ok(records),
            Ok(Ok(_)) | Err(_) => Ok(failure(self.queries)),
            Ok(Err(error)) => Err(error),
        }
    }
}

async fn answer(
    store: &Handle,
    machine: &Machine,
    queries: Queries<'_>,
    samples: Vec<SampleId>,
    until: Instant,
) -> Result<Option<Reply>, Error> {
    let mut records = Vec::new();
    let mut candidates = Vec::new();
    let mut bytes = 0usize;
    let limits = machine.snapshot.effective().stack().limits();
    for sample in samples {
        let answer = crate::orders::lookup(store, machine, sample, until).await?;
        match answer {
            Lookup::NotTrusted => return Ok(failure(queries)),
            Lookup::NoOrders => {}
            Lookup::Orders(orders) => {
                for cached in orders {
                    bytes = bytes
                        .checked_add(cached.document.len())
                        .filter(|n| *n <= limits.bytes())
                        .ok_or_else(|| Error::QueryResponseTooLarge)?;
                    match admission::prepare(
                        store,
                        machine,
                        cached,
                        until,
                        |error: &Error| format!("rejected: {error}"),
                        |order| {
                            let order = crate::orders::app_order(order)
                                .map_err(crate::orders::OrderError::from)?;
                            Ok(analyte_mapping::astm::map_order(
                                &order,
                                queries.stack.settings().syntax().limits(),
                                queries.orders,
                            )?)
                        },
                    )
                    .await?
                    {
                        Prepared::Skipped => continue,
                        Prepared::Rejected(_) => {
                            return Ok(failure(queries));
                        }
                        Prepared::Mapped(candidate, mapped) => {
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
                        return Ok(failure(queries));
                    }
                }
            }
        }
    }
    if Instant::now() >= until {
        return Ok(failure(queries));
    }
    let code = if records.is_empty() {
        analyte_astm::OrderTermination::NoInformation
    } else {
        analyte_astm::OrderTermination::Normal
    };
    Ok(Some(Reply {
        records: envelope(queries.orders, records, code),
        candidates,
    }))
}
/// Applies the explicit failure policy without manufacturing an empty answer.
pub(crate) fn failure(queries: Queries<'_>) -> Option<Reply> {
    match queries.policy.when_untrusted {
        WhenUntrusted::Error => Some(Reply {
            records: envelope(
                queries.orders,
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
    settings: &AstmOrders,
    orders: Vec<Vec<u8>>,
    termination: analyte_astm::OrderTermination,
) -> Vec<Vec<u8>> {
    let receiver = settings
        .layout()
        .receiver
        .as_ref()
        .map_or("", |id| id.as_str());
    let timestamp = jiff::Timestamp::now()
        .strftime("%Y%m%d%H%M%S%z")
        .to_string();
    analyte_astm::encode_order_envelope(receiver, &timestamp, orders, termination)
}

/// Reserves query orders immediately before link transmission, with no app event.
/// Returns no records when a reservation is refused, so the caller sends its failure reply.
/// The connection owns every reservation even if this future fails or is cancelled.
pub(crate) async fn reserve_query(
    store: &Handle,
    machine: &Machine,
    reply: Reply,
    until: Instant,
    active: &mut Vec<analyte_store::MachineOrder>,
) -> Result<Option<Vec<Vec<u8>>>, Error> {
    let reserved = crate::orders::sending::reserve_candidates(
        store,
        &machine.model,
        reply.candidates,
        until,
        active,
        None,
        machine.destination_limits.freshness.as_secs(),
    )
    .await?;
    Ok(reserved.then_some(reply.records))
}
