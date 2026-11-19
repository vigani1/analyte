//! Shows analyzer execution separately from command state on authenticated admin pages.
use super::{
    pages::{Cursor, finish, next},
    server::Context,
    view::{self, Tab, plain},
};
use crate::admin::Error;
use crate::admin::markup::Markup;
use crate::deadline;
use axum::{
    extract::{Path, Query, State},
    response::Response,
};
use std::sync::Arc;

/// What the machine reported doing with one order item, kept apart from the order's own state.
pub(super) fn projection(item: &analyte_store::MachineOrderItem) -> Markup {
    match &item.execution {
        Some(p) => Markup::new()
            .markup("<p title=\"Execution reported by the analyzer\">Item ")
            .text(item.index)
            .markup(", what the machine reports: <strong>")
            .text(p.state)
            .markup("</strong> <span class=\"muted\">(order status: ")
            .text(plain::order_state(item.state))
            .markup("; reported in message <span title=\"Receipt ")
            .text(uuid::Uuid::from(p.receipt).to_string())
            .markup(", interpretation ")
            .text(p.interpretation)
            .markup("\">")
            .text(
                uuid::Uuid::from(p.receipt)
                    .to_string()
                    .chars()
                    .take(8)
                    .collect::<String>(),
            )
            .markup("</span>, recorded ")
            .fragment(&plain::when_unix(p.recorded_at, jiff::Timestamp::now()))
            .markup("; a cancel request was pending then: ")
            .text(plain::yes_no(p.cancel_outstanding))
            .markup(")</span> <a href=\"/awos/")
            .text(item.machine_order)
            .markup("/")
            .text(item.index)
            .markup(
                "\" title=\"Execution history and reconciliation evidence\">Item history</a></p>",
            ),
        None => Markup::new()
            .markup("<p title=\"No execution evidence\">Item ")
            .text(item.index)
            .markup(", what the machine reports: nothing yet.</p>"),
    }
}

pub(super) async fn cancelled(
    State(state): State<Arc<Context>>,
    Query(cursor): Query<Cursor>,
) -> Response {
    let result = cancelled_page(&state, cursor).await;

    finish(result)
}

pub(super) async fn detail(
    State(state): State<Arc<Context>>,
    Path((order, item)): Path<(i64, u16)>,
    Query(cursor): Query<Cursor>,
) -> Response {
    let result = detail_page(&state, order, item, cursor).await;

    finish(result)
}

fn source_evidence(source: &str) -> Markup {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(source) else {
        return Markup::new().markup("<pre>").text(source).markup("</pre>");
    };
    let control = value
        .get("control")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    let Ok(control) = analyte_hl7::law::RetainedControl::parse(control) else {
        return Markup::new().markup("<pre>").text(source).markup("</pre>");
    };
    let estimate = control.estimate();
    let estimate = match estimate {
        "" => "Not sent; the earlier expected time is kept",
        "\"\"" => "Sent as empty (cleared by the machine)",
        supplied => supplied,
    };
    let header = value
        .get("header_time")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("not known");
    let event = match control.event_time() {
        "" => "not sent",
        supplied => supplied,
    };
    let mut html = Markup::new()
        .markup("<dl><dt title=\"Availability estimate as supplied\">Expected ready time, as sent</dt><dd>")
        .text(estimate)
        .markup("</dd><dt title=\"Zone evidence (MSH-7)\">Message time and zone</dt><dd>")
        .text(header)
        .markup(concat!(
            "</dd><dt title=\"Source event time (ORC-9), as supplied\">Event time, as ",
            "sent</dt><dd>",
        ))
        .text(event)
        .markup("</dd></dl><ul>");
    if let Some(notes) = value.get("notes").and_then(serde_json::Value::as_array) {
        for note in notes.iter().filter_map(serde_json::Value::as_str) {
            html.push_fragment(&Markup::new().markup("<li>").text(note).markup("</li>"));
        }
    }
    html.push_markup("</ul>");
    html
}

async fn cancelled_page(state: &Context, cursor: Cursor) -> Result<Markup, Error> {
    let after = i64::try_from(cursor.after)?;
    let (rows, more) = state
        .store
        .request(deadline()?, move |s| {
            let rows = s.analyzer_cancelled_items(after)?;
            let more = match rows.last() {
                Some(last) => !s.analyzer_cancelled_items(last.cursor)?.is_empty(),
                None => false,
            };
            Ok::<_, analyte_store::ReconciliationError>((rows, more))
        })
        .await?;
    let mut html = view::page(Tab::CancelledByMachine, "Cancelled by machine");
    html.push_markup(concat!(
        "<p class=\"notice\" title=\"Analyzer-cancelled AWOSs; reconciliation required\">",
        "<strong>Check each order below with the lab and the lab app.</strong> The machine ",
        "cancelled these tests on its own. That does not cancel them in the lab app, does not ",
        "cancel the order's other tests and does not let Analyte send anything else. The lab ",
        "app is not told automatically.</p>",
    ));
    if rows.is_empty() {
        html.push_markup("<p class=\"muted\">No order was cancelled by a machine.</p>");
    }
    for row in &rows {
        html.push_fragment(
            &Markup::new()
                .markup("<section><h2 title=\"Machine order ")
                .text(row.order.id)
                .markup(", item ")
                .text(row.item.index)
                .markup("\">Order ")
                .text(row.order.key.order.as_str())
                .markup(" on ")
                .text(row.order.key.machine.as_str())
                .markup("</h2><p class=\"muted\" title=\"Reserved version\">Order version sent: ")
                .text(row.order.version)
                .markup("</p>")
                .fragment(&(projection(&row.item))),
        );
        let destination = row.order.key.destination.clone();
        let order = row.order.key.order.clone();
        let cached = state
            .store
            .request(deadline()?, move |s| s.cached_order(&destination, &order))
            .await?;
        if let Some(cached) = cached {
            let document: serde_json::Value = serde_json::from_str(&cached.document)?;
            html.push_fragment(
                &Markup::new()
                    .markup("<p title=\"Current app intent\">The lab app's current status: ")
                    .text(
                        document
                            .get("state")
                            .map_or_else(|| "not known".into(), ToString::to_string),
                    )
                    .markup(concat!(
                        ". <span title=\"Reconcile through the original LAB-28 response; ",
                        "no CR is inferred from CA\">If a cancel request from the lab app is ",
                        "pending, settle it with the lab app's own answer; Analyte does not ",
                        "count the machine's cancel as that answer.</span></p>",
                    )),
            );
        }
        html.push_markup("</section>");
    }
    if let Some(last) = rows.last()
        && more
    {
        next(&mut html, "/analyzer-cancelled", last.cursor);
    }
    Ok(html)
}

async fn detail_page(
    state: &Context,
    order: i64,
    item: u16,
    cursor: Cursor,
) -> Result<Markup, Error> {
    let after = i64::try_from(cursor.after)?;
    let (retained, rows) = state
        .store
        .request::<_, analyte_store::ReconciliationError>(deadline()?, move |s| {
            Ok::<_, analyte_store::ReconciliationError>((
                s.machine_order_by_id(order)?,
                s.law_work_evidence(order, item, after)?,
            ))
        })
        .await?;
    let item = retained
        .items
        .iter()
        .find(|i| i.index == item)
        .ok_or(Error::AwosItemNotRetained)?;
    let mut html = view::page(Tab::MachineOrders, "Order item history");
    html.push_fragment(&projection(item));
    html.push_markup(concat!(
        "<p class=\"lead\" title=\"AWOS execution history\">What the machine accepted and ",
        "what it actually did are shown separately. Times keep the machine's own spelling and ",
        "time zone; when the machine leaves a time out, the earlier one is kept. Messages ",
        "marked as rejected were not applied.</p>",
    ));
    for row in &rows {
        html.push_fragment(
            &Markup::new()
                .markup("<section><h2 title=\"Receipt ")
                .text(uuid::Uuid::from(row.receipt).to_string())
                .markup(", interpretation ")
                .text(row.version)
                .markup("\">Message received ")
                .text(row.received_at.as_str())
                .markup("</h2><dl>")
                .fragment(&view::row(
                    "Recorded by Analyte",
                    Some("Host recorded"),
                    &Markup::new().text(row.custody.recorded_at),
                ))
                .fragment(&view::row(
                    "Message",
                    None,
                    &Markup::new().text(row.custody.message.as_str()),
                ))
                .fragment(&view::row(
                    "Outcome",
                    Some("Disposition"),
                    &Markup::new().text(row.disposition),
                ))
                .markup("</dl>"),
        );
        for work in row
            .custody
            .work
            .iter()
            .filter(|w| w.order == order && w.item == item.index)
        {
            html.push_fragment(
                &Markup::new()
                    .markup("<p title=\"Execution\">What the machine reports: ")
                    .text(crate::operator::display::optional(work.execution))
                    .markup(". <span title=\"Result-bearing completion evidence\">Results came with it: ")
                    .text(plain::yes_no(work.complete))
                    .markup("</span>.</p>")
                    .fragment(&(source_evidence(work.source.as_str()))),
            );
        }
        html.push_markup("</section>");
    }
    if let Some(last) = rows.last() {
        next(
            &mut html,
            &format!("/awos/{order}/{}", item.index),
            last.cursor,
        );
    }
    Ok::<_, Error>(html)
}

#[cfg(test)]
mod tests {
    use super::*;
    // ADR 0016
    #[test]
    fn estimates_keep_null_and_absence_and_escape_notes() {
        for (estimate, expected) in [
            ("", "Not sent; the earlier expected time is kept"),
            ("\"\"", "Sent as empty (cleared by the machine)"),
            ("20260101", "20260101"),
        ] {
            let mut fields = vec![""; 28];
            fields[0] = "ORC";
            fields[27] = estimate;
            let source = serde_json::json!({
                "control": fields.join("|"),
                "header_time": "20260101120000+0130",
                "notes": ["NTE|||<script>patient</script>"],
            })
            .to_string();
            let html = source_evidence(&source);
            assert!(html.contains(expected));
            assert!(html.contains("20260101120000+0130"));
            assert!(html.contains("&lt;script&gt;patient&lt;/script&gt;"));
            assert!(!html.contains("<script>"));
        }
    }
}
