//! Signs a browser in with the admin key or a Windows one-time code, and out again: the sign-in page, the challenge every unsigned request gets, and the session cookie.
use super::{
    Context, Session, Token,
    sessions::SignedIn,
    windows::line::{Next, Refusal as Declined},
};
use crate::admin::view::{Tone, format, frame::Plain, refusal::Refusal};
use askama::Template as _;
use axum::{
    Extension, Form,
    extract::{Query, Request, State, rejection::QueryRejection},
    http::{Method, StatusCode, header},
    response::{Html, IntoResponse, Response},
};
use std::sync::Arc;

/// How the Analyte app's pages view ends its user agent (ADR 0021 §6). It is
/// only a hint: a browser that fakes it lands on a page that says Windows
/// sign-in works in the app, next to the key form.
const APP_MARK: &str = "AnalyteApp";
/// Windows sign-in's address: the app takes it before it loads, and a
/// browser gets the key form with a note.
const WINDOWS_PATH: &str = "/login/windows";

/// Sign in (the `SignIn` mockup): the admin key's field and the session's form
/// token. It never holds the key that was typed.
#[derive(askama::Template)]
#[template(path = "pages/server/sign_in.html")]
struct Page {
    frame: Plain,
    csrf: String,
    /// Why the last try was refused, under the key's field.
    error: Option<&'static str>,
    /// What to know before signing in, above the key's field.
    notice: Option<Notice>,
    /// Windows sign-in works on this system, so the page links to it.
    windows: bool,
    /// The app's own Windows sign-in address, such as
    /// `/login/windows?next=%2Fparked`; only on a challenge to the app.
    refresh: Option<String>,
}

/// A boxed sentence above the key's field; every word is fixed here, never
/// taken from the address.
pub(super) enum Notice {
    /// Something did not work, in a tone.
    Alert {
        tone: Tone,
        title: &'static str,
        text: &'static str,
    },
    /// Something worth knowing.
    Note {
        title: &'static str,
        text: &'static str,
    },
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
    /// The app's Windows sign-in was refused for this reason (ADR 0021 §6).
    Windows(Declined),
    /// A Windows sign-in code that is wrong, used or older than 20 s.
    CodeSpent,
    /// `GET /login/windows` without a code: a browser, not the app.
    Browser,
}

impl Attempt {
    /// What the page says under the key's field.
    const fn error(self) -> Option<&'static str> {
        match self {
            Self::Open | Self::Form | Self::Windows(_) | Self::CodeSpent | Self::Browser => None,
            Self::WrongKey => Some("This is not the admin key. Check it and try again."),
            Self::Unreadable => Some("Analyte could not read the form. Enter the key again."),
        }
    }

    /// What the page says above the key's field.
    const fn notice(self) -> Option<Notice> {
        const fn attention(title: &'static str, text: &'static str) -> Option<Notice> {
            Some(Notice::Alert {
                tone: Tone::Attention,
                title,
                text,
            })
        }
        let refused = "Windows sign-in did not work";
        match self {
            Self::Open | Self::WrongKey | Self::Unreadable => None,
            Self::Form => attention(
                format::NOTHING_CHANGED,
                "The form was sent without a signed-in session. Sign in, then open the page again and repeat what you did.",
            ),
            Self::Windows(Declined::NotAllowed) => attention(
                refused,
                "Your Windows account is not allowed to use Analyte. Ask an administrator to add it to the Analyte users group, or sign in with the admin key.",
            ),
            Self::Windows(Declined::BadRequest) => attention(
                refused,
                "The Analyte app asked in a way this engine does not understand. Sign in with the admin key, and ask IT to update the app.",
            ),
            Self::Windows(Declined::Unidentified) => attention(
                refused,
                "Windows could not say which account is asking. Sign in with the admin key.",
            ),
            Self::Windows(Declined::Unavailable) => attention(
                refused,
                "Windows sign-in is not available right now. Try again in a moment, or sign in with the admin key.",
            ),
            Self::CodeSpent => attention(
                "This sign-in has expired",
                "A Windows sign-in works once, within 20 seconds. Open the Analyte app again, or sign in with the admin key.",
            ),
            Self::Browser => Some(Notice::Note {
                title: "Windows sign-in works in the Analyte app",
                text: "In a browser, sign in with the admin key.",
            }),
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
    sign_in_page(state, existing, attempt, None)
}

/// The challenge to a request that is not signed in: [`unauthorized`], and
/// for a page the Analyte app opens, a refresh to its Windows sign-in that
/// returns to the page (ADR 0021 §6). Only `GET` asks the app to sign in by
/// itself, so a refused form still says nothing was changed; sign-in pages
/// never do, so signing out stays signed out.
pub(super) fn challenge(
    state: &Context,
    existing: Option<&Arc<Session>>,
    request: &Request,
) -> Response {
    let get = request.method() == Method::GET;
    let attempt = if request.method() == Method::POST {
        Attempt::Form
    } else {
        Attempt::Open
    };
    let app = request
        .headers()
        .get(header::USER_AGENT)
        .and_then(|agent| agent.to_str().ok())
        .is_some_and(|agent| agent.ends_with(APP_MARK));
    let page = request
        .uri()
        .path_and_query()
        .map_or("/", |page| page.as_str());
    let refresh = (get && app && !page.starts_with("/login")).then(|| {
        format!(
            "{WINDOWS_PATH}?next={}",
            format::path_segment(Next::new(page).as_str())
        )
    });
    sign_in_page(state, existing, attempt, refresh)
}

fn sign_in_page(
    state: &Context,
    existing: Option<&Arc<Session>>,
    attempt: Attempt,
    refresh: Option<String>,
) -> Response {
    tracing::warn!("admin authentication refused");
    let session = existing
        .cloned()
        .map(Ok)
        .unwrap_or_else(|| state.sessions.issue(SignedIn::Pending, None));
    let mut response = match session {
        Ok(session) => {
            let page = Page {
                frame: Plain::new(&state.assets, SIGN_IN),
                csrf: session.csrf.form_value().to_owned(),
                error: attempt.error(),
                notice: attempt.notice(),
                windows: state.windows.status().is_some() && attempt != Attempt::Browser,
                refresh,
            };
            let mut response = match page.render() {
                Ok(html) => (StatusCode::UNAUTHORIZED, Html(html)).into_response(),
                Err(render) => Refusal::PageFailed {
                    status: StatusCode::UNAUTHORIZED,
                    title: SIGN_IN,
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

/// What `GET /login` may carry: the word the app sends after a refused
/// Windows sign-in, shown only as its fixed sentence.
#[derive(serde::Deserialize)]
pub(super) struct LoginQuery {
    windows: Option<String>,
}

/// `GET /login`: the key form, never refreshed, so sign-out stays signed out;
/// with `?windows=<word>` one of [`Declined`]'s fixed sentences above it,
/// and nothing for any other text.
pub(super) async fn login_page(
    State(state): State<Arc<Context>>,
    session: Option<Extension<Arc<Session>>>,
    query: Result<Query<LoginQuery>, QueryRejection>,
) -> Response {
    let attempt = query
        .ok()
        .and_then(|Query(query)| query.windows)
        .and_then(|word| Declined::from_word(&word))
        .map_or(Attempt::Open, Attempt::Windows);
    unauthorized(&state, session.as_ref().map(|Extension(s)| s), attempt)
}

/// What `GET /login/windows` may carry: the one-time code. Its `next` is for
/// the app to read; the code carries its own checked page.
#[derive(serde::Deserialize)]
pub(super) struct WindowsQuery {
    code: Option<String>,
}

/// `GET /login/windows`: a fresh code starts a session for its Windows
/// account and goes to its page; a wrong, used or expired one gets the key
/// form saying so. Without a code it is a browser, since the app takes this
/// address before it loads, so the key form says where Windows sign-in works.
pub(super) async fn windows(
    State(state): State<Arc<Context>>,
    session: Option<Extension<Arc<Session>>>,
    query: Result<Query<WindowsQuery>, QueryRejection>,
) -> Response {
    let previous = session.as_ref().map(|Extension(s)| s);
    let Some(code) = query.ok().and_then(|Query(query)| query.code) else {
        return unauthorized(&state, previous, Attempt::Browser);
    };
    match state.windows.codes.redeem(&code) {
        Ok(Some((account, next))) => signed_in(
            &state,
            SignedIn::Windows(account),
            previous.map(AsRef::as_ref),
            next.as_str(),
        ),
        Ok(None) => unauthorized(&state, previous, Attempt::CodeSpent),
        Err(error) => {
            tracing::error!(error = %error, "windows sign-in codes cannot be read");
            Refusal::NeedsRestart.page(&state.assets)
        }
    }
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
    match SignedIn::admin_key() {
        Ok(signed) => signed_in(&state, signed, Some(&previous), "/"),
        Err(error) => Refusal::SessionsFull(error.to_string()).page(&state.assets),
    }
}

/// Starts a session signed in as `signed` in place of `previous`, sets its
/// cookie and goes to `page`, a checked local path.
fn signed_in(
    state: &Context,
    signed: SignedIn,
    previous: Option<&Session>,
    page: &str,
) -> Response {
    // This listener is HTTP loopback only. Forwarded scheme headers are never
    // trusted. HTTPS transport, when supported, must also enable Secure here.
    let session = match state.sessions.issue(signed, previous) {
        Ok(session) => session,
        Err(error) => return Refusal::SessionsFull(error.to_string()).page(&state.assets),
    };
    let (Ok(cookie), Ok(location)) = (
        header::HeaderValue::from_str(&session.id.cookie(false)),
        header::HeaderValue::from_str(page),
    ) else {
        return Refusal::SessionHeader.page(&state.assets);
    };
    let mut response = StatusCode::SEE_OTHER.into_response();
    response.headers_mut().insert(header::SET_COOKIE, cookie);
    response.headers_mut().insert(header::LOCATION, location);
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

/// Labels the sign-in page and its action.
pub(in crate::admin) const SIGN_IN: &str = "Sign in";
