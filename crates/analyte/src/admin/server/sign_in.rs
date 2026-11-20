//! Signs a browser in with the admin key and out again: the sign-in page, the challenge every unsigned request gets, and the session cookie.
use super::{Context, Session, Token};
use crate::admin::view::{frame::Plain, refusal::Refusal};
use askama::Template as _;
use axum::{
    Extension, Form,
    extract::State,
    http::{StatusCode, header},
    response::{Html, IntoResponse, Response},
};
use std::sync::Arc;

/// Sign in (the `SignIn` mockup): the admin key's field and the session's form
/// token. It never holds the key that was typed.
#[derive(askama::Template)]
#[template(path = "pages/server/sign_in.html")]
struct Page {
    frame: Plain,
    csrf: String,
    /// Why the last try was refused, under the key's field.
    error: Option<&'static str>,
    /// A form was sent without a signed-in session and was not acted on.
    refused: bool,
}

/// What brought someone to the sign-in page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Attempt {
    /// Opening it, or any page, without a signed-in session.
    Open,
    /// Sending a form without a signed-in session; it was not acted on.
    Form,
    /// A key that is not the admin key.
    WrongKey,
    /// A sign-in form that could not be read, or whose form token did not match.
    Unreadable,
}

impl Attempt {
    /// What the page says under the key's field.
    const fn error(self) -> Option<&'static str> {
        match self {
            Self::Open | Self::Form => None,
            Self::WrongKey => Some("This is not the admin key. Check it and try again."),
            Self::Unreadable => Some("Analyte could not read the form. Enter the key again."),
        }
    }
}

/// The sign-in page with status 401, the answer to every request that is not
/// signed in. It keeps `existing`'s session, so its form token stays valid
/// for another try, or starts a session that is not signed in yet.
pub(super) fn unauthorized(
    state: &Context,
    existing: Option<&Arc<Session>>,
    attempt: Attempt,
) -> Response {
    tracing::warn!("admin authentication refused");
    let session = existing
        .cloned()
        .map(Ok)
        .unwrap_or_else(|| state.sessions.issue(false, None));
    let mut response = match session {
        Ok(session) => {
            let page = Page {
                frame: Plain::new(&state.assets, "Sign in"),
                csrf: session.csrf.form_value().to_owned(),
                error: attempt.error(),
                refused: attempt == Attempt::Form,
            };
            let mut response = match page.render() {
                Ok(html) => (StatusCode::UNAUTHORIZED, Html(html)).into_response(),
                Err(render) => Refusal::PageFailed {
                    status: StatusCode::UNAUTHORIZED,
                    title: "Sign in",
                    text: "The sign-in page could not be shown. Give the details below to support.",
                    detail: format!("sign-in page failed: {render}"),
                }
                .page(&state.assets),
            };
            if let Ok(value) = header::HeaderValue::from_str(&session.id.cookie(false)) {
                response.headers_mut().insert(header::SET_COOKIE, value);
            }
            response
        }
        Err(error) => {
            let mut response = Refusal::SessionsFull(error.to_string()).page(&state.assets);
            // The challenge stays a 401, as every unsigned request's answer is.
            *response.status_mut() = StatusCode::UNAUTHORIZED;
            response
        }
    };
    response.headers_mut().insert(
        header::WWW_AUTHENTICATE,
        header::HeaderValue::from_static("Bearer"),
    );
    response
}

pub(super) async fn login_page(
    State(state): State<Arc<Context>>,
    session: Option<Extension<Arc<Session>>>,
) -> Response {
    unauthorized(
        &state,
        session.as_ref().map(|Extension(s)| s),
        Attempt::Open,
    )
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Login {
    csrf: String,
    token: String,
}

pub(super) async fn login(
    State(state): State<Arc<Context>>,
    Extension(previous): Extension<Arc<Session>>,
    form: Result<Form<Login>, axum::extract::rejection::FormRejection>,
) -> Response {
    let Ok(Form(form)) = form else {
        return unauthorized(&state, Some(&previous), Attempt::Unreadable);
    };
    if !previous.csrf_valid(&form.csrf) {
        return unauthorized(&state, Some(&previous), Attempt::Unreadable);
    }
    if !state.token.matches(&form.token) {
        return unauthorized(&state, Some(&previous), Attempt::WrongKey);
    }
    // This listener is HTTP loopback only. Forwarded scheme headers are never
    // trusted. HTTPS transport, when supported, must also enable Secure here.
    let session = match state.sessions.issue(true, Some(&previous)) {
        Ok(session) => session,
        Err(error) => return Refusal::SessionsFull(error.to_string()).page(&state.assets),
    };
    let cookie = session.id.cookie(false);
    let Ok(cookie) = header::HeaderValue::from_str(&cookie) else {
        return Refusal::SessionHeader.page(&state.assets);
    };
    let mut response = StatusCode::SEE_OTHER.into_response();
    response.headers_mut().insert(header::SET_COOKIE, cookie);
    response
        .headers_mut()
        .insert(header::LOCATION, header::HeaderValue::from_static("/"));
    response
}

/// POST /logout: the dispatcher already checked the session's CSRF token. Ends
/// the session, clears its cookie and returns to the sign-in page. Sign-in is
/// not audited, so sign-out is not either.
pub(super) async fn logout(
    State(state): State<Arc<Context>>,
    Extension(session): Extension<Arc<Session>>,
) -> Response {
    if let Err(error) = state.sessions.end(&session) {
        return Refusal::SignOut(error.to_string()).page(&state.assets);
    }
    let mut response = StatusCode::SEE_OTHER.into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        header::HeaderValue::from_static(Token::CLEARED_COOKIE),
    );
    response
        .headers_mut()
        .insert(header::LOCATION, header::HeaderValue::from_static("/login"));
    response
}
