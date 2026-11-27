//! Holds every refusal that has no page of its own, in the plain frame because it may be shown before sign-in: its status code, what it tells people, where to go next and the exact reason kept for support.
use super::{assets::Root, frame::Plain};
use analyte_i18n::{message as tr_args, text as tr};
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
    /// Every live stream is in use; pages still answer without one.
    TooManyStreams,
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

    pub(in crate::admin) fn label(self) -> &'static str {
        match self {
            Self::Overview => tr("admin-go-to-overview"),
            Self::SignIn => tr("admin-try-again"),
        }
    }
}

/// The refusal page: what happened, what to do, the link onward and the
/// exact reason for support.
#[derive(askama::Template)]
#[template(path = "pages/view/refusal.html")]
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
            Self::NeedsRestart
            | Self::SessionsFull(_)
            | Self::SignOut(_)
            | Self::TooManyStreams => StatusCode::SERVICE_UNAVAILABLE,
            Self::SessionHeader => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// The page's title and what it tells people.
    fn words(&self) -> (&'static str, &'static str) {
        match self {
            Self::PageNotFound => (
                tr("admin-page-not-found"),
                tr("admin-analyte-has-no-page-at-this-address-check-the"),
            ),
            Self::FileNotFound => (
                tr("admin-file-not-found"),
                tr("admin-analyte-has-no-file-at-this-address"),
            ),
            Self::WrongMethod(_) => (
                tr("admin-this-cannot-be-done-here"),
                tr("admin-analyte-does-not-take-this-kind-of-request-at"),
            ),
            Self::Unreadable { .. } => (
                tr("admin-this-could-not-be-read"),
                tr("admin-analyte-could-not-read-the-address-or-the-form"),
            ),
            Self::FormExpired(_) => (
                tr("admin-this-form-has-expired"),
                tr("admin-go-back-reload-the-page-and-try-again-nothing"),
            ),
            Self::TooMuchText => (
                tr("admin-too-much-text"),
                tr("admin-the-form-holds-more-text-than-analyte-accepts-shorten"),
            ),
            Self::TooLong => (
                tr("admin-this-took-too-long"),
                tr("admin-analyte-did-not-finish-in-time-check-the-page"),
            ),
            Self::NeedsRestart => (
                tr("admin-analyte-needs-a-restart"),
                tr("admin-analyte-cannot-check-browser-sessions-any-more-restart-analyte"),
            ),
            Self::SessionsFull(_) => (
                tr("admin-you-cannot-sign-in-right-now"),
                tr("admin-too-many-browser-sessions-are-open-wait-a-few"),
            ),
            Self::SessionHeader => (
                tr("admin-you-cannot-sign-in-right-now"),
                tr("admin-analyte-could-not-start-a-browser-session-try-again"),
            ),
            Self::SignOut(_) => (
                tr("admin-you-cannot-sign-out-right-now"),
                tr("admin-analyte-cannot-check-browser-sessions-any-more-restart-analyte-2"),
            ),
            Self::TooManyStreams => (
                tr("admin-too-many-live-pages"),
                tr("admin-analyte-keeps-a-few-pages-live-at-once-and"),
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
            | Self::TooManyStreams
            | Self::PageFailed { .. } => Some(Next::Overview),
            Self::SessionsFull(_) | Self::SessionHeader => Some(Next::SignIn),
            Self::NeedsRestart | Self::SignOut(_) => None,
        }
    }

    /// The exact reason, for support.
    fn detail(&self) -> String {
        match self {
            Self::PageNotFound => tr("admin-no-admin-page-at-this-address").into(),
            Self::FileNotFound => tr("admin-static-asset-not-found").into(),
            Self::WrongMethod(method) => tr_args(
                "admin-is-not-allowed-at-this-address",
                [("method", (method).to_string().into())],
            ),
            Self::Unreadable { detail, .. }
            | Self::SessionsFull(detail)
            | Self::SignOut(detail)
            | Self::PageFailed { detail, .. } => detail.clone(),
            Self::FormExpired(detail) => (*detail).into(),
            Self::TooMuchText => tr("admin-admin-form-exceeds-limit").into(),
            Self::TooLong => tr("admin-admin-request-deadline-exceeded").into(),
            Self::NeedsRestart => tr("admin-admin-session-lookup-failed").into(),
            Self::SessionHeader => tr("admin-admin-session-header-invalid").into(),
            Self::TooManyStreams => tr("admin-every-admin-live-stream-is-in-use").into(),
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
                tr_args(
                    "admin-details-for-support-the-page-failed-too",
                    [
                        ("title", (title).to_string().into()),
                        ("text", (text).to_string().into()),
                        ("detail", (self.detail()).to_string().into()),
                        ("render", (render).to_string().into()),
                    ],
                ),
            )
                .into_response(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A refusal and what its page must show: status, title, a sentence of
    /// what to do, the link onward (none when only a restart helps) and the
    /// exact reason.
    type Case = (
        Refusal,
        u16,
        &'static str,
        &'static str,
        Option<&'static str>,
        &'static str,
    );

    const OVERVIEW: Option<&str> = Some("href=\"/\">Go to Overview</a>");
    const AGAIN: Option<&str> = Some("href=\"/login\">Try again</a>");

    /// Renders each case and checks its page in the plain frame.
    async fn check(cases: Vec<Case>) -> Result<(), axum::Error> {
        for (refusal, status, title, text, link, detail) in cases {
            let response = refusal.page(&Root::of_this_build());
            assert_eq!(response.status().as_u16(), status, "{title}");
            let body = axum::body::to_bytes(response.into_body(), 1 << 16).await?;
            let page = String::from_utf8_lossy(&body);
            assert!(page.contains("<main class=\"center\">"), "{title}");
            assert!(!page.contains("class=\"sidebar\""), "{title}");
            assert!(page.contains(&format!("<title>{title} · Analyte</title>")));
            assert!(page.contains(&format!("class=\"panel__title\">{title}</h1>")));
            assert!(page.contains(text), "{title}: {text}");
            match link {
                Some(link) => assert!(page.contains(link), "{title}: {link}"),
                None => assert!(!page.contains("panel__action"), "{title}"),
            }
            assert!(page.contains(detail), "{title}: {detail}");
            assert!(!page.contains("<b>"), "{title}");
        }
        Ok(())
    }

    // Owner request 2026-11-20 (admin restyle b6): the refusals about sessions
    // and time are the plain frame with their status, words, link and reason.
    // A broken session table cannot be made through the router, so these
    // states are rendered here.
    #[tokio::test]
    async fn session_and_time_refusals_say_what_happened() -> Result<(), axum::Error> {
        check(vec![
            (
                Refusal::TooLong,
                504,
                "This took too long",
                "an action may already have been saved.",
                OVERVIEW,
                "admin request deadline exceeded",
            ),
            (
                Refusal::NeedsRestart,
                503,
                "Analyte needs a restart",
                "Restart Analyte, then sign in again.",
                None,
                "admin session lookup failed",
            ),
            (
                Refusal::SignOut("admin session lock poisoned".into()),
                503,
                "You cannot sign out right now",
                "Restart Analyte; that signs everyone out.",
                None,
                "admin session lock poisoned",
            ),
            (
                Refusal::SessionHeader,
                500,
                "You cannot sign in right now",
                "Analyte could not start a browser session.",
                AGAIN,
                "admin session header invalid",
            ),
            (
                Refusal::SessionsFull("full".into()),
                503,
                "You cannot sign in right now",
                "Too many browser sessions are open.",
                AGAIN,
                "full",
            ),
            (
                Refusal::FormExpired("CSRF session required"),
                403,
                "This form has expired",
                "Nothing was changed.",
                OVERVIEW,
                "CSRF session required",
            ),
        ])
        .await
    }

    // Owner request 2026-11-20 (admin restyle b6): the refusals about an
    // address, a form or a page that failed are the plain frame with their
    // status, words, link and reason; a reason from outside stays escaped.
    #[tokio::test]
    async fn address_and_page_refusals_say_what_happened() -> Result<(), axum::Error> {
        check(vec![
            (
                Refusal::TooMuchText,
                413,
                "Too much text",
                "Shorten what you typed and try again.",
                OVERVIEW,
                "admin form exceeds limit",
            ),
            (
                Refusal::PageNotFound,
                404,
                "Page not found",
                "Analyte has no page at this address.",
                OVERVIEW,
                "no admin page at this address",
            ),
            (
                Refusal::FileNotFound,
                404,
                "File not found",
                "Analyte has no file at this address.",
                OVERVIEW,
                "static asset not found",
            ),
            (
                Refusal::WrongMethod(Method::DELETE),
                405,
                "This cannot be done here",
                "Nothing was changed.",
                OVERVIEW,
                "DELETE is not allowed at this address",
            ),
            (
                Refusal::Unreadable {
                    status: StatusCode::UNPROCESSABLE_ENTITY,
                    detail: "<b>bad</b>".into(),
                },
                422,
                "This could not be read",
                "Go back and try again.",
                OVERVIEW,
                "&#60;b&#62;bad&#60;/b&#62;",
            ),
            (
                Refusal::PageFailed {
                    status: StatusCode::NOT_FOUND,
                    title: "Item not found",
                    text: "This page could not be shown.",
                    detail: "parked item not found; the page failed too: x".into(),
                },
                404,
                "Item not found",
                "This page could not be shown.",
                OVERVIEW,
                "the page failed too: x",
            ),
        ])
        .await
    }
}
