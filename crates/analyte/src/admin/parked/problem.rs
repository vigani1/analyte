//! Holds every way a Needs review page refuses or fails: its status code, what it tells people and the exact reason kept for support.
use super::page::{Button, Head, Message, item_href};
use crate::admin::server::Context;
use crate::admin::sessions::Session;
use crate::admin::view::{self, Tab, Tone, format, frame::Frame, refusal::Refusal};
use askama::Template as _;
use axum::{
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};
use jiff::Timestamp;

/// Why a Needs review page could not do what was asked.
pub(super) enum Problem {
    /// No parked item has this number.
    NotFound,
    /// The item was resolved before; resolving or processing it again is refused.
    AlreadyResolved(i64),
    /// An action or a confirmation was opened without a signed-in browser session.
    SignIn,
    /// A form came without this session's form token or its confirmation, so
    /// nothing was done.
    FormExpired,
    /// The action ran into an error and changed nothing; the detail says why.
    Failed(String),
    /// The item is not linked to exactly one stored message, so it cannot be
    /// processed again.
    NoSingleMessage(i64, String),
    /// Data keeping removed the stored message.
    Purged,
    /// No more original bytes follow at this point.
    NothingMore,
    /// The store could not be read; the detail says why.
    Store(String),
    /// The reads took longer than [`view::READ_BUDGET`].
    Time,
}

impl Problem {
    /// Its status code: what went wrong in HTTP's terms.
    const fn status(&self) -> StatusCode {
        match self {
            Self::NotFound | Self::NothingMore => StatusCode::NOT_FOUND,
            Self::AlreadyResolved(_) | Self::Failed(_) | Self::NoSingleMessage(..) => {
                StatusCode::CONFLICT
            }
            Self::SignIn | Self::FormExpired => StatusCode::FORBIDDEN,
            Self::Purged => StatusCode::GONE,
            Self::Store(_) => StatusCode::SERVICE_UNAVAILABLE,
            Self::Time => StatusCode::GATEWAY_TIMEOUT,
        }
    }

    /// The page's title, the alert's title and what it says.
    fn words(&self) -> (&'static str, &'static str, String) {
        match self {
            Self::NotFound => (
                "Item not found",
                "There is no item with this number",
                "Go back to Needs review and pick one from the list.".into(),
            ),
            Self::AlreadyResolved(_) => (
                "Already resolved",
                "This item is already resolved",
                "Its history on the item page says when it was resolved, how and why.".into(),
            ),
            Self::SignIn => (
                "Sign in first",
                "This needs a signed-in browser session",
                "Sign in with the admin key, then open the page again.".into(),
            ),
            Self::FormExpired => (
                "Nothing was changed",
                "This form is from an older or another session",
                "Open the item again and repeat the action; if the page asks, sign in first."
                    .into(),
            ),
            Self::Failed(_) => (
                "That could not be done",
                "Nothing was changed",
                concat!(
                    "Check the item and try again; if this keeps happening, give the ",
                    "details below to support.",
                )
                .into(),
            ),
            Self::NoSingleMessage(..) => (
                "Cannot process this item again",
                "This item is not linked to exactly one stored message",
                concat!(
                    "Process again reads the item's original message, so it needs exactly ",
                    "one. Once the item is settled, use Mark resolved.",
                )
                .into(),
            ),
            Self::Purged => (
                "The original message is no longer kept",
                "Data keeping removed the original message",
                "It cannot be shown or read again. The item and its history stay.".into(),
            ),
            Self::NothingMore => (
                "Nothing more to show",
                "There are no more original bytes for this record",
                "Go back to the item to see its other linked records.".into(),
            ),
            Self::Store(_) => (
                "This page could not be shown",
                "Analyte could not read its stored data just now",
                "Reload the page; if this keeps happening, give the details below to support."
                    .into(),
            ),
            Self::Time => (
                "This page could not be shown",
                "Analyte took too long",
                format!(
                    "{} Reload the page; if this keeps happening, give the details below to support.",
                    format::SLOW_READ
                ),
            ),
        }
    }

    /// The exact reason, for support.
    fn detail(&self) -> String {
        match self {
            Self::NotFound => "parked item not found".into(),
            Self::AlreadyResolved(_) => "parked item already resolved".into(),
            Self::SignIn => "authenticated session required".into(),
            Self::FormExpired => "authenticated CSRF session required".into(),
            Self::Failed(detail) | Self::NoSingleMessage(_, detail) | Self::Store(detail) => {
                detail.clone()
            }
            Self::Purged => "linked receipt purged by retention".into(),
            Self::NothingMore => "no linked raw evidence at this cursor".into(),
            Self::Time => format!(
                "admin Needs review deadline of {} ms exceeded",
                view::READ_BUDGET.as_millis()
            ),
        }
    }

    /// Where to go from here: back to the item when it exists, and the list.
    fn links(&self, id: Option<i64>) -> Vec<Button> {
        let item = match self {
            Self::AlreadyResolved(id) | Self::NoSingleMessage(id, _) => Some(*id),
            Self::NotFound => None,
            Self::SignIn
            | Self::FormExpired
            | Self::Failed(_)
            | Self::Purged
            | Self::NothingMore
            | Self::Store(_)
            | Self::Time => id,
        };
        let mut links: Vec<Button> = item
            .map(|id| Button {
                href: item_href(id),
                label: "Open the item",
                class: "btn--primary",
            })
            .into_iter()
            .collect();
        links.push(Button {
            href: Tab::NeedsReview.href().to_owned(),
            label: "Back to Needs review",
            class: if links.is_empty() {
                "btn--primary"
            } else {
                "btn--outline"
            },
        });
        links
    }

    /// The page in the new look, `id` naming the item when the request was about
    /// one; should the page itself fail to render, the plain refusal carries
    /// both reasons.
    pub(super) fn page(
        &self,
        state: &Context,
        session: Option<&Session>,
        id: Option<i64>,
    ) -> Response {
        let (title, alert, text) = self.words();
        let page = Message {
            frame: Frame::new(
                state,
                Tab::NeedsReview,
                title,
                None,
                session,
                Timestamp::now(),
            ),
            head: Head::under(id.filter(|_| !matches!(self, Self::NotFound)), false, title),
            tone: Tone::Problem,
            title: alert.into(),
            text,
            links: self.links(id),
            support: vec![self.detail()],
        };
        match page.render() {
            Ok(html) => (self.status(), Html(html)).into_response(),
            Err(render) => Refusal::PageFailed {
                status: self.status(),
                title,
                text: "This page could not be shown. Give the details below to support.",
                detail: format!("{}; the page failed too: {render}", self.detail()),
            }
            .page(&state.assets),
        }
    }
}
