//! Runs audited operator actions with GET confirmations and CSRF-checked POST requests.
use super::{escape, pages::NAV, server::Context, sessions::Session};
use crate::{Error, deadline};
use analyte_model::{DeliveryId, Reason};
use analyte_store::DeliveryState;
use axum::{
    Extension, Form,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Confirmation {
    csrf: String,
    reason: String,
    confirm: String,
    #[serde(default)]
    acknowledge_superseded: bool,
}

fn page(title: &str, body: &str) -> Response {
    Html(format!("<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><title>{}</title>{NAV}<h1>{}</h1>{body}</html>", escape(title), escape(title))).into_response()
}
fn failure(error: &dyn std::error::Error) -> Response {
    (
        StatusCode::CONFLICT,
        Html(format!(
            "{NAV}<h1>Action refused</h1><p>{}</p>",
            escape(&error.to_string())
        )),
    )
        .into_response()
}
fn id(text: &str) -> Result<DeliveryId, Error> {
    Ok(uuid::Uuid::parse_str(text)?.try_into()?)
}
fn form(session: &Session, route: &str, label: &str, superseded: bool) -> String {
    let acknowledgement = if superseded {
        "<label><input type=\"checkbox\" name=\"acknowledge_superseded\" value=\"true\">I explicitly request this delivery even if a newer revision exists.</label>"
    } else {
        ""
    };
    format!(
        "<form action=\"{}\" method=\"post\"><input type=\"hidden\" name=\"csrf\" value=\"{}\"><input type=\"hidden\" name=\"confirm\" value=\"yes\"><label>Reason <input name=\"reason\" required maxlength=\"256\"></label>{acknowledgement}<button>{}</button></form>",
        escape(route),
        escape(session.csrf.form_value()),
        escape(label)
    )
}

pub(super) async fn deliveries(
    State(state): State<Arc<Context>>,
    Query(cursor): Query<super::pages::Cursor>,
) -> Response {
    let result = state
        .store
        .request(deadline(), move |s| {
            s.deliveries(
                i64::try_from(cursor.after).map_err(|_| analyte_store::Error::InvalidHostInput)?,
            )
        })
        .await;
    match result {
        Ok(rows) => {
            let rows: Vec<_> = rows.into_iter().take(super::pages::PAGE).collect();
            let mut html = String::new();
            for row in &rows {
                let delivery = uuid::Uuid::from(row.id);
                html.push_str(&format!(
                    "<section><h2>{delivery}</h2><p>Destination: {}. State: {}.</p>",
                    escape(row.destination.as_str()),
                    row.state
                ));
                match row.state {
                    DeliveryState::Done | DeliveryState::Parked | DeliveryState::Paused => html
                        .push_str(&format!(
                            "<a href=\"/deliveries/{delivery}/resend\">Resend this delivery</a>"
                        )),
                    DeliveryState::Pending | DeliveryState::InFlight | DeliveryState::Conflict => {}
                }
                html.push_str("</section>");
            }
            if let Some(last) = rows.last() {
                html.push_str(&format!(
                    "<a rel=\"next\" href=\"/deliveries?after={}\">Next page</a>",
                    last.sequence
                ));
            }
            page("Deliveries", &html)
        }
        Err(error) => failure(&error),
    }
}

pub(super) async fn confirm_reprocess(
    State(state): State<Arc<Context>>,
    Path(parked): Path<i64>,
    session: Option<Extension<Arc<Session>>>,
) -> Response {
    let Some(Extension(session)) = session.filter(|Extension(s)| s.authenticated) else {
        return (StatusCode::FORBIDDEN, "authenticated session required").into_response();
    };
    match state
        .store
        .request(deadline(), move |s| s.parked_receipt(parked))
        .await
    {
        Ok(receipt) => page(
            "Confirm reprocess",
            &format!(
                "<p>Reinterpret original receipt {} with the current configuration. A new version is retained; results use the normal revision rules. This never sends work to a machine.</p>{}",
                uuid::Uuid::from(receipt),
                form(
                    &session,
                    &format!("/parked/{parked}/reprocess"),
                    "Confirm reprocess",
                    false
                )
            ),
        ),
        Err(error) => failure(&error),
    }
}

pub(super) async fn confirm_resend(
    State(state): State<Arc<Context>>,
    Path(text): Path<String>,
    session: Option<Extension<Arc<Session>>>,
) -> Response {
    let Some(Extension(session)) = session.filter(|Extension(s)| s.authenticated) else {
        return (StatusCode::FORBIDDEN, "authenticated session required").into_response();
    };
    let delivery = match id(&text) {
        Ok(id) => id,
        Err(error) => return failure(error.as_ref()),
    };
    match state
        .store
        .request(deadline(), move |s| {
            s.delivery(delivery)?
                .ok_or(analyte_store::Error::UnknownDelivery)
        })
        .await
    {
        Ok(row) => page(
            "Confirm resend",
            &format!(
                "<p>Resend delivery {} to {} with its original ID and exact bytes. Recorded state: {}. The app treats a repeat as already done. The store checks eligibility again on confirmation.</p>{}",
                uuid::Uuid::from(delivery),
                escape(row.destination.as_str()),
                row.state,
                form(
                    &session,
                    &format!("/deliveries/{}/resend", uuid::Uuid::from(delivery)),
                    "Confirm resend",
                    true
                )
            ),
        ),
        Err(error) => failure(&error),
    }
}

fn valid(session: &Session, form: &Confirmation) -> bool {
    session.authenticated && session.csrf_valid(&form.csrf) && form.confirm == "yes"
}

pub(super) async fn reprocess(
    State(state): State<Arc<Context>>,
    Path(parked): Path<i64>,
    Extension(session): Extension<Arc<Session>>,
    Form(form): Form<Confirmation>,
) -> Response {
    if !valid(&session, &form) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let result = async {
        let reason = Reason::new(&form.reason)?;
        let path = state.config_path.clone();
        let program =
            tokio::task::spawn_blocking(move || crate::installation::config::Program::load(&path))
                .await??;
        if program.data_directory().canonicalize()? != state.directory.canonicalize()? {
            return Err(crate::invalid(
                "current configuration points to a different store",
            ));
        }
        crate::operator::recovery::reprocess(
            &state.store,
            &program,
            crate::operator::recovery::ReprocessTarget::Parked(parked),
            session.actor.clone(),
            reason,
        )
        .await
    }
    .await;
    match result {
        Ok(commit) => page(
            "Reprocess recorded",
            &format!(
                "<p>Interpretation {} recorded. Decisions: {}. <a href=\"/parked\">Inspect remaining parked reasons</a>.</p>",
                commit.version,
                crate::operator::display::list(&commit.outcomes)
            ),
        ),
        Err(error) => failure(error.as_ref()),
    }
}

pub(super) async fn resend(
    State(state): State<Arc<Context>>,
    Path(text): Path<String>,
    Extension(session): Extension<Arc<Session>>,
    Form(form): Form<Confirmation>,
) -> Response {
    if !valid(&session, &form) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let result = async {
        crate::operator::recovery::resend(
            &state.store,
            id(&text)?,
            form.acknowledge_superseded,
            session.actor.clone(),
            Reason::new(&form.reason)?,
        )
        .await
    }
    .await;
    match result {
        Ok(()) => page(
            "Resend queued",
            "<p>The original delivery ID and bytes are queued for the app.</p>",
        ),
        Err(error) => failure(error.as_ref()),
    }
}
