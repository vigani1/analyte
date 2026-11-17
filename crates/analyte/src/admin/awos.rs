//! Shows analyzer execution separately from command state on authenticated admin pages.
use super::{
    escape,
    pages::{Cursor, finish, next, start},
    server::Context,
};
use crate::admin::Error;
use crate::deadline;
use axum::{
    extract::{Path, Query, State},
    response::Response,
};
use std::sync::Arc;

pub(super) fn projection(item: &analyte_store::MachineOrderItem) -> String {
    match &item.execution {
        Some(p) => format!(
            "<p><strong>Execution: {}</strong>. Command state: {}. Origin: analyzer. Receipt: {} (interpretation {}). Host recorded: {}. Cancel request outstanding at receipt: {}. <a href=\"/awos/{}/{}\">Execution history and reconciliation evidence</a>.</p>",
            p.state,
            item.state,
            escape(&uuid::Uuid::from(p.receipt).to_string()),
            p.interpretation,
            p.recorded_at,
            p.cancel_outstanding,
            item.machine_order,
            item.index
        ),
        None => "<p>Execution: unknown (no execution evidence).</p>".into(),
    }
}

pub(super) async fn cancelled(
    State(state): State<Arc<Context>>,
    Query(cursor): Query<Cursor>,
) -> Response {
    finish(async {
        let after=i64::try_from(cursor.after)?;
        let rows=state.store.request(deadline()?,move |s|s.analyzer_cancelled_items(after)).await?;
        let mut html=start("Analyzer-cancelled AWOSs — reconciliation required");
        html.push_str("<p><strong>Review each affected AWOS with the lab and lab app.</strong> Analyzer CA does not confirm an app cancellation, cancel siblings or authorize another machine command. This condition is not sent to contract v1 apps.</p>");
        for row in &rows {
            html.push_str(&format!("<section><h2>Machine {} — order {} item {}</h2><p>App order: {}. Reserved version: {}.</p>{}",escape(row.order.key.machine.as_str()),row.order.id,row.item.index,escape(row.order.key.order.as_str()),row.order.version,projection(&row.item)));
            let destination=row.order.key.destination.clone();let order=row.order.key.order.clone();
            let cached=state.store.request(deadline()?,move |s|s.cached_order(&destination,&order)).await?;
            if let Some(cached)=cached {
                let document: serde_json::Value=serde_json::from_str(&cached.document)?;
                html.push_str(&format!("<p>Current app intent: {}. Reconcile any pending cancel through its original LAB-28 response; no CR is inferred from CA.</p>",escape(&document.get("state").map_or_else(||"unknown".into(),ToString::to_string))));
            }
            html.push_str("</section>");
        }
        if let Some(last)=rows.last() {next(&mut html,"/analyzer-cancelled",last.cursor);}
        Ok(html)
    }.await)
}

pub(super) async fn detail(
    State(state): State<Arc<Context>>,
    Path((order, item)): Path<(i64, u16)>,
    Query(cursor): Query<Cursor>,
) -> Response {
    finish(async {
        let after=i64::try_from(cursor.after)?;
        let (retained,rows)=state.store.request(deadline()?,move |s|Ok((s.machine_order_by_id(order)?,s.law_work_evidence(order,item,after)?))).await?;
        let item=retained.items.iter().find(|i|i.index==item).ok_or(Error::AwosItemNotRetained)?;
        let mut html=start("AWOS execution history");
        html.push_str(&projection(item));
        html.push_str("<p>Command acceptance and execution are independent. Estimate ordering is uncertain unless source evidence proves otherwise. Estimates keep source spelling and use only the MSH-7 zone; an absent estimate does not erase earlier evidence. Rejected message evidence below was not applied.</p>");
        for row in &rows {
            html.push_str(&format!("<section><h2>Receipt {} — interpretation {}</h2><p>Received: {}. Host recorded: {}. Message: {}. Disposition: {}.</p>",escape(&uuid::Uuid::from(row.receipt).to_string()),row.version,escape(row.received_at.as_str()),row.custody.recorded_at,escape(row.custody.message.as_str()),row.disposition));
            for work in row.custody.work.iter().filter(|w|w.order==order&&w.item==item.index) {
                html.push_str(&format!("<p>Execution: {}. Result-bearing completion evidence: {}.</p>{}",crate::operator::display::optional(work.execution),work.complete,source_evidence(work.source.as_str())));
            }
            html.push_str("</section>");
        }
        if let Some(last)=rows.last() {next(&mut html,&format!("/awos/{order}/{}",item.index),last.cursor);}
        Ok::<_,Error>(html)
    }.await)
}

fn source_evidence(source: &str) -> String {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(source) else {
        return format!("<pre>{}</pre>", escape(source));
    };
    let control = value
        .get("control")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    let delimiter = control.chars().nth(3).unwrap_or('|');
    let estimate = control.split(delimiter).nth(27).unwrap_or("");
    let estimate = match estimate {
        "" => "Absent ? earlier estimate evidence is retained",
        "\"\"" => "Explicit source null",
        supplied => supplied,
    };
    let header = value
        .get("header_time")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");
    let event = control
        .split(delimiter)
        .nth(9)
        .filter(|s| !s.is_empty())
        .unwrap_or("not supplied");
    let mut html = format!(
        "<dl><dt>Availability estimate as supplied</dt><dd>{}</dd><dt>Zone evidence (MSH-7)</dt><dd>{}</dd><dt>Source event time (ORC-9), as supplied</dt><dd>{}</dd></dl><ul>",
        escape(estimate),
        escape(header),
        escape(event)
    );
    if let Some(notes) = value.get("notes").and_then(serde_json::Value::as_array) {
        for note in notes.iter().filter_map(serde_json::Value::as_str) {
            html.push_str(&format!("<li>{}</li>", escape(note)));
        }
    }
    html.push_str("</ul>");
    html
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
            let source=serde_json::json!({"control":fields.join("|"),"header_time":"20260101120000+0130","notes":["NTE|||<script>patient</script>"]}).to_string();
            let html = source_evidence(&source);
            assert!(html.contains(expected));
            assert!(html.contains("20260101120000+0130"));
            assert!(html.contains("&lt;script&gt;patient&lt;/script&gt;"));
            assert!(!html.contains("<script>"));
        }
    }
}
