//! Holds what pages share to report: a page head with its way back, buttons, statuses, facts, filter chips, the page that only reports, and every way a Machines, Lab apps, Setup or More page refuses or fails with its status code, words and the exact reason kept for support.
use super::{
    self as view, Tab, Tone, format,
    format::{Line, Moment, Part},
    frame::Frame,
    icon::Icon,
    refusal::Refusal,
};
use crate::admin::server::Context;
use crate::admin::server::sessions::Session;
use askama::Template as _;
use axum::{
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};
use jiff::Timestamp;

/// A status: tone, word, the exact value on hover and what was observed under it.
pub(in crate::admin) struct Status {
    pub(in crate::admin) tone: Tone,
    /// Usually the tone's own icon; a state with an icon of its own, such as
    /// an order's, keeps it on every page.
    pub(in crate::admin) icon: Icon,
    pub(in crate::admin) word: String,
    pub(in crate::admin) exact: String,
    /// What Analyte observed, shown while the link is not open.
    pub(in crate::admin) note: Option<String>,
}

/// The head of a page about one thing: the way back, the title with its status and a line under it.
pub(in crate::admin) struct Head {
    pub(in crate::admin) crumbs: Vec<Crumb>,
    pub(in crate::admin) title: String,
    pub(in crate::admin) status: Option<Status>,
    pub(in crate::admin) lede: Option<Line>,
}

/// One step of the way back, linked unless it is this page.
pub(in crate::admin) struct Crumb {
    pub(in crate::admin) label: String,
    pub(in crate::admin) href: Option<String>,
}

/// A link drawn as a button: where it goes, its words and its kind.
pub(in crate::admin) struct Button {
    pub(in crate::admin) href: String,
    pub(in crate::admin) label: &'static str,
    pub(in crate::admin) class: &'static str,
}

/// One key and its value on a page about one thing.
pub(in crate::admin) struct Fact {
    pub(in crate::admin) term: &'static str,
    pub(in crate::admin) value: Value,
}

/// A fact's value, as it is drawn.
pub(in crate::admin) enum Value {
    Text(String),
    /// Words in mono with their exact value on hover, such as a settings version.
    Code {
        shown: String,
        exact: String,
    },
    Status(Status),
    Link(Link),
    /// A time, or "Nothing yet".
    Time(Option<Moment>),
    /// A page about it.
    Page {
        href: String,
        label: String,
    },
}

/// How a machine is attached: the icon, the kind and where.
pub(in crate::admin) struct Link {
    pub(in crate::admin) icon: Icon,
    pub(in crate::admin) kind: &'static str,
    pub(in crate::admin) detail: Option<String>,
    /// What the connection does that people may not expect.
    pub(in crate::admin) note: Option<&'static str>,
}

/// One filter link with its count.
pub(in crate::admin) struct Chip {
    pub(in crate::admin) href: &'static str,
    pub(in crate::admin) label: &'static str,
    pub(in crate::admin) count: u64,
    pub(in crate::admin) current: bool,
}

/// A page that only reports: what an action did, or why a request was refused.
#[derive(askama::Template)]
#[template(path = "pages/view/report.html")]
pub(in crate::admin) struct Report {
    pub(in crate::admin) frame: Frame,
    pub(in crate::admin) head: Head,
    pub(in crate::admin) tone: Tone,
    pub(in crate::admin) title: String,
    pub(in crate::admin) text: String,
    pub(in crate::admin) lines: Vec<String>,
    pub(in crate::admin) problems: Vec<String>,
    /// Restart now, with the session's form token, after a change to the settings.
    pub(in crate::admin) restart: Option<String>,
    pub(in crate::admin) links: Vec<Button>,
    pub(in crate::admin) support: Vec<String>,
}

/// Which pages a report belongs to: its tab, the way back and its links.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::admin) enum Section<'a> {
    /// Under Machines, about the machine named, which has a page.
    Machines(Option<&'a str>),
    /// Under Lab apps, about the lab app named, which has a page.
    LabApps(Option<&'a str>),
    /// Under Setup, about the machine named, whose setup steps start at step 1.
    Setup(Option<&'a str>),
    /// Under a page in the sidebar's More group, such as Deliveries.
    More(Tab),
}

impl Section<'_> {
    /// The sidebar entry the page sits under.
    const fn tab(self) -> Tab {
        match self {
            Self::Machines(_) => Tab::Machines,
            Self::LabApps(_) => Tab::LabApps,
            Self::Setup(_) => Tab::Setup,
            Self::More(tab) => tab,
        }
    }

    /// The link to the page of the thing named, its words and the thing's name.
    fn item(self) -> Option<(&'static str, String, String)> {
        match self {
            Self::Machines(id) => {
                id.map(|id| ("Open the machine", id.to_owned(), format::machine_href(id)))
            }
            Self::LabApps(name) => name.map(|name| {
                (
                    "Open the lab app",
                    name.to_owned(),
                    format::lab_app_href(name),
                )
            }),
            Self::Setup(id) => {
                id.map(|id| ("Open its setup", id.to_owned(), format::setup_href(id, "")))
            }
            Self::More(_) => None,
        }
    }

    /// The head of a page here: "Machines / cobas-e411", "Lab apps /
    /// main-lab" or "Setup / cobas-e411" when it is about one thing, linked
    /// back to its page.
    pub(in crate::admin) fn head(self, title: impl Into<String>) -> Head {
        let tab = self.tab();
        let mut crumbs = vec![Crumb {
            label: tab.label().to_owned(),
            href: Some(tab.href().to_owned()),
        }];
        crumbs.extend(self.item().map(|(_, label, href)| Crumb {
            label,
            href: Some(href),
        }));
        Head {
            crumbs,
            title: title.into(),
            status: None,
            lede: None,
        }
    }

    /// The links at the foot of a report: the thing's own page when it has
    /// one, then the list; the first is the main one.
    pub(in crate::admin) fn links(self) -> Vec<Button> {
        let tab = self.tab();
        let mut links: Vec<Button> = self
            .item()
            .map(|(label, _, href)| Button {
                href,
                label,
                class: "btn--primary",
            })
            .into_iter()
            .collect();
        links.push(Button {
            href: tab.href().to_owned(),
            label: tab.back(),
            class: if links.is_empty() {
                "btn--primary"
            } else {
                "btn--outline"
            },
        });
        links
    }

    /// The frame of a page under this section's tab, made now.
    pub(in crate::admin) fn frame(
        self,
        state: &Context,
        title: impl Into<String>,
        session: Option<&Session>,
    ) -> Frame {
        Frame::new(state, self.tab(), title, None, session, Timestamp::now())
    }
}

/// A page rendered with `status`, or the store-failure page in `section`
/// when it could not be written.
pub(in crate::admin) fn respond(
    state: &Context,
    session: Option<&Session>,
    section: Section<'_>,
    status: StatusCode,
    html: Result<String, askama::Error>,
) -> Response {
    match html {
        Ok(html) => (status, Html(html)).into_response(),
        Err(error) => Problem::Store(format!("page could not be written: {error}"))
            .page(state, session, section),
    }
}

/// Why a Machines, Lab apps, Setup or More page could not do what was asked.
pub(in crate::admin) enum Problem {
    /// No machine runs or is listed under this name.
    NotFound,
    /// A reading number below 1 was asked for.
    ReadingNumber,
    /// The message has no reading with this number.
    ReadingNotFound,
    /// A page that changes settings was opened without a signed-in browser session.
    SignIn,
    /// The setup step refused; nothing was changed. The detail says why, in words.
    Refused(String),
    /// Analyte's settings files could not be read; the detail says why.
    Settings(String),
    /// The store could not be read; the detail says why.
    Store(String),
    /// The reads took longer than [`view::READ_BUDGET`].
    Time,
}

impl Problem {
    /// The page for a read that failed: [`Self::Time`] when the store stopped
    /// it at its deadline, as when the page gave up on it, else [`Self::Store`]
    /// with the reason.
    pub(in crate::admin) fn of(error: &crate::admin::Error) -> Self {
        if error.read_stopped() {
            Self::Time
        } else {
            Self::Store(error.to_string())
        }
    }

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
    fn words(&self, section: Section<'_>) -> (&'static str, &'static str, String) {
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
                match section {
                    Section::Machines(_) => {
                        "Removing a machine changes settings, so it needs a signed-in browser session"
                    }
                    Section::LabApps(_) => {
                        "Lab apps hold addresses and keys, so their pages need a signed-in browser session"
                    }
                    Section::Setup(_) => {
                        "Setup changes settings, so its pages need a signed-in browser session"
                    }
                    Section::More(_) => {
                        "Analyte records who sends again or changes a hold, so this needs a signed-in browser session"
                    }
                },
                "Sign in with the admin key, then open the page again.".into(),
            ),
            Self::Refused(why) => (
                "Nothing was changed",
                match section {
                    Section::Machines(_) | Section::LabApps(_) | Section::Setup(_) => {
                        "Setup could not continue"
                    }
                    Section::More(_) => "That could not be done",
                },
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
    fn detail(&self, section: Section<'_>) -> Option<String> {
        match self {
            Self::NotFound => Some("machine not running and not listed".into()),
            Self::ReadingNumber => Some("positive interpretation version required".into()),
            Self::ReadingNotFound => Some("interpretation not found".into()),
            Self::SignIn => Some("authenticated session required".into()),
            Self::Refused(_) => None,
            Self::Settings(detail) | Self::Store(detail) => Some(detail.clone()),
            Self::Time => Some(format!(
                "admin {} deadline of {} ms exceeded",
                section.tab().label(),
                view::READ_BUDGET.as_millis()
            )),
        }
    }

    /// The page in the new look in `section`; a machine that is not found has
    /// no page to link to. Should the page itself fail to render, the plain
    /// refusal carries both reasons.
    pub(in crate::admin) fn page(
        &self,
        state: &Context,
        session: Option<&Session>,
        section: Section<'_>,
    ) -> Response {
        let (title, alert, text) = self.words(section);
        let section = match section {
            Section::Machines(_) if matches!(self, Self::NotFound) => Section::Machines(None),
            Section::Machines(_) | Section::LabApps(_) | Section::Setup(_) | Section::More(_) => {
                section
            }
        };
        let detail = self.detail(section);
        let page = Report {
            frame: section.frame(state, title, session),
            head: section.head(title),
            tone: Tone::Problem,
            title: alert.into(),
            text,
            lines: Vec::new(),
            problems: Vec::new(),
            restart: None,
            links: section.links(),
            support: detail.iter().cloned().collect(),
        };
        match page.render() {
            Ok(html) => (self.status(), Html(html)).into_response(),
            Err(render) => Refusal::PageFailed {
                status: self.status(),
                title,
                text: "This page could not be shown. Give the details below to support.",
                detail: format!(
                    "{}; the page failed too: {render}",
                    detail.unwrap_or_else(|| alert.to_owned())
                ),
            }
            .page(&state.assets),
        }
    }
}
