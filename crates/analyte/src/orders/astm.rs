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

type LookupFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Vec<Lookup>, analyte_store::Error>> + Send + 'a>>;

/// Owned by one connection; dropping cancels lookup without issuing machine work.
pub(crate) struct Pending<'a> {
    future: LookupFuture<'a>,
    until: Instant,
}

pub(crate) enum Request {
    Orders(Vec<SampleId>),
    Cancel,
    Unsupported,
}

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
    pub(crate) fn deadline(&self) -> Instant {
        self.until
    }
    pub(crate) fn new(
        store: &'a Handle,
        machine: &Machine,
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
        let destination = super::destination(machine);
        let byte_limit = machine.snapshot.effective().machine().syntax.bytes;
        let future = Box::pin(async move {
            let mut answers = Vec::new();
            let mut bytes = 0usize;
            for sample in samples {
                let destination = destination.clone();
                let answer = store
                    .request(until, move |s| {
                        s.orders(
                            &destination,
                            &sample,
                            jiff::Timestamp::now().as_second(),
                            super::FRESHNESS_SECONDS,
                        )
                    })
                    .await?;
                if let Lookup::Orders(orders) = &answer {
                    for order in orders {
                        bytes = bytes
                            .checked_add(order.document.len())
                            .filter(|n| *n <= byte_limit)
                            .ok_or(analyte_store::Error::InvalidInput)?;
                    }
                }
                answers.push(answer);
            }
            Ok(answers)
        });
        Ok(Self { future, until })
    }

    pub(crate) async fn finish(
        &mut self,
        machine: &Machine,
    ) -> Result<Option<Vec<Vec<u8>>>, Error> {
        let result =
            tokio::time::timeout_at(tokio::time::Instant::from_std(self.until), &mut self.future)
                .await;
        let Ok(Ok(answers)) = result else {
            return Ok(failure(machine));
        };
        if Instant::now() >= self.until {
            return Ok(failure(machine));
        }
        let mut records = Vec::new();
        for answer in answers {
            match answer {
                Lookup::NotTrusted => return Ok(failure(machine)),
                Lookup::NoOrders => {}
                Lookup::Orders(orders) => {
                    for cached in orders {
                        let order: analyte_contract::Order =
                            analyte_contract::decode(cached.document.as_bytes())?;
                        if order.fields().state != analyte_contract::evidence::OrderState::Requested
                            || order.fields().tombstone
                        {
                            continue;
                        }
                        let Ok(mapped) = super::app_order(&order).and_then(|order| {
                            Ok(analyte_mapping::map_order(&order, &machine.snapshot)?)
                        }) else {
                            return Ok(failure(machine));
                        };
                        records.push(mapped.record().to_vec());
                        if records.len() > machine.snapshot.effective().machine().syntax.records {
                            return Ok(failure(machine));
                        }
                    }
                }
            }
        }
        if Instant::now() >= self.until {
            return Ok(failure(machine));
        }
        let code = if records.is_empty() { b'I' } else { b'N' };
        Ok(Some(envelope(records, code)))
    }
}

pub(crate) fn failure(machine: &Machine) -> Option<Vec<Vec<u8>>> {
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
        WhenUntrusted::Error => Some(envelope(Vec::new(), b'Q')),
        WhenUntrusted::NoAnswer => None,
    }
}

pub(crate) fn envelope(mut orders: Vec<Vec<u8>>, termination: u8) -> Vec<Vec<u8>> {
    let mut records = vec![b"H|\\^&|||Analyte|||||||P|1\r".to_vec()];
    for (index, order) in orders.drain(..).enumerate() {
        records.push(format!("P|{}\r", index.saturating_add(1)).into_bytes());
        records.push(order);
    }
    records.push(vec![b'L', b'|', b'1', b'|', termination, b'\r']);
    records
}
