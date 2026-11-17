//! Shows retention policy and committed purge evidence and applies audited operator holds.
use super::{escape, pages::NAV, server::Context, sessions::Session};
use crate::admin::Error;
use crate::deadline;
use analyte_model::{Identifier, Reason};
use analyte_store::{Audit, HoldTarget};
use axum::{
    Extension, Form,
    extract::{Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};
use serde::Deserialize;
use std::sync::Arc;

fn page(body: &str) -> Response {
    Html(format!("<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><title>Retention and holds</title>{NAV}<h1>Retention and holds</h1>{body}</html>")).into_response()
}
fn failure(error: &dyn std::error::Error) -> Response {
    (
        StatusCode::CONFLICT,
        Html(format!(
            "{NAV}<h1>Retention action refused</h1><p>{}</p>",
            escape(&error.to_string())
        )),
    )
        .into_response()
}
pub(super) async fn show(
    State(state): State<Arc<Context>>,
    Query(cursor): Query<super::pages::Cursor>,
    session: Option<Extension<Arc<Session>>>,
) -> Response {
    match render(
        &state,
        session.as_ref().map(|Extension(s)| s.as_ref()),
        cursor.after,
    )
    .await
    {
        Ok(body) => page(&body),
        Err(error) => failure(&error),
    }
}
async fn render(state: &Context, session: Option<&Session>, after: u64) -> Result<String, Error> {
    let after = i64::try_from(after)?;
    let (last, oldest, holds) = state
        .store
        .request(deadline()?, move |store| {
            Ok((
                store.last_purge_run()?,
                store.oldest_retained_receipt()?,
                store.retention_holds(after)?,
            ))
        })
        .await?;
    let mut html = "<h2>Policy</h2><ul>".to_owned();
    for (machine, days) in &state.retention {
        let policy = days.map_or_else(
            || "keep (never purge)".into(),
            |days| format!("{days} days after the last acknowledgment"),
        );
        html.push_str(&format!(
            "<li>{}: {}</li>",
            escape(machine.as_str()),
            escape(&policy)
        ));
    }
    html.push_str("</ul><h2>Last run</h2>");
    if let Some(run) = last {
        html.push_str(&format!(
            "<p>Unix time {}. Examined: {}. Purged receipts: {}. Purged orders: {}. Paused: {}. Failed: {}.</p>",
            run.time, run.examined, run.purged, run.orders, run.paused, run.failed
        ));
    } else {
        html.push_str("<p>No retention run recorded.</p>");
    }
    html.push_str(&format!(
        "<h2>Oldest retained item</h2><p>{}</p>",
        oldest.map_or_else(
            || "No retained receipts.".into(),
            |time| format!("Received at Unix time {time}.")
        )
    ));
    html.push_str("<h2>Active holds</h2><p>Holds have no expiry. An order hold covers linked receipts. Uncertain links retain additional data.</p><ul>");
    let next = holds.last().map(|hold| hold.sequence);
    for hold in holds {
        html.push_str(&format!(
            "<li>{}</li>",
            escape(&serde_json::to_string(&hold.target)?)
        ));
    }
    html.push_str("</ul>");
    if let Some(after) = next {
        html.push_str(&format!(
            "<a rel=\"next\" href=\"/retention?after={after}\">Next holds page</a>"
        ));
    }
    if let Some(session) = session.filter(|s| s.authenticated) {
        html.push_str(&format!(r#"<h2>Place or release a hold</h2><form action="/retention" method="post"><input type="hidden" name="csrf" value="{}"><label>Scope <select name="scope"><option value="receipt">Receipt</option><option value="order">Order</option></select></label><label>Receipt UUID or app order ID <input name="id" required maxlength="256"></label><label>Destination (orders only) <input name="destination" maxlength="256"></label><label>Action <select name="operation"><option value="place">Place hold</option><option value="release">Release hold</option></select></label><label>Reason <input name="reason" required maxlength="256"></label><label><input type="checkbox" name="confirm" value="yes" required>I confirm this hold change.</label><button>Record hold change</button></form>"#,escape(session.csrf.form_value())));
    }
    Ok(html)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct HoldForm {
    csrf: String,
    scope: String,
    id: String,
    destination: String,
    operation: String,
    reason: String,
    confirm: String,
}
pub(super) async fn hold(
    State(state): State<Arc<Context>>,
    Extension(session): Extension<Arc<Session>>,
    Form(form): Form<HoldForm>,
) -> Response {
    if !session.authenticated || !session.csrf_valid(&form.csrf) || form.confirm != "yes" {
        return StatusCode::FORBIDDEN.into_response();
    }
    let result = apply(&state, &session, form).await;
    match result {
        Ok(()) => page("<p>Hold change recorded with your actor identity and reason.</p>"),
        Err(error) => failure(&error),
    }
}
async fn apply(state: &Context, session: &Session, form: HoldForm) -> Result<(), Error> {
    let target = match form.scope.as_str() {
        "receipt" => {
            HoldTarget::Receipt(serde_json::from_value(serde_json::Value::String(form.id))?)
        }
        "order" => HoldTarget::Order {
            destination: Identifier::new(&form.destination)?,
            order: Identifier::new(&form.id)?,
        },
        _ => return Err(Error::ChooseReceiptOrOrder),
    };
    let active = match form.operation.as_str() {
        "place" => true,
        "release" => false,
        _ => return Err(Error::ChoosePlaceOrRelease),
    };
    let audit = Audit {
        actor: session.actor.clone(),
        action: Identifier::new(if active {
            "retention-hold-place"
        } else {
            "retention-hold-release"
        })?,
        targets: Vec::new(),
        time: jiff::Timestamp::now().as_second(),
        reason: Reason::new(&form.reason)?,
    };
    state
        .store
        .request(deadline()?, move |store| {
            store.set_retention_hold(&target, active, &audit)
        })
        .await?;
    Ok(())
}
