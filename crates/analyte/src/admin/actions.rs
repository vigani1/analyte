//! Runs audited operator actions with GET confirmations and CSRF-checked POST requests.
use super::{pages::NAV, server::Context, sessions::Session};
use crate::admin::Error;
use crate::admin::markup::Markup;
use crate::deadline;
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

fn page(title: &str, body: &Markup) -> Response {
    Html(
        Markup::new()
            .markup(concat!(
                "<!doctype html><html lang=\"en\"><meta ",
                "charset=\"utf-8\"><title>",
            ))
            .text(title)
            .markup("</title>")
            .markup(NAV)
            .markup("<h1>")
            .text(title)
            .markup("</h1>")
            .fragment(body)
            .markup("</html>"),
    )
    .into_response()
}
fn failure(error: &dyn std::error::Error) -> Response {
    (
        StatusCode::CONFLICT,
        Html(
            Markup::new()
                .markup(NAV)
                .markup("<h1>Action refused</h1><p>")
                .text(error.to_string())
                .markup("</p>"),
        ),
    )
        .into_response()
}
fn id(text: &str) -> Result<DeliveryId, Error> {
    Ok(uuid::Uuid::parse_str(text)?.try_into()?)
}
fn form(session: &Session, route: &str, label: &str, superseded: bool) -> Markup {
    let acknowledgement = if superseded {
        concat!(
            concat!(
                "<label><input type=\"checkbox\" ",
                "name=\"acknowledge_superseded\" value=\"true\">I ",
            ),
            concat!(
                "explicitly request this delivery even if a newer ",
                "revision exists.</label>",
            ),
        )
    } else {
        ""
    };
    Markup::new()
        .markup("<form action=\"")
        .text(route)
        .markup(concat!(
            "\" method=\"post\"><input type=\"hidden\" name=\"csrf\" ",
            "value=\"",
        ))
        .text(session.csrf.form_value())
        .markup(concat!(
            "\"><input type=\"hidden\" name=\"confirm\" ",
            "value=\"yes\"><label>Reason <input name=\"reason\" ",
        ))
        .markup("required maxlength=\"256\"></label>")
        .markup(acknowledgement)
        .markup("<button>")
        .text(label)
        .markup("</button></form>")
}

pub(super) async fn deliveries(
    State(state): State<Arc<Context>>,
    Query(cursor): Query<super::pages::Cursor>,
) -> Response {
    let after = match i64::try_from(cursor.after) {
        Ok(after) => after,
        Err(error) => return failure(&error),
    };
    let result = state
        .store
        .request::<_, crate::admin::error::CommandError>(
            {
                match deadline() {
                    Ok(until) => until,
                    Err(error) => return failure(&error),
                }
            },
            move |s| Ok(s.deliveries(after)?),
        )
        .await;
    match result {
        Ok(rows) => {
            let rows: Vec<_> = rows.into_iter().take(super::pages::PAGE).collect();
            let mut html = Markup::new();
            for row in &rows {
                let delivery = uuid::Uuid::from(row.id);
                html.push_fragment(
                    &Markup::new()
                        .markup("<section><h2>")
                        .text(delivery)
                        .markup("</h2><p>Destination: ")
                        .text(row.destination.as_str())
                        .markup(". State: ")
                        .text(row.state)
                        .markup(".</p>"),
                );
                match row.state {
                    DeliveryState::Done | DeliveryState::Parked | DeliveryState::Paused => html
                        .push_fragment(
                            &Markup::new()
                                .markup("<a href=\"/deliveries/")
                                .text(delivery)
                                .markup("/resend\">Resend this delivery</a>"),
                        ),
                    DeliveryState::Pending | DeliveryState::InFlight | DeliveryState::Conflict => {}
                }
                html.push_markup("</section>");
            }
            if let Some(last) = rows.last() {
                html.push_fragment(
                    &Markup::new()
                        .markup("<a rel=\"next\" href=\"/deliveries?after=")
                        .text(last.sequence)
                        .markup("\">Next page</a>"),
                );
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
        .request::<_, crate::admin::error::CommandError>(
            {
                match deadline() {
                    Ok(until) => until,
                    Err(error) => return failure(&error),
                }
            },
            move |s| Ok(s.parked_receipt(parked)?),
        )
        .await
    {
        Ok(receipt) => page(
            "Confirm reprocess",
            &Markup::new()
                .markup("<p>Reinterpret original receipt ")
                .text(uuid::Uuid::from(receipt))
                .markup(concat!(
                    " with the current configuration. A new version is ",
                    "retained; results use the normal ",
                ))
                .markup("revision rules. This never sends work to a machine.</p>")
                .fragment(
                    &(form(
                        &session,
                        &format!("/parked/{parked}/reprocess"),
                        "Confirm reprocess",
                        false,
                    )),
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
        Err(error) => return failure(&error),
    };
    match state
        .store
        .request::<_, crate::admin::error::CommandError>(
            {
                match deadline() {
                    Ok(until) => until,
                    Err(error) => return failure(&error),
                }
            },
            move |s| {
                s.delivery(delivery)?
                    .ok_or(crate::admin::error::CommandError::from(
                        analyte_store::DeliveryError::UnknownDelivery,
                    ))
            },
        )
        .await
    {
        Ok(row) => page(
            "Confirm resend",
            &Markup::new()
                .markup("<p>Resend delivery ")
                .text(uuid::Uuid::from(delivery))
                .markup(" to ")
                .text(row.destination.as_str())
                .markup(" with its original ID and exact bytes. Recorded state: ")
                .text(row.state)
                .markup(concat!(
                    ". The app treats a repeat as already done. The ",
                    "store checks eligibility again on ",
                ))
                .markup("confirmation.</p>")
                .fragment(
                    &(form(
                        &session,
                        &format!("/deliveries/{}/resend", uuid::Uuid::from(delivery)),
                        "Confirm resend",
                        true,
                    )),
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
            return Err(Error::CurrentConfigurationPointsToADifferentStore);
        }
        crate::operator::recovery::reprocess(
            &state.store,
            &program,
            crate::operator::recovery::ReprocessTarget::Parked(parked),
            session.actor.clone(),
            reason,
        )
        .await
        .map_err(Error::from)
    }
    .await;
    match result {
        Ok(commit) => page(
            "Reprocess recorded",
            &Markup::new()
                .markup("<p>Interpretation ")
                .text(commit.version)
                .markup(" recorded. Decisions: ")
                .text(crate::operator::display::list(&commit.outcomes))
                .markup(concat!(
                    ". <a href=\"/parked\">Inspect remaining parked ",
                    "reasons</a>.</p>",
                )),
        ),
        Err(error) => failure(&error),
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
        .map_err(Error::from)
    }
    .await;
    match result {
        Ok(()) => page(
            "Resend queued",
            &Markup::new().markup(concat!(
                "<p>The original delivery ID and bytes are queued ",
                "for the app.</p>",
            )),
        ),
        Err(error) => failure(&error),
    }
}
