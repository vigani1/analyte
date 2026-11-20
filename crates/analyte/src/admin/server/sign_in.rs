//! Signs a browser in with the admin key and out again: the sign-in page, the challenge every unsigned request gets, and the session cookie.
use super::{Context, Session, Token};
use axum::{
    Extension, Form,
    extract::State,
    http::{StatusCode, header},
    response::{Html, IntoResponse, Response},
};
use std::sync::Arc;

pub(super) fn unauthorized(state: &Context, existing: Option<&Arc<Session>>) -> Response {
    tracing::warn!("admin authentication refused");
    let session = existing
        .cloned()
        .map(Ok)
        .unwrap_or_else(|| state.sessions.issue(false, None));
    let mut response = match session {
        Ok(session) => {
            let html = crate::admin::view::login("Sign in to Analyte")
                .markup(concat!(
                    "<section><p class=\"lead\">Enter the admin key. It is in the file ",
                    "<code>admin-token</code> in Analyte's data folder on this PC; ask IT if you ",
                    "do not have it.</p><form ",
                    "action=\"/login\" method=\"post\"><input name=\"csrf\" ",
                ))
                .markup("type=\"hidden\" value=\"")
                .text(session.csrf.form_value())
                .markup(concat!(
                    "\"><label class=\"field\"><span title=\"Admin token\">Admin key</span><input name=\"token\" ",
                    "type=\"password\" required autocomplete=\"off\" ",
                ))
                .markup("maxlength=\"44\"></label><button>Sign in</button></form></section>")
                .markup(crate::admin::view::END);
            let mut response = (StatusCode::UNAUTHORIZED, Html(html)).into_response();
            if let Ok(value) = header::HeaderValue::from_str(&session.id.cookie(false)) {
                response.headers_mut().insert(header::SET_COOKIE, value);
            }
            response
        }
        Err(error) => crate::admin::view::refusal(
            StatusCode::UNAUTHORIZED,
            "You cannot sign in right now",
            "Too many browser sessions are open. Wait a few minutes and try again, or restart Analyte.",
            error,
        ),
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
    unauthorized(&state, session.as_ref().map(|Extension(s)| s))
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
        return unauthorized(&state, Some(&previous));
    };
    if !previous.csrf_valid(&form.csrf) || !state.token.matches(&form.token) {
        return unauthorized(&state, Some(&previous));
    }
    // This listener is HTTP loopback only. Forwarded scheme headers are never
    // trusted. HTTPS transport, when supported, must also enable Secure here.
    let session = match state.sessions.issue(true, Some(&previous)) {
        Ok(session) => session,
        Err(error) => {
            return crate::admin::view::refusal(
                StatusCode::SERVICE_UNAVAILABLE,
                "You cannot sign in right now",
                "Too many browser sessions are open. Wait a few minutes and try again, or restart Analyte.",
                error,
            );
        }
    };
    let cookie = session.id.cookie(false);
    let Ok(cookie) = header::HeaderValue::from_str(&cookie) else {
        return crate::admin::view::refusal(
            StatusCode::INTERNAL_SERVER_ERROR,
            "You cannot sign in right now",
            "Analyte could not start a browser session. Try again; if this keeps happening, give the details below to support.",
            "admin session header invalid",
        );
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
        return crate::admin::view::refusal(
            StatusCode::SERVICE_UNAVAILABLE,
            "You cannot sign out right now",
            "Analyte cannot check browser sessions any more. Restart Analyte; that signs everyone out.",
            error,
        );
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
