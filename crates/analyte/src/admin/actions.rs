//! Runs audited operator actions with GET confirmations and CSRF-checked POST requests.
use super::{
    server::Context,
    sessions::Session,
    view::{self, Tab, Tone, exact, plain},
};
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

fn page(tab: Tab, title: &str, body: &Markup) -> Response {
    Html(view::page(tab, title).fragment(body).markup(view::END)).into_response()
}
fn failure(error: &dyn std::error::Error) -> Response {
    view::refusal(
        StatusCode::CONFLICT,
        "That could not be done",
        concat!(
            "Nothing was changed. Check the page it came from and try again; if this keeps ",
            "happening, give the details below to support.",
        ),
        error,
    )
}
/// The refusal for a confirmation page opened without a signed-in browser session.
fn sign_in_first() -> Response {
    view::refusal(
        StatusCode::FORBIDDEN,
        "Sign in first",
        "This action needs a signed-in browser session. Sign in, then open the page again.",
        "authenticated session required",
    )
}
fn id(text: &str) -> Result<DeliveryId, Error> {
    Ok(uuid::Uuid::parse_str(text)?.try_into()?)
}
fn form(session: &Session, route: &str, label: &str, superseded: bool) -> Markup {
    let acknowledgement = if superseded {
        concat!(
            concat!(
                "<label class=\"check\" title=\"Acknowledge superseded revision\"><input ",
                "type=\"checkbox\" name=\"acknowledge_superseded\" value=\"true\">",
            ),
            concat!("Send it even if a newer correction of this result exists.</label>",),
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
            "value=\"yes\"><label class=\"field\"><span>Reason</span><input name=\"reason\" ",
        ))
        .markup(concat!(
            "required maxlength=\"256\"></label><p class=\"muted\">Your reason is recorded ",
            "with the action.</p>",
        ))
        .markup(acknowledgement)
        .markup("<div class=\"actions-row\"><button>")
        .text(label)
        .markup("</button></div></form>")
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
            move |s| {
                let rows = s.deliveries(after)?;
                let mut more = rows.len() > super::pages::PAGE;
                let rows: Vec<_> = rows.into_iter().take(super::pages::PAGE).collect();
                if let Some(last) = rows.last()
                    && !more
                {
                    more = !s.deliveries(last.sequence)?.is_empty();
                }
                Ok((rows, more))
            },
        )
        .await;
    match result {
        Ok((rows, more)) => {
            let mut html = Markup::new().markup(concat!(
                "<p class=\"lead\">Results and order messages Analyte sends to a lab app. Each ",
                "keeps the same delivery number on every retry, so the lab app never files it ",
                "twice.</p>",
            ));
            if rows.is_empty() {
                html.push_markup("<p class=\"muted\">No deliveries to show.</p>");
            } else {
                html.push_markup(concat!(
                    "<section><div class=\"table\"><table class=\"stack\"><tr>",
                    "<th title=\"Delivery ID\">Delivery number</th>",
                    "<th title=\"Destination\">Lab app</th><th>Status</th><th></th></tr>",
                ));
            }
            for row in &rows {
                let delivery = uuid::Uuid::from(row.id);
                let tone = match row.state {
                    DeliveryState::Done => Tone::Good,
                    DeliveryState::Pending | DeliveryState::InFlight => Tone::Neutral,
                    DeliveryState::Parked | DeliveryState::Paused | DeliveryState::Conflict => {
                        Tone::Attention
                    }
                };
                html.push_fragment(
                    &Markup::new()
                        .markup("<tr><td><code>")
                        .text(delivery)
                        .markup("</code></td><td data-label=\"Lab app\">")
                        .text(row.destination.as_str())
                        .markup("</td><td data-label=\"Status\"><span title=\"")
                        .text(row.state)
                        .markup("\">")
                        .fragment(&view::pill(tone, plain::delivery_state(row.state)))
                        .markup("</span></td><td class=\"actions\">"),
                );
                match row.state {
                    DeliveryState::Done | DeliveryState::Parked | DeliveryState::Paused => html
                        .push_fragment(
                            &Markup::new()
                                .markup("<a href=\"/deliveries/")
                                .text(delivery)
                                .markup("/resend\" title=\"Resend this delivery\">Send again</a>"),
                        ),
                    DeliveryState::Pending | DeliveryState::InFlight | DeliveryState::Conflict => {}
                }
                html.push_markup("</td></tr>");
            }
            if let Some(last) = rows.last() {
                html.push_markup("</table></div></section>");
                if more {
                    super::pages::next(&mut html, "/deliveries", last.sequence);
                }
            }
            page(Tab::Deliveries, "Deliveries", &html)
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
        return sign_in_first();
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
            Tab::NeedsReview,
            "Process this message again?",
            &Markup::new()
                .markup("<section><p title=\"Reprocess: reinterpret the original receipt\">")
                .markup("Analyte reads the original message <code>")
                .text(uuid::Uuid::from(receipt))
                .markup(concat!(
                    "</code> again with the current settings. The earlier reading is kept and ",
                    "a new one is added; corrected results follow the usual correction rules. ",
                    "Nothing is sent to the machine.</p>",
                ))
                .fragment(
                    &(form(
                        &session,
                        &format!("/parked/{parked}/reprocess"),
                        "Process again",
                        false,
                    )),
                )
                .markup("</section>"),
        ),
        Err(error) => failure(&error),
    }
}

pub(super) async fn confirm_resolve(
    State(state): State<Arc<Context>>,
    Path(parked): Path<i64>,
    session: Option<Extension<Arc<Session>>>,
) -> Response {
    let Some(Extension(session)) = session.filter(|Extension(s)| s.authenticated) else {
        return sign_in_first();
    };
    let until = match deadline() {
        Ok(until) => until,
        Err(error) => return failure(&error),
    };
    let item = state
        .store
        .request::<_, crate::admin::error::CommandError>(until, move |s| Ok(s.parked_item(parked)?))
        .await;
    match item {
        Ok(Some(item)) if item.resolution.is_none() => page(
            Tab::NeedsReview,
            "Mark this item resolved?",
            &Markup::new()
                .markup("<section><p title=\"Resolve parked item ")
                .text(parked)
                .markup("\">Item ")
                .text(parked)
                .markup(concat!(
                    " leaves the list of items waiting for review. Nothing is deleted and ",
                    "nothing is sent to a machine or the lab app. The item stays in the ",
                    "history, marked resolved by you with your reason.</p>",
                ))
                .fragment(&form(
                    &session,
                    &format!("/parked/{parked}/resolve"),
                    "Mark resolved",
                    false,
                ))
                .markup("</section>"),
        ),
        Ok(Some(_)) => view::refusal(
            StatusCode::CONFLICT,
            "Already resolved",
            "This item is already resolved. Open it from Needs review to see who resolved it and why.",
            "parked item already resolved",
        ),
        Ok(None) => view::refusal(
            StatusCode::NOT_FOUND,
            "Item not found",
            "There is no item to review with this number. Go back to Needs review and pick one from the list.",
            "parked item not found",
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
        return sign_in_first();
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
            Tab::Deliveries,
            "Send this delivery again?",
            &Markup::new()
                .markup(
                    "<section><p title=\"Resend with the original delivery ID and exact bytes\">",
                )
                .markup("Analyte sends delivery <code>")
                .text(uuid::Uuid::from(delivery))
                .markup("</code> to lab app ")
                .text(row.destination.as_str())
                .markup(" again, unchanged and with the same delivery number. Its status now: ")
                .fragment(&exact(
                    plain::delivery_state(row.state),
                    &row.state.to_string(),
                ))
                .markup(concat!(
                    ". If the lab app already has it, it treats the repeat as done. Analyte ",
                    "checks again that it may be sent when you confirm.</p>",
                ))
                .fragment(
                    &(form(
                        &session,
                        &format!("/deliveries/{}/resend", uuid::Uuid::from(delivery)),
                        "Send again",
                        true,
                    )),
                )
                .markup("</section>"),
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
        return sign_in_first();
    }
    let result = async {
        let reason = Reason::new(&form.reason)?;
        let path = state.config_path.clone();
        let program =
            tokio::task::spawn_blocking(move || crate::installation::config::Program::load(&path))
                .await??;
        if program.data_directory().canonicalize()? != state.directory.canonicalize()? {
            return Err(Error::StoreMismatch);
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
            Tab::NeedsReview,
            "Processed again",
            &Markup::new()
                .markup("<p class=\"notice info\" title=\"Reprocess recorded\">Reading ")
                .text(commit.version)
                .markup(" was recorded. What happened: ")
                .text(crate::operator::display::list(&commit.outcomes))
                .markup(".</p><p><a href=\"/parked\">Back to Needs review</a></p>"),
        ),
        Err(error) => failure(&error),
    }
}

pub(super) async fn resolve(
    State(state): State<Arc<Context>>,
    Path(parked): Path<i64>,
    Extension(session): Extension<Arc<Session>>,
    Form(form): Form<Confirmation>,
) -> Response {
    if !valid(&session, &form) {
        return sign_in_first();
    }
    let result = async {
        crate::operator::recovery::resolve(
            &state.store,
            parked,
            session.actor.clone(),
            Reason::new(&form.reason)?,
        )
        .await
        .map_err(Error::from)
    }
    .await;
    match result {
        Ok(()) => page(
            Tab::NeedsReview,
            "Marked resolved",
            &Markup::new()
                .markup("<p class=\"notice info\" title=\"Resolution recorded\">Item ")
                .text(parked)
                .markup(concat!(
                    " is marked resolved with your reason. It stays in the history.</p>",
                    "<p><a href=\"/parked\">Back to Needs review</a></p>",
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
        return sign_in_first();
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
            Tab::Deliveries,
            "Queued to send again",
            &Markup::new().markup(concat!(
                "<p class=\"notice info\" title=\"Resend queued\">The delivery is queued and ",
                "goes to the lab app unchanged, with the same delivery number.</p>",
                "<p><a href=\"/deliveries\">Back to Deliveries</a></p>",
            )),
        ),
        Err(error) => failure(&error),
    }
}
