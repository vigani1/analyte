//! Holds every refusal that has no page of its own, in the plain frame because it may be shown before sign-in: its status code, what it tells people, where to go next and the exact reason kept for support.
use super::{assets::Root, frame::Plain};
use askama::Template as _;
use axum::{
    http::{Method, StatusCode},
    response::{Html, IntoResponse, Response},
};

/// Why Analyte answered with a refusal rather than a page.
pub(in crate::admin) enum Refusal {
    /// No page is served at this address.
    PageNotFound,
    /// No file is served at this address under `/static/`.
    FileNotFound,
    /// The address takes other requests, such as Sign out, which only takes its form.
    WrongMethod(Method),
    /// The address, its query or the form could not be read; the status and
    /// the words are the web framework's own.
    Unreadable { status: StatusCode, detail: String },
    /// A form came without this session's form token; the detail says which check failed.
    FormExpired(&'static str),
    /// A form held more text than its limit.
    TooMuchText,
    /// The request ran past its budget; what it started may still finish.
    TooLong,
    /// Browser sessions can no longer be checked; only a restart helps.
    NeedsRestart,
    /// No browser session could be started; the detail says why.
    SessionsFull(String),
    /// The new session's cookie could not be written.
    SessionHeader,
    /// Sign out could not end the session; the detail says why.
    SignOut(String),
    /// A page could not be written: its own status and title, what it tells
    /// people and both reasons.
    PageFailed {
        status: StatusCode,
        title: &'static str,
        text: &'static str,
        detail: String,
    },
}

/// Where a refusal sends people next.
#[derive(Clone, Copy)]
pub(in crate::admin) enum Next {
    /// The Overview, which asks to sign in first when needed.
    Overview,
    /// The sign-in page, to try again.
    SignIn,
}

impl Next {
    pub(in crate::admin) const fn href(self) -> &'static str {
        match self {
            Self::Overview => "/",
            Self::SignIn => "/login",
        }
    }

    pub(in crate::admin) const fn label(self) -> &'static str {
        match self {
            Self::Overview => "Go to Overview",
            Self::SignIn => "Try again",
        }
    }
}

/// The refusal page: what happened, what to do, the link onward and the
/// exact reason for support.
#[derive(askama::Template)]
#[template(path = "pages/refusal.html")]
struct Page {
    frame: Plain,
    title: &'static str,
    text: &'static str,
    next: Option<Next>,
    support: Vec<String>,
}

impl Refusal {
    /// Its status code: what went wrong in HTTP's terms.
    const fn status(&self) -> StatusCode {
        match self {
            Self::PageNotFound | Self::FileNotFound => StatusCode::NOT_FOUND,
            Self::WrongMethod(_) => StatusCode::METHOD_NOT_ALLOWED,
            Self::Unreadable { status, .. } | Self::PageFailed { status, .. } => *status,
            Self::FormExpired(_) => StatusCode::FORBIDDEN,
            Self::TooMuchText => StatusCode::PAYLOAD_TOO_LARGE,
            Self::TooLong => StatusCode::GATEWAY_TIMEOUT,
            Self::NeedsRestart | Self::SessionsFull(_) | Self::SignOut(_) => {
                StatusCode::SERVICE_UNAVAILABLE
            }
            Self::SessionHeader => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// The page's title and what it tells people.
    const fn words(&self) -> (&'static str, &'static str) {
        match self {
            Self::PageNotFound => (
                "Page not found",
                "Analyte has no page at this address. Check the address, or go to the Overview.",
            ),
            Self::FileNotFound => ("File not found", "Analyte has no file at this address."),
            Self::WrongMethod(_) => (
                "This cannot be done here",
                "Analyte does not take this kind of request at this address. Nothing was changed.",
            ),
            Self::Unreadable { .. } => (
                "This could not be read",
                "Analyte could not read the address or the form it was sent. Nothing was changed. Go back and try again.",
            ),
            Self::FormExpired(_) => (
                "This form has expired",
                "Go back, reload the page and try again. Nothing was changed.",
            ),
            Self::TooMuchText => (
                "Too much text",
                "The form holds more text than Analyte accepts. Shorten what you typed and try again.",
            ),
            Self::TooLong => (
                "This took too long",
                "Analyte did not finish in time. Check the page before trying again: an action may already have been saved.",
            ),
            Self::NeedsRestart => (
                "Analyte needs a restart",
                "Analyte cannot check browser sessions any more. Restart Analyte, then sign in again.",
            ),
            Self::SessionsFull(_) => (
                "You cannot sign in right now",
                "Too many browser sessions are open. Wait a few minutes and try again, or restart Analyte.",
            ),
            Self::SessionHeader => (
                "You cannot sign in right now",
                "Analyte could not start a browser session. Try again; if this keeps happening, give the details below to support.",
            ),
            Self::SignOut(_) => (
                "You cannot sign out right now",
                "Analyte cannot check browser sessions any more. Restart Analyte; that signs everyone out.",
            ),
            Self::PageFailed { title, text, .. } => (title, text),
        }
    }

    /// Where to go next; none when only a restart helps.
    const fn next(&self) -> Option<Next> {
        match self {
            Self::PageNotFound
            | Self::FileNotFound
            | Self::WrongMethod(_)
            | Self::Unreadable { .. }
            | Self::FormExpired(_)
            | Self::TooMuchText
            | Self::TooLong
            | Self::PageFailed { .. } => Some(Next::Overview),
            Self::SessionsFull(_) | Self::SessionHeader => Some(Next::SignIn),
            Self::NeedsRestart | Self::SignOut(_) => None,
        }
    }

    /// The exact reason, for support.
    fn detail(&self) -> String {
        match self {
            Self::PageNotFound => "no admin page at this address".into(),
            Self::FileNotFound => "static asset not found".into(),
            Self::WrongMethod(method) => format!("{method} is not allowed at this address"),
            Self::Unreadable { detail, .. }
            | Self::SessionsFull(detail)
            | Self::SignOut(detail)
            | Self::PageFailed { detail, .. } => detail.clone(),
            Self::FormExpired(detail) => (*detail).into(),
            Self::TooMuchText => "admin form exceeds limit".into(),
            Self::TooLong => "admin request deadline exceeded".into(),
            Self::NeedsRestart => "admin session lookup failed".into(),
            Self::SessionHeader => "admin session header invalid".into(),
        }
    }

    /// The refusal page with its status code, loading this build's files from
    /// `assets`. Should the page itself fail to render, the answer is the same
    /// words as plain text with both reasons.
    pub(in crate::admin) fn page(&self, assets: &Root) -> Response {
        let (title, text) = self.words();
        let page = Page {
            frame: Plain::new(assets, title),
            title,
            text,
            next: self.next(),
            support: vec![self.detail()],
        };
        match page.render() {
            Ok(html) => (self.status(), Html(html)).into_response(),
            Err(render) => (
                self.status(),
                format!(
                    "{title}\n\n{text}\n\nDetails for support: {}; the page failed too: {render}",
                    self.detail()
                ),
            )
                .into_response(),
        }
    }
}
