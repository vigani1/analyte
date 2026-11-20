//! Holds every way a Machines page refuses or fails: its status code, what it tells people and the exact reason kept for support.
use super::page::{Button, Head, Report};
use crate::admin::server::Context;
use crate::admin::sessions::Session;
use crate::admin::view::{self, Tab, Tone, format, frame::Frame};
use askama::Template as _;
use axum::{
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};
use jiff::Timestamp;

/// Why a Machines page could not do what was asked.
pub(in crate::admin) enum Problem {
    /// No machine runs or is listed under this name.
    NotFound,
    /// A reading number below 1 was asked for.
    ReadingNumber,
    /// The message has no reading with this number.
    ReadingNotFound,
    /// Removing was opened without a signed-in browser session.
    SignIn,
    /// The setup step refused; nothing was changed. The detail says why, in words.
    Refused(String),
    /// Analyte's settings file could not be read; the detail says why.
    Settings(String),
    /// The store could not be read; the detail says why.
    Store(String),
    /// The reads took longer than [`view::READ_BUDGET`].
    Time,
}

impl Problem {
    /// Its status code: what went wrong in HTTP's terms.
    const fn status(&self) -> StatusCode {
        match self {
            Self::NotFound | Self::ReadingNotFound => StatusCode::NOT_FOUND,
            Self::ReadingNumber => StatusCode::BAD_REQUEST,
            Self::SignIn => StatusCode::FORBIDDEN,
            Self::Refused(_) => StatusCode::CONFLICT,
            Self::Settings(_) | Self::Store(_) => StatusCode::SERVICE_UNAVAILABLE,
            Self::Time => StatusCode::GATEWAY_TIMEOUT,
        }
    }

    /// The page's title, the alert's title and what it says.
    fn words(&self) -> (&'static str, &'static str, String) {
        match self {
            Self::NotFound => (
                "Machine not found",
                "No machine is called this",
                "Go back to Machines and pick one from the list.".into(),
            ),
            Self::ReadingNumber => (
                "Reading not found",
                "Readings are numbered from 1",
                "Open the message again from its machine’s page.".into(),
            ),
            Self::ReadingNotFound => (
                "Reading not found",
                "This message has no reading with this number",
                "Its machine’s page lists its messages with their latest reading.".into(),
            ),
            Self::SignIn => (
                "Sign in first",
                "Removing a machine changes settings, so it needs a signed-in browser session",
                "Sign in with the admin key, then open the page again.".into(),
            ),
            Self::Refused(why) => (
                "Nothing was changed",
                "Setup could not continue",
                format::sentence(why),
            ),
            Self::Settings(_) => (
                "This page could not be shown",
                "Analyte could not read its settings just now",
                "Reload the page; if this keeps happening, give the details below to support."
                    .into(),
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

    /// The exact reason, for support; a refusal's reason is already on the page.
    fn detail(&self) -> Option<String> {
        match self {
            Self::NotFound => Some("machine not running and not listed".into()),
            Self::ReadingNumber => Some("positive interpretation version required".into()),
            Self::ReadingNotFound => Some("interpretation not found".into()),
            Self::SignIn => Some("authenticated session required".into()),
            Self::Refused(_) => None,
            Self::Settings(detail) | Self::Store(detail) => Some(detail.clone()),
            Self::Time => Some(format!(
                "admin Machines deadline of {} ms exceeded",
                view::READ_BUDGET.as_millis()
            )),
        }
    }

    /// The page in the new look, `machine` naming the machine the request was
    /// about; should the page itself fail to render, the plain refusal carries
    /// both reasons.
    pub(in crate::admin) fn page(
        &self,
        state: &Context,
        session: Option<&Session>,
        machine: Option<&str>,
    ) -> Response {
        let (title, alert, text) = self.words();
        let machine = machine.filter(|_| !matches!(self, Self::NotFound));
        let mut links: Vec<Button> = machine
            .map(|id| Button {
                href: format::machine_href(id),
                label: "Open the machine",
                class: "btn--primary",
            })
            .into_iter()
            .collect();
        links.push(Button {
            href: Tab::Machines.href().to_owned(),
            label: "Back to Machines",
            class: if links.is_empty() {
                "btn--primary"
            } else {
                "btn--outline"
            },
        });
        let page = Report {
            frame: Frame::new(state, Tab::Machines, title, None, session, Timestamp::now()),
            head: Head::under(machine, title),
            tone: Tone::Problem,
            title: alert.into(),
            text,
            lines: Vec::new(),
            problems: Vec::new(),
            restart: None,
            links,
            support: self.detail().into_iter().collect(),
        };
        match page.render() {
            Ok(html) => (self.status(), Html(html)).into_response(),
            Err(render) => view::refusal(
                self.status(),
                title,
                "This page could not be shown. Give the details below to support.",
                format!(
                    "{}; the page failed too: {render}",
                    self.detail().unwrap_or_else(|| alert.to_owned())
                ),
            ),
        }
    }
}

impl Head {
    /// A page under Machines: "Machines / cobas-e411" when it is about one
    /// machine, linked back to the machine's page.
    pub(super) fn under(machine: Option<&str>, title: impl Into<String>) -> Self {
        let mut crumbs = vec![super::page::Crumb {
            label: Tab::Machines.label().to_owned(),
            href: Some(Tab::Machines.href().to_owned()),
        }];
        if let Some(id) = machine {
            crumbs.push(super::page::Crumb {
                label: id.to_owned(),
                href: Some(format::machine_href(id)),
            });
        }
        Self {
            crumbs,
            title: title.into(),
            status: None,
            lede: None,
        }
    }
}
