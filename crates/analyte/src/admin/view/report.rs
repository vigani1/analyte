//! Holds what pages share to report: a page head with its way back, buttons, statuses, facts, filter chips, the page that only reports, and every way an admin page refuses or fails with its status code, words and the exact reason kept for support.
use super::{
    self as view, Tab, Tone, format,
    format::{Line, Moment, Part},
    frame::Frame,
    icon::Icon,
    refusal::Refusal,
};
use crate::admin::server::Context;
use crate::admin::server::sessions::Session;
use analyte_i18n::{message as tr_args, text as tr};
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

/// One step of the way back: words, an item number in mono, and a link unless it is this page.
pub(in crate::admin) struct Crumb {
    pub(in crate::admin) label: String,
    pub(in crate::admin) number: Option<i64>,
    pub(in crate::admin) href: Option<String>,
}

impl Crumb {
    /// A step back to the page at `href`.
    pub(in crate::admin) fn link(label: impl Into<String>, href: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            number: None,
            href: Some(href.into()),
        }
    }

    /// The last step, this page itself, which is not linked.
    pub(in crate::admin) fn here(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            number: None,
            href: None,
        }
    }
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

impl Value {
    /// The value as people read it, for the command line: the words shown,
    /// a status with what was observed, a time as shown or "Nothing yet".
    pub(in crate::admin) fn plain(&self) -> String {
        match self {
            Self::Text(text) => text.clone(),
            Self::Code { shown, .. } => shown.clone(),
            Self::Status(status) => match &status.note {
                Some(note) => format!("{} ({note})", status.word),
                None => status.word.clone(),
            },
            Self::Link(link) => match &link.detail {
                Some(detail) => format!("{} {detail}", link.kind),
                None => link.kind.to_owned(),
            },
            Self::Time(moment) => moment.as_ref().map_or_else(
                || format::nothing_yet().to_owned(),
                |moment| moment.shown.clone(),
            ),
            Self::Page { label, .. } => label.clone(),
        }
    }
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

impl Report {
    /// A report in `section`, inside `frame` and titled as it is, saying
    /// `alert` and `text` in `tone`, with the section's way back and links.
    /// It has no lines, problems, Restart now or support yet; callers set the
    /// ones they need with struct update syntax.
    pub(in crate::admin) fn new(
        frame: Frame,
        section: Section<'_>,
        tone: Tone,
        alert: impl Into<String>,
        text: impl Into<String>,
    ) -> Self {
        Self {
            head: section.head(frame.title.clone()),
            frame,
            tone,
            title: alert.into(),
            text: text.into(),
            lines: Vec::new(),
            problems: Vec::new(),
            restart: None,
            links: section.links(),
            support: Vec::new(),
        }
    }
}

/// Which pages a report belongs to: its tab, the way back and its links.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::admin) enum Section<'a> {
    /// The Overview, the first page, which has no way back.
    Overview,
    /// Under Needs review, about the item numbered, which has a page.
    NeedsReview(Option<i64>),
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
            Self::Overview => Tab::Overview,
            Self::NeedsReview(_) => Tab::NeedsReview,
            Self::Machines(_) => Tab::Machines,
            Self::LabApps(_) => Tab::LabApps,
            Self::Setup(_) => Tab::Setup,
            Self::More(tab) => tab,
        }
    }

    /// The words of the link to the page of the thing named, and its step
    /// in the way back.
    fn item(self) -> Option<(&'static str, Crumb)> {
        match self {
            Self::NeedsReview(id) => id.map(|id| {
                (
                    format::open_item(),
                    Crumb {
                        label: tr("admin-item").into(),
                        number: Some(id),
                        href: Some(format::parked_href(id)),
                    },
                )
            }),
            Self::Machines(id) => id.map(|id| {
                (
                    format::open_machine(),
                    Crumb::link(id, format::machine_href(id)),
                )
            }),
            Self::LabApps(name) => name.map(|name| {
                (
                    tr("admin-open-the-lab-app"),
                    Crumb::link(name, format::lab_app_href(name)),
                )
            }),
            Self::Setup(id) => id.map(|id| {
                (
                    tr("admin-open-its-setup"),
                    Crumb::link(id, format::setup_href(id, "")),
                )
            }),
            Self::Overview | Self::More(_) => None,
        }
    }

    /// The same section, about nothing in particular: a thing that was not
    /// found has no page to link to.
    const fn unnamed(self) -> Self {
        match self {
            Self::NeedsReview(_) => Self::NeedsReview(None),
            Self::Machines(_) => Self::Machines(None),
            Self::LabApps(_) => Self::LabApps(None),
            Self::Setup(_) => Self::Setup(None),
            Self::Overview | Self::More(_) => self,
        }
    }

    /// The head of a page here: "Needs review / Item 412", "Machines /
    /// cobas-e411", "Lab apps / main-lab" or "Setup / cobas-e411" when it is
    /// about one thing, linked back to its page. The Overview has no way back.
    pub(in crate::admin) fn head(self, title: impl Into<String>) -> Head {
        let tab = self.tab();
        let mut crumbs: Vec<Crumb> = (self != Self::Overview)
            .then(|| Crumb::link(tab.label(), tab.href()))
            .into_iter()
            .collect();
        crumbs.extend(self.item().map(|(_, crumb)| crumb));
        Head {
            crumbs,
            title: title.into(),
            status: None,
            lede: None,
        }
    }

    /// The links at the foot of a report: the thing's own page when it has
    /// one, then the list; the first is the main one. The Overview's own
    /// report has none: reloading it is the way forward.
    pub(in crate::admin) fn links(self) -> Vec<Button> {
        if self == Self::Overview {
            return Vec::new();
        }
        let tab = self.tab();
        let mut links: Vec<Button> = self
            .item()
            .and_then(|(label, crumb)| crumb.href.map(|href| (label, href)))
            .map(|(label, href)| Button {
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

    /// What a request for a thing that is not here says: the page's title,
    /// the alert's title, what it says and the exact reason for support.
    fn not_found(self) -> (&'static str, &'static str, &'static str, &'static str) {
        match self {
            Self::NeedsReview(_) => (
                tr("admin-item-not-found"),
                tr("admin-there-is-no-item-with-this-number"),
                tr("admin-go-back-to-needs-review-and-pick-one-from"),
                tr("admin-parked-item-not-found"),
            ),
            Self::Overview
            | Self::Machines(_)
            | Self::LabApps(_)
            | Self::Setup(_)
            | Self::More(_) => (
                tr("admin-machine-not-found"),
                tr("admin-no-machine-is-called-this"),
                tr("admin-go-back-to-machines-and-pick-one-from-the"),
                tr("admin-machine-not-running-and-not-listed"),
            ),
        }
    }

    /// Why pages here need a signed-in browser session.
    fn why_sign_in(self) -> &'static str {
        match self {
            Self::Overview | Self::NeedsReview(_) => {
                tr("admin-this-needs-a-signed-in-browser-session")
            }
            Self::Machines(_) => tr("admin-removing-a-machine-changes-settings-so-it-needs-a"),
            Self::LabApps(_) => tr("admin-lab-apps-hold-addresses-and-keys-so-their-pages"),
            Self::Setup(_) => tr("admin-setup-changes-settings-so-its-pages-need-a-signed"),
            Self::More(_) => tr("admin-analyte-records-who-sends-again-or-changes-a-hold"),
        }
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
        Err(error) => Problem::Store(tr_args(
            "admin-page-could-not-be-written",
            [("error", (error).to_string().into())],
        ))
        .page(state, session, section),
    }
}

/// Why an admin page could not do what was asked.
pub(in crate::admin) enum Problem {
    /// No machine runs or is listed under this name, or no Needs review item
    /// has this number.
    NotFound,
    /// A reading number below 1 was asked for.
    ReadingNumber,
    /// The message has no reading with this number.
    ReadingNotFound,
    /// A page that changes settings or records an action was opened without a
    /// signed-in browser session.
    SignIn,
    /// A Needs review form came without this session's form token or its
    /// confirmation, so nothing was done.
    FormExpired,
    /// The setup step refused; nothing was changed. The detail says why, in words.
    Refused(String),
    /// A Needs review action ran into an error and changed nothing; the detail
    /// says why.
    Failed(String),
    /// The Needs review item was resolved before; resolving or processing it
    /// again is refused.
    AlreadyResolved,
    /// The Needs review item is not linked to exactly one stored message, so
    /// it cannot be processed again; the detail says why.
    NoSingleMessage(String),
    /// Data keeping removed the stored message.
    Purged,
    /// No more original bytes follow at this point.
    NothingMore,
    /// Analyte's settings files could not be read; the detail says why.
    Settings(String),
    /// The store could not be read; the detail says why.
    Store(String),
    /// One of the Overview's reads failed: what Analyte could not read, in
    /// words, and the detail why.
    Unread(&'static str, String),
    /// The reads took longer than [`view::READ_BUDGET`].
    Time,
}

/// What a page that failed tells people to do.
fn reload_after(reason: &str) -> String {
    tr_args("admin-report-reload-after", [("reason", reason.into())])
}

/// Runs a page's reads within [`view::READ_BUDGET`]: what they returned, or
/// the problem the page shows instead, [`Problem::Time`] once the budget is
/// spent, else what their error says. A store read stops at its own deadline;
/// a store request that may write runs to its end even when the page gives up.
pub(in crate::admin) async fn read_within<T, E>(
    read: impl Future<Output = Result<T, E>>,
) -> Result<T, Problem>
where
    Problem: From<E>,
{
    match tokio::time::timeout(view::READ_BUDGET, read).await {
        Ok(read) => read.map_err(Problem::from),
        Err(_) => Err(Problem::Time),
    }
}

impl From<crate::admin::Error> for Problem {
    /// The page for a read that failed: [`Self::Time`] when the store stopped
    /// it at its deadline, as when the page gave up on it, else [`Self::Store`]
    /// with the reason.
    fn from(error: crate::admin::Error) -> Self {
        if error.read_stopped() {
            Self::Time
        } else {
            Self::Store(error.to_string())
        }
    }
}

impl Problem {
    /// Its status code: what went wrong in HTTP's terms.
    pub(in crate::admin) const fn status(&self) -> StatusCode {
        match self {
            Self::NotFound | Self::ReadingNotFound | Self::NothingMore => StatusCode::NOT_FOUND,
            Self::ReadingNumber => StatusCode::BAD_REQUEST,
            Self::SignIn | Self::FormExpired => StatusCode::FORBIDDEN,
            Self::Refused(_)
            | Self::Failed(_)
            | Self::AlreadyResolved
            | Self::NoSingleMessage(_) => StatusCode::CONFLICT,
            Self::Purged => StatusCode::GONE,
            Self::Settings(_) | Self::Store(_) | Self::Unread(..) => {
                StatusCode::SERVICE_UNAVAILABLE
            }
            Self::Time => StatusCode::GATEWAY_TIMEOUT,
        }
    }

    /// The page's title, the alert's title and what it says. The Overview
    /// keeps its own name as the title and says which read failed.
    fn words(&self, section: Section<'_>) -> (&'static str, &'static str, String) {
        const FAILED: &str = "This page could not be shown";
        let overview = (
            Tab::Overview.label(),
            tr("admin-the-overview-could-not-be-shown-just-now"),
        );
        match self {
            Self::NotFound => {
                let (title, alert, text, _) = section.not_found();
                (title, alert, text.into())
            }
            Self::ReadingNumber => (
                tr("admin-reading-not-found"),
                tr("admin-readings-are-numbered-from-1"),
                tr("admin-open-the-message-again-from-its-machine-s-page").into(),
            ),
            Self::ReadingNotFound => (
                tr("admin-reading-not-found"),
                tr("admin-this-message-has-no-reading-with-this-number"),
                tr("admin-its-machine-s-page-lists-its-messages-with-their").into(),
            ),
            Self::SignIn => (
                tr("admin-sign-in-first"),
                section.why_sign_in(),
                tr("admin-sign-in-with-the-admin-key-then-open-the").into(),
            ),
            Self::FormExpired => (
                format::nothing_changed(),
                tr("admin-this-form-is-from-an-older-or-another-session"),
                tr("admin-open-the-item-again-and-repeat-the-action-if").into(),
            ),
            Self::Refused(why) => (
                format::nothing_changed(),
                match section {
                    Section::Machines(_) | Section::LabApps(_) | Section::Setup(_) => {
                        setup_stopped()
                    }
                    Section::Overview | Section::NeedsReview(_) | Section::More(_) => {
                        tr("admin-that-could-not-be-done")
                    }
                },
                format::sentence(why),
            ),
            Self::Failed(_) => (
                tr("admin-that-could-not-be-done"),
                format::nothing_changed(),
                tr("admin-check-the-item-and-try-again-if-this-keeps").into(),
            ),
            Self::AlreadyResolved => (
                tr("admin-already-resolved"),
                tr("admin-this-item-is-already-resolved"),
                tr("admin-its-history-on-the-item-page-says-when-it").into(),
            ),
            Self::NoSingleMessage(_) => (
                tr("admin-cannot-process-this-item-again"),
                tr("admin-this-item-is-not-linked-to-exactly-one-stored"),
                tr("admin-process-again-reads-the-item-s-original-message-so").into(),
            ),
            Self::Purged => (
                tr("admin-the-original-message-is-no-longer-kept"),
                tr("admin-data-keeping-removed-the-original-message"),
                tr("admin-it-cannot-be-shown-or-read-again-the-item").into(),
            ),
            Self::NothingMore => (
                tr("admin-nothing-more-to-show"),
                tr("admin-there-are-no-more-original-bytes-for-this-record"),
                tr("admin-go-back-to-the-item-to-see-its-other").into(),
            ),
            Self::Settings(_) => (
                FAILED,
                tr("admin-analyte-could-not-read-its-settings-just-now"),
                tr("admin-report-reload").into(),
            ),
            Self::Store(_) => (
                FAILED,
                tr("admin-analyte-could-not-read-its-stored-data-just-now"),
                tr("admin-report-reload").into(),
            ),
            Self::Unread(what, _) => (overview.0, overview.1, reload_after(what)),
            Self::Time => {
                let (title, alert) = if section == Section::Overview {
                    overview
                } else {
                    (FAILED, tr("admin-analyte-took-too-long"))
                };
                (title, alert, reload_after(format::slow_read()))
            }
        }
    }

    /// The exact reason, for support; a refusal's reason is already on the page.
    fn detail(&self, section: Section<'_>) -> Option<String> {
        match self {
            Self::NotFound => Some(section.not_found().3.into()),
            Self::ReadingNumber => {
                Some(tr("admin-positive-interpretation-version-required").into())
            }
            Self::ReadingNotFound => Some(tr("admin-interpretation-not-found").into()),
            Self::SignIn => Some(tr("admin-authenticated-session-required").into()),
            Self::FormExpired => Some(tr("admin-authenticated-csrf-session-required").into()),
            Self::Refused(_) => None,
            Self::AlreadyResolved => Some(tr("admin-parked-item-already-resolved").into()),
            Self::Purged => Some(tr("admin-linked-receipt-purged-by-retention").into()),
            Self::NothingMore => Some(tr("admin-no-linked-raw-evidence-at-this-cursor").into()),
            Self::Failed(detail)
            | Self::NoSingleMessage(detail)
            | Self::Settings(detail)
            | Self::Store(detail)
            | Self::Unread(_, detail) => Some(detail.clone()),
            Self::Time => Some(tr_args(
                "admin-admin-deadline-of-ms-exceeded",
                [
                    (
                        "label",
                        (if section == Section::Overview {
                            "overview"
                        } else {
                            section.tab().label()
                        })
                        .to_string()
                        .into(),
                    ),
                    (
                        "as-millis",
                        (view::READ_BUDGET.as_millis()).to_string().into(),
                    ),
                ],
            )),
        }
    }

    /// The report in `section` inside `frame`, which carries this problem's
    /// title; a thing that is not found has no page to link to.
    pub(in crate::admin) fn report(&self, frame: Frame, section: Section<'_>) -> Report {
        let section = if matches!(self, Self::NotFound) {
            section.unnamed()
        } else {
            section
        };
        let (_, alert, text) = self.words(section);
        Report {
            support: self.detail(section).into_iter().collect(),
            ..Report::new(frame, section, Tone::Problem, alert, text)
        }
    }

    /// The page in the new look in `section`. Should the page itself fail to
    /// render, the plain refusal carries both reasons.
    pub(in crate::admin) fn page(
        &self,
        state: &Context,
        session: Option<&Session>,
        section: Section<'_>,
    ) -> Response {
        let (title, ..) = self.words(section);
        match self
            .report(section.frame(state, title, session), section)
            .render()
        {
            Ok(html) => (self.status(), Html(html)).into_response(),
            Err(render) => self.fallback(&state.assets, section, &render),
        }
    }

    /// The plain refusal for when this problem's own page could not be
    /// written: both reasons, for support.
    pub(in crate::admin) fn fallback(
        &self,
        assets: &view::assets::Root,
        section: Section<'_>,
        render: &askama::Error,
    ) -> Response {
        let (title, alert, _) = self.words(section);
        Refusal::PageFailed {
            status: self.status(),
            title,
            text: tr("admin-this-page-could-not-be-shown-give-the-details"),
            detail: tr_args(
                "admin-the-page-failed-too",
                [
                    (
                        "alert",
                        (self.detail(section).unwrap_or_else(|| alert.to_owned()))
                            .to_string()
                            .into(),
                    ),
                    ("render", (render).to_string().into()),
                ],
            ),
        }
        .page(assets)
    }
}

/// Heads a refusal to continue setup.
pub(in crate::admin) fn setup_stopped() -> &'static str {
    tr("admin-setup-stopped")
}
