//! Shows analyzer execution separately from command state on authenticated admin pages.
use super::{
    pages::{Cursor, finish, next, start},
    server::Context,
};
use crate::admin::Error;
use crate::admin::markup::Markup;
use crate::deadline;
use axum::{
    extract::{Path, Query, State},
    response::Response,
};
use std::sync::Arc;

pub(super) fn projection(item: &analyte_store::MachineOrderItem) -> Markup {
    match &item.execution {
        Some(p) => Markup::new()
            .markup("<p><strong>Execution: ")
            .text(p.state)
            .markup("</strong>. Command state: ")
            .text(item.state)
            .markup(". Origin: analyzer. Receipt: ")
            .text(uuid::Uuid::from(p.receipt).to_string())
            .markup(" (interpretation ")
            .text(p.interpretation)
            .markup("). Host recorded: ")
            .text(p.recorded_at)
            .markup(". Cancel request outstanding at receipt: ")
            .text(p.cancel_outstanding)
            .markup(". <a href=\"/awos/")
            .text(item.machine_order)
            .markup("/")
            .text(item.index)
            .markup(concat!(
                "\">Execution history and reconciliation ",
                "evidence</a>.</p>",
            )),
        None => Markup::new().markup("<p>Execution: unknown (no execution evidence).</p>"),
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
        "" => "Absent ? earlier estimate evidence is retained",
        "\"\"" => "Explicit source null",
        supplied => supplied,
    };
    let header = value
        .get("header_time")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");
    let event = match control.event_time() {
        "" => "not supplied",
        supplied => supplied,
    };
    let mut html = Markup::new()
        .markup("<dl><dt>Availability estimate as supplied</dt><dd>")
        .text(estimate)
        .markup("</dd><dt>Zone evidence (MSH-7)</dt><dd>")
        .text(header)
        .markup(concat!(
            "</dd><dt>Source event time (ORC-9), as ",
            "supplied</dt><dd>",
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
    let rows = state
        .store
        .request(deadline()?, move |s| Ok(s.analyzer_cancelled_items(after)?))
        .await?;
    let mut html = start("Analyzer-cancelled AWOSs — reconciliation required");
    html.push_markup(concat!(
        concat!(
            "<p><strong>Review each affected AWOS with the lab ",
            "and lab app.</strong> Analyzer CA ",
        ),
        concat!(
            "does not confirm an app cancellation, cancel ",
            "siblings or authorize another machine ",
        ),
        concat!(
            "command. This condition is not sent to contract ",
            "v1 apps.</p>",
        ),
    ));
    for row in &rows {
        html.push_fragment(
            &Markup::new()
                .markup("<section><h2>Machine ")
                .text(row.order.key.machine.as_str())
                .markup(" — order ")
                .text(row.order.id)
                .markup(" item ")
                .text(row.item.index)
                .markup("</h2><p>App order: ")
                .text(row.order.key.order.as_str())
                .markup(". Reserved version: ")
                .text(row.order.version)
                .markup(".</p>")
                .fragment(&(projection(&row.item))),
        );
        let destination = row.order.key.destination.clone();
        let order = row.order.key.order.clone();
        let cached = state
            .store
            .request(deadline()?, move |s| {
                Ok(s.cached_order(&destination, &order)?)
            })
            .await?;
        if let Some(cached) = cached {
            let document: serde_json::Value = serde_json::from_str(&cached.document)?;
            html.push_fragment(
                &Markup::new()
                    .markup("<p>Current app intent: ")
                    .text(
                        document
                            .get("state")
                            .map_or_else(|| "unknown".into(), ToString::to_string),
                    )
                    .markup(concat!(
                        ". Reconcile any pending cancel through its ",
                        "original LAB-28 response; no CR is inferred ",
                    ))
                    .markup("from CA.</p>"),
            );
        }
        html.push_markup("</section>");
    }
    if let Some(last) = rows.last() {
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
        .request(deadline()?, move |s| {
            Ok((
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
    let mut html = start("AWOS execution history");
    html.push_fragment(&projection(item));
    html.push_markup(concat!(
        concat!(
            "<p>Command acceptance and execution are ",
            "independent. Estimate ordering is uncertain ",
        ),
        concat!(
            "unless source evidence proves otherwise. ",
            "Estimates keep source spelling and use only ",
        ),
        concat!(
            "the MSH-7 zone; an absent estimate does not erase ",
            "earlier evidence. Rejected message ",
        ),
        "evidence below was not applied.</p>",
    ));
    for row in &rows {
        html.push_fragment(
            &Markup::new()
                .markup("<section><h2>Receipt ")
                .text(uuid::Uuid::from(row.receipt).to_string())
                .markup(" — interpretation ")
                .text(row.version)
                .markup("</h2><p>Received: ")
                .text(row.received_at.as_str())
                .markup(". Host recorded: ")
                .text(row.custody.recorded_at)
                .markup(". Message: ")
                .text(row.custody.message.as_str())
                .markup(". Disposition: ")
                .text(row.disposition)
                .markup(".</p>"),
        );
        for work in row
            .custody
            .work
            .iter()
            .filter(|w| w.order == order && w.item == item.index)
        {
            html.push_fragment(
                &Markup::new()
                    .markup("<p>Execution: ")
                    .text(crate::operator::display::optional(work.execution))
                    .markup(". Result-bearing completion evidence: ")
                    .text(work.complete)
                    .markup(".</p>")
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
    #[test]
    fn adr0016_protected_estimates_keep_null_absence_and_escape_scoped_source_notes() {
        for (estimate, expected) in [
            ("", "Absent"),
            ("\"\"", "Explicit source null"),
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
