//! Checks the order feed: paging, cursors, 410, versions, and orders the developer seeded.
use crate::client::{App, Key};
use crate::{Outcome, Report, RunError, SEED_FILE, limit};
use analyte_contract::{ErrorBody, Order, OrdersPage};
use serde_json::Value;
use std::collections::BTreeMap;

const ORDERS: &str = "GET /orders";
/// Bounds every chain the checker walks, even against a feed that never completes.
const MAX_PAGES: usize = 1000;

/// One order per ID, as typed values so unknown fields never count as differences.
type Orders = BTreeMap<String, Value>;

/// Why a chain of pages did not reach `complete: true`.
enum ChainError {
    /// The app broke a paging rule; the text says which.
    Broken(String),
    /// More pages than the checker walks; not a contract fault.
    TooLong,
}

impl ChainError {
    fn reason(self) -> String {
        match self {
            Self::Broken(reason) => reason,
            Self::TooLong => format!("no page with complete true within {MAX_PAGES} pages"),
        }
    }
}

/// A completed chain: its orders in feed order and the cursor to keep.
struct Chain {
    orders: Vec<Order>,
    cursor: String,
    pages: usize,
}

pub(crate) async fn check(
    app: &App,
    seed: Option<&[Order]>,
    report: &mut Report,
) -> Result<(), RunError> {
    let max = limit("max_orders_per_page")?;
    let max_text = max.to_string();
    let outcome = match page(app, &[("limit", &max_text)], max).await {
        Ok(_) => Outcome::Pass,
        Err(reason) => Outcome::Fail(reason),
    };
    report.add("orders.first-page", ORDERS, outcome);
    report.add("orders.limit-one", ORDERS, limit_one(app).await);
    report.add(
        "orders.limit-above-maximum",
        ORDERS,
        above_maximum(app, max).await,
    );
    report.add("orders.unknown-cursor", ORDERS, unknown_cursor(app).await);

    let snapshot = match chain(app, None, max).await.and_then(|chain| {
        unique(&chain.orders)
            .map(|orders| (orders, chain.cursor))
            .map_err(ChainError::Broken)
    }) {
        Ok(snapshot) => {
            report.add("orders.snapshot", ORDERS, Outcome::Pass);
            Some(snapshot)
        }
        Err(error) => {
            report.add("orders.snapshot", ORDERS, Outcome::Fail(error.reason()));
            None
        }
    };
    let Some((first, cursor)) = snapshot else {
        let skip = Outcome::Skip("needs a complete snapshot (see orders.snapshot)".into());
        for name in ["orders.cursor-retained", "orders.versions-only-increase"] {
            report.add(name, ORDERS, skip.clone());
        }
        seeded(app, seed, None, report).await;
        return Ok(());
    };
    let changes = chain(app, Some(&cursor), max).await;
    let changes_outcome = match &changes {
        Ok(_) => Outcome::Pass,
        Err(ChainError::Broken(reason)) => {
            Outcome::Fail(format!("paging from the last snapshot cursor: {reason}"))
        }
        Err(ChainError::TooLong) => Outcome::Fail(ChainError::TooLong.reason()),
    };
    report.add("orders.cursor-retained", ORDERS, changes_outcome);
    let versions = match (changes, chain(app, None, max).await) {
        (Ok(changes), Ok(second)) => compare(&first, &changes.orders)
            .and_then(|()| compare(&first, &second.orders))
            .map_or_else(Outcome::Fail, |()| Outcome::Pass),
        _ => {
            Outcome::Skip("needs the changes-since chain and a second snapshot to complete".into())
        }
    };
    report.add("orders.versions-only-increase", ORDERS, versions);
    seeded(app, seed, Some(&first), report).await;
    Ok(())
}

async fn limit_one(app: &App) -> Outcome {
    let first = match page(app, &[("limit", "1")], 1).await {
        Ok(first) => first,
        Err(reason) => return Outcome::Fail(reason),
    };
    if first.fields().complete {
        return Outcome::Pass;
    }
    let cursor = first.fields().next_cursor.as_str();
    match page(app, &[("limit", "1"), ("cursor", cursor)], 1).await {
        Ok(_) => Outcome::Pass,
        Err(reason) => Outcome::Fail(format!("second page: {reason}")),
    }
}

async fn above_maximum(app: &App, max: usize) -> Outcome {
    let above = max.saturating_add(1).to_string();
    match app.orders(&[("limit", &above)], Key::Correct).await {
        Err(error) => Outcome::Fail(error.to_string()),
        Ok(answer) if matches!(answer.status, 400 | 422) => Outcome::Pass,
        Ok(answer) if answer.status == 200 => {
            match analyte_contract::decode::<OrdersPage>(&answer.body) {
                Ok(page) if page.fields().orders.len() <= max => Outcome::Pass,
                Ok(page) => Outcome::Fail(format!(
                    "limit={above} returned {} orders; a page holds at most {max}",
                    page.fields().orders.len()
                )),
                Err(error) => {
                    Outcome::Fail(format!("not a valid contract v1 orders page: {error}"))
                }
            }
        }
        Ok(answer) => Outcome::Fail(format!(
            "expected HTTP 400 or 422, or a page of at most {max}; {}",
            answer.describe()
        )),
    }
}

async fn unknown_cursor(app: &App) -> Outcome {
    let cursor = format!("analyte-contract-check-{}", uuid::Uuid::now_v7());
    match app
        .orders(&[("limit", "1"), ("cursor", &cursor)], Key::Correct)
        .await
    {
        Err(error) => Outcome::Fail(error.to_string()),
        Ok(answer) if answer.status == 410 => {
            match analyte_contract::decode::<ErrorBody>(&answer.body) {
                Ok(body) if body.contract_version == 1 => Outcome::Pass,
                _ => Outcome::Fail(
                    "HTTP 410 without a contract_version 1 error body (error, detail)".into(),
                ),
            }
        }
        Ok(answer) => Outcome::Fail(format!(
            "expected HTTP 410 for an unknown cursor, so Analyte reloads a snapshot; got HTTP {}",
            answer.status
        )),
    }
}

/// Seeded-state cases; without a seed they are skipped with what to do.
async fn seeded(app: &App, seed: Option<&[Order]>, snapshot: Option<&Orders>, report: &mut Report) {
    const NAMES: [&str; 4] = [
        "seed.orders-present",
        "seed.cancellations",
        "seed.tombstones",
        "seed.paging",
    ];
    let Some(seed) = seed else {
        let skip = Outcome::Skip(format!(
            "load {SEED_FILE} into your app, then rerun with --seed-file {SEED_FILE}"
        ));
        for name in NAMES {
            report.add(name, ORDERS, skip.clone());
        }
        return;
    };
    let Some(snapshot) = snapshot else {
        for name in NAMES {
            report.add(
                name,
                ORDERS,
                Outcome::Skip("needs a complete snapshot (see orders.snapshot)".into()),
            );
        }
        return;
    };
    let live = |order: &&Order| !order.fields().tombstone;
    report.add(
        "seed.orders-present",
        ORDERS,
        present(seed.iter().filter(live), snapshot),
    );
    let cancelled: Vec<&Order> = seed
        .iter()
        .filter(live)
        .filter(|order| {
            use analyte_contract::document::OrderState;
            matches!(
                order.fields().state,
                OrderState::CancelRequested | OrderState::CancelConfirmed
            )
        })
        .collect();
    let outcome = if cancelled.is_empty() {
        Outcome::Skip("the seed file has no cancelled live order".into())
    } else {
        present(cancelled.into_iter(), snapshot)
    };
    report.add("seed.cancellations", ORDERS, outcome);
    report.add("seed.tombstones", ORDERS, tombstones(seed, snapshot));
    report.add("seed.paging", ORDERS, paging(app, seed).await);
}

/// Each seeded order is in the snapshot exactly as seeded: version, state, sample ID spelling and the rest.
fn present<'a>(seeded: impl Iterator<Item = &'a Order>, snapshot: &Orders) -> Outcome {
    for order in seeded {
        let id = order.fields().id.as_str();
        let Some(served) = snapshot.get(id) else {
            return Outcome::Fail(format!("seeded order {id} is missing from the snapshot"));
        };
        let expected = serde_json::to_value(order).unwrap_or(Value::Null);
        if *served != expected {
            return Outcome::Fail(format!(
                "seeded order {id} differs in {}",
                differing(&expected, served)
            ));
        }
    }
    Outcome::Pass
}

/// A seeded tombstone is either absent from the snapshot or still a tombstone; never live again.
fn tombstones(seed: &[Order], snapshot: &Orders) -> Outcome {
    let mut any = false;
    for order in seed.iter().filter(|order| order.fields().tombstone) {
        any = true;
        let id = order.fields().id.as_str();
        if let Some(served) = snapshot.get(id) {
            if served.get("tombstone") != Some(&Value::Bool(true)) {
                return Outcome::Fail(format!("tombstoned order {id} is served as a live order"));
            }
            if version(served) < order.fields().version.get() {
                return Outcome::Fail(format!(
                    "tombstoned order {id} went back to version {}",
                    version(served)
                ));
            }
        }
    }
    if any {
        Outcome::Pass
    } else {
        Outcome::Skip("the seed file has no tombstone".into())
    }
}

/// A limit=1 snapshot of two or more orders takes more than one page and loses none.
async fn paging(app: &App, seed: &[Order]) -> Outcome {
    let live: Vec<&str> = seed
        .iter()
        .filter(|order| !order.fields().tombstone)
        .map(|order| order.fields().id.as_str())
        .collect();
    if live.len() < 2 {
        return Outcome::Skip("needs at least two seeded live orders".into());
    }
    match chain(app, None, 1).await {
        Err(ChainError::TooLong) => Outcome::Skip(format!(
            "the app holds more than {MAX_PAGES} orders; limit=1 walk skipped"
        )),
        Err(ChainError::Broken(reason)) => Outcome::Fail(reason),
        Ok(chain) if chain.pages < 2 => {
            Outcome::Fail("a limit=1 snapshot of several orders ended after one page".into())
        }
        Ok(chain) => match live.iter().find(|id| {
            !chain
                .orders
                .iter()
                .any(|order| order.fields().id.as_str() == **id)
        }) {
            Some(id) => Outcome::Fail(format!("limit=1 paging lost seeded order {id}")),
            None => Outcome::Pass,
        },
    }
}

/// Later offers never go back in version, and one version keeps one content.
fn compare(earlier: &Orders, later: &[Order]) -> Result<(), String> {
    for order in later {
        let id = order.fields().id.as_str();
        let Some(before) = earlier.get(id) else {
            continue;
        };
        let (old, new) = (version(before), order.fields().version.get());
        if new < old {
            return Err(format!("order {id} went back from version {old} to {new}"));
        }
        if new == old && serde_json::to_value(order).ok().as_ref() != Some(before) {
            return Err(format!(
                "order {id} version {old} came back with different content"
            ));
        }
    }
    Ok(())
}

/// One snapshot holds the latest version of each order once.
fn unique(orders: &[Order]) -> Result<Orders, String> {
    let mut seen = Orders::new();
    for order in orders {
        let id = order.fields().id.as_str();
        let value = serde_json::to_value(order).map_err(|error| error.to_string())?;
        if seen.insert(id.to_owned(), value).is_some() {
            return Err(format!(
                "order {id} appears twice in one snapshot; a snapshot holds the latest version of each order once"
            ));
        }
    }
    Ok(seen)
}

async fn chain(app: &App, start: Option<&str>, limit: usize) -> Result<Chain, ChainError> {
    let limit_text = limit.to_string();
    let mut cursor = start.map(str::to_owned);
    let mut orders = Vec::new();
    for pages in 1..=MAX_PAGES {
        let mut query = vec![("limit", limit_text.as_str())];
        if let Some(cursor) = &cursor {
            query.push(("cursor", cursor.as_str()));
        }
        let page = page(app, &query, limit).await.map_err(ChainError::Broken)?;
        let fields = page.fields();
        if !fields.complete && cursor.as_deref() == Some(fields.next_cursor.as_str()) {
            return Err(ChainError::Broken(
                "next_cursor did not move on an incomplete page".into(),
            ));
        }
        orders.extend(fields.orders.iter().cloned());
        cursor = Some(fields.next_cursor.as_str().to_owned());
        if fields.complete {
            return Ok(Chain {
                orders,
                cursor: fields.next_cursor.as_str().to_owned(),
                pages,
            });
        }
    }
    Err(ChainError::TooLong)
}

/// One validated page: HTTP 200, a contract v1 orders page, at most `limit` orders.
async fn page(app: &App, query: &[(&str, &str)], limit: usize) -> Result<OrdersPage, String> {
    let answer = app
        .orders(query, Key::Correct)
        .await
        .map_err(|error| error.to_string())?;
    if answer.status != 200 {
        return Err(format!("expected HTTP 200; got HTTP {}", answer.status));
    }
    let page: OrdersPage = analyte_contract::decode(&answer.body)
        .map_err(|error| format!("not a valid contract v1 orders page: {error}"))?;
    let count = page.fields().orders.len();
    if count > limit {
        return Err(format!("limit={limit} returned {count} orders"));
    }
    Ok(page)
}

fn version(order: &Value) -> u64 {
    order.get("version").and_then(Value::as_u64).unwrap_or(0)
}

fn differing(expected: &Value, served: &Value) -> String {
    let fields: Vec<&str> = expected
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(field, value)| served.get(field.as_str()) != Some(value))
        .map(|(field, _)| field.as_str())
        .collect();
    if fields.is_empty() {
        "fields".into()
    } else {
        fields.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn order(version: u64, state: &str) -> Result<Order, Box<dyn std::error::Error>> {
        let mut value: Value = serde_json::from_slice(include_bytes!(
            "../../../../docs/contract/vectors/order.json"
        ))?;
        value["version"] = version.into();
        value["state"] = state.into();
        Ok(serde_json::from_value(value)?)
    }

    // Contract v1 "GET /orders": an order's version only increases; one version has one content
    #[test]
    fn version_going_back_or_changing_content_fails() -> Result<(), Box<dyn std::error::Error>> {
        let earlier = unique(&[order(2, "requested")?])?;
        assert!(compare(&earlier, &[order(2, "requested")?]).is_ok());
        assert!(compare(&earlier, &[order(3, "cancel_requested")?]).is_ok());
        let back = compare(&earlier, &[order(1, "requested")?]);
        assert!(back.is_err_and(|reason| reason.contains("went back from version 2 to 1")));
        let changed = compare(&earlier, &[order(2, "cancel_requested")?]);
        assert!(changed.is_err_and(|reason| reason.contains("different content")));
        assert!(unique(&[order(1, "requested")?, order(2, "requested")?]).is_err());
        Ok(())
    }
}
