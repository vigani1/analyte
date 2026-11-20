//! Runs the audited Send again action (GET and POST /deliveries/{id}/resend) in the old look until the Deliveries page moves (admin restyle b5).
use super::{
    server::Context,
    sessions::Session,
    view::{self, Tab, exact, format},
};
use crate::admin::Error;
use crate::admin::error::CommandError;
use crate::admin::markup::Markup;
use crate::deadline;
use analyte_model::{DeliveryId, Identifier, Reason};
use axum::{
    Extension, Form,
    extract::{Path, State},
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

pub(super) fn page(tab: Tab, title: &str, body: &Markup) -> Response {
    Html(view::page(tab, title).fragment(body).markup(view::END)).into_response()
}
pub(super) fn failure(error: &dyn std::error::Error) -> Response {
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
fn form(session: &Session, route: &str, label: &str) -> Markup {
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
        .markup(concat!(
            "<label class=\"check\" title=\"Acknowledge superseded revision\"><input ",
            "type=\"checkbox\" name=\"acknowledge_superseded\" value=\"true\">",
            "Send it even if a newer correction of this result exists.</label>",
        ))
        .markup("<div class=\"actions-row\"><button>")
        .text(label)
        .markup("</button></div></form>")
}

/// Shows an audited action's confirmation page, only to a signed-in browser
/// session: `read` fetches what the page needs from the store within the request
/// deadline, then `show` builds the page around [`form`], which carries the
/// session's CSRF token and asks for the reason.
async fn confirmation<T: Send + 'static>(
    state: &Context,
    session: Option<Extension<Arc<Session>>>,
    read: impl FnOnce(&mut analyte_store::Store) -> Result<T, CommandError> + Send + 'static,
    show: impl FnOnce(T, &Session) -> Response,
) -> Response {
    let Some(Extension(session)) = session.filter(|Extension(s)| s.authenticated) else {
        return sign_in_first();
    };
    let until = match deadline() {
        Ok(until) => until,
        Err(error) => return failure(&error),
    };
    match state.store.request::<_, CommandError>(until, read).await {
        Ok(value) => show(value, &session),
        Err(error) => failure(&error),
    }
}

/// Runs an audited action posted from its confirmation page. A form without this
/// session's CSRF token and "confirm=yes" is refused before anything runs; the
/// typed reason and the session's actor are what `act` records in the audit.
async fn confirmed<T, F: Future<Output = Result<T, Error>>>(
    session: &Session,
    form: &Confirmation,
    act: impl FnOnce(Reason, Identifier) -> F,
    done: impl FnOnce(T) -> Response,
) -> Response {
    if !(session.authenticated && session.csrf_valid(&form.csrf) && form.confirm == "yes") {
        return sign_in_first();
    }
    let result = match Reason::new(&form.reason) {
        Ok(reason) => act(reason, session.actor.clone()).await,
        Err(error) => Err(error.into()),
    };
    match result {
        Ok(value) => done(value),
        Err(error) => failure(&error),
    }
}

pub(super) async fn confirm_resend(
    State(state): State<Arc<Context>>,
    Path(text): Path<String>,
    session: Option<Extension<Arc<Session>>>,
) -> Response {
    let delivery = match id(&text) {
        Ok(id) => id,
        Err(error) => return failure(&error),
    };
    let read = move |s: &mut analyte_store::Store| {
        s.delivery(delivery)?.ok_or(CommandError::from(
            analyte_store::DeliveryError::UnknownDelivery,
        ))
    };
    confirmation(&state, session, read, |row, session| {
        page(
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
                    format::delivery_state(row.state),
                    &row.state.to_string(),
                ))
                .markup(concat!(
                    ". If the lab app already has it, it treats the repeat as done. Analyte ",
                    "checks again that it may be sent when you confirm.</p>",
                ))
                .fragment(&form(
                    session,
                    &format!("/deliveries/{}/resend", uuid::Uuid::from(delivery)),
                    "Send again",
                ))
                .markup("</section>"),
        )
    })
    .await
}

pub(super) async fn resend(
    State(state): State<Arc<Context>>,
    Path(text): Path<String>,
    Extension(session): Extension<Arc<Session>>,
    Form(form): Form<Confirmation>,
) -> Response {
    let superseded = form.acknowledge_superseded;
    let act = |reason, actor| async move {
        crate::operator::recovery::resend(&state.store, id(&text)?, superseded, actor, reason)
            .await
            .map_err(Error::from)
    };
    confirmed(&session, &form, act, |()| {
        page(
            Tab::Deliveries,
            "Queued to send again",
            &Markup::new().markup(concat!(
                "<p class=\"notice info\" title=\"Resend queued\">The delivery is queued and ",
                "goes to the lab app unchanged, with the same delivery number.</p>",
                "<p><a href=\"/deliveries\">Back to Deliveries</a></p>",
            )),
        )
    })
    .await
}
