//! Serves the Machines pages: every machine Analyte runs or its settings list (GET /machines), one machine's page (GET /machines/{id}) with the Remove dialog drawn over it, and how one stored message was read (GET /receipts/{id}/departures).
//!
//! Pages show only what Analyte observed or stored: the link light in each
//! connection type's own words, the model named at startup, and for each
//! stored message its time, its sample IDs as stored, how it was read and how
//! its deliveries stand. Test codes and result values never show; raw IDs only
//! appear under "Details for support". Reads stop after
//! [`crate::admin::view::READ_BUDGET`] with the error page in the same look.
pub(super) mod departures;
mod page;

pub(super) use page::{DialogItem, RemoveDialog, Removed, Rule};

use crate::admin::server::sessions::Session;
use crate::admin::server::{Context, Machine};
use crate::admin::setup::Listed;
use crate::admin::view::list::{Cursor, Empty, PAGE};
use crate::admin::view::parts::Notice;
use crate::admin::view::report::{
    Button, Crumb, Head, Problem, Report, Section, Status, read_within, respond,
};
use crate::admin::view::report::{Fact, Link, Value};
use crate::admin::view::{Tab, Tone, format, frame::Frame, icon::Icon};
use crate::admin::{Error, error::CommandError};
use crate::deadline;
use crate::machine::link::status::Light;
use analyte_i18n::{message as tr_args, text as tr};
use analyte_model::Identifier;
use analyte_store::{RecentOutcome, RecentReceipt};
use askama::Template as _;
use axum::{
    Extension,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse as _, Response},
};
use jiff::Timestamp;
use page::{Delivery, Detail, List, Message, Row, Todo};
use std::sync::{Arc, atomic::Ordering};

/// How many of a machine's newest stored messages its page lists.
const RECENT: usize = 20;

/// Whether the settings file still lists a machine Analyte runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InSettings {
    Listed,
    /// Taken out of the settings: it stops at the next restart.
    Removed,
    /// The settings file could not be read, so this is not known.
    Unknown,
}

/// One machine the Machines pages know.
enum Entry<'a> {
    /// Analyte runs it now.
    Running {
        machine: &'a Machine,
        settings: InSettings,
    },
    /// Its settings list it, but Analyte does not run it yet: it is not set up,
    /// or it was set up since the last start.
    Waiting { id: String, set_up: bool },
}

impl Entry<'_> {
    fn id(&self) -> &str {
        match self {
            Self::Running { machine, .. } => machine.id.as_str(),
            Self::Waiting { id, .. } => id,
        }
    }

    /// Its status: the link light while it runs, otherwise where its setup stands.
    /// `detail` keeps what was observed for an open link too, for its own page.
    fn status(&self, now: Timestamp, detail: bool) -> Status {
        let Self::Running { machine, settings } = self else {
            let (tone, word) = match self {
                Self::Waiting { set_up: false, .. } => (Tone::Neutral, format::not_set_up()),
                Self::Waiting { set_up: true, .. } | Self::Running { .. } => {
                    (Tone::Attention, format::starts_after_restart())
                }
            };
            return Status {
                tone,
                icon: tone.icon(),
                word: word.into(),
                exact: String::new(),
                note: None,
            };
        };
        let link = machine.status.borrow().clone();
        let light = link.light();
        let running = machine.running.load(Ordering::Acquire);
        let sentence = format::link_sentence(
            &machine.connection,
            light,
            &format::clock(link.changed_at(), now),
            machine.id.as_str(),
        );
        let observed = if running {
            sentence
        } else {
            format::stopped_sentence().to_owned()
        };
        let exact = format!("{}: {}", link.link().as_str(), link.state());
        let (tone, word, note) = match (settings, running, light) {
            (InSettings::Removed, ..) => {
                (Tone::Attention, format::stops_at_restart(), Some(observed))
            }
            (_, false, _) => (Tone::Problem, format::stopped(), Some(observed)),
            (_, true, Light::Live) => (Tone::Good, link.state(), detail.then_some(observed)),
            (_, true, Light::Waiting) => (Tone::Neutral, link.state(), Some(observed)),
            (_, true, Light::Off) => (
                Tone::Problem,
                link.state(),
                Some(if detail {
                    format!("{observed}. {}", format::link_advice(&machine.connection))
                } else {
                    observed
                }),
            ),
        };
        Status {
            tone,
            icon: tone.icon(),
            word: word.into(),
            exact,
            note,
        }
    }
}

/// Every machine the pages know: those Analyte runs, in setup order, then
/// those its settings list that do not run yet, in the settings' order.
/// `listed` is none when the settings file could not be read.
fn roster<'a>(state: &'a Context, listed: Option<&[Listed]>) -> Vec<Entry<'a>> {
    let mut entries: Vec<Entry<'a>> = state
        .machines
        .iter()
        .map(|machine| Entry::Running {
            machine,
            settings: match listed {
                None => InSettings::Unknown,
                Some(listed) if listed.iter().any(|entry| entry.id == machine.id.as_str()) => {
                    InSettings::Listed
                }
                Some(_) => InSettings::Removed,
            },
        })
        .collect();
    for entry in listed.unwrap_or_default() {
        if !state
            .machines
            .iter()
            .any(|machine| machine.id.as_str() == entry.id)
        {
            entries.push(Entry::Waiting {
                id: entry.id.clone(),
                set_up: entry.set_up,
            });
        }
    }
    entries
}

/// How a running machine is attached.
fn link(machine: &Machine) -> Link {
    Link {
        icon: Icon::connection(&machine.connection),
        kind: format::connection_kind(&machine.connection),
        detail: format::connection_detail(&machine.connection),
        note: format::connection_note(&machine.connection),
    }
}

/// What GET /machines reads: the settings file, then one store request.
struct ListRead {
    listed: Result<Vec<Listed>, String>,
    restore_paused: bool,
    /// When the latest stored message from each running machine arrived.
    last: std::collections::BTreeMap<String, Timestamp>,
    /// Results waiting for lab apps no running machine uses, or why they could not be counted.
    kept: Result<Vec<crate::admin::removed::Line>, String>,
}

async fn read_list(state: &Arc<Context>) -> Result<ListRead, Error> {
    let listed = crate::admin::setup::listed(state)
        .await
        .map_err(|error| error.to_string());
    let until = deadline()?;
    let restore_paused = state
        .store
        .read(until, analyte_store::Store::restore_unreconciled)
        .await?;
    let last = crate::admin::overview::activity(state, until).await?;
    let kept = crate::admin::removed::lines(state)
        .await
        .map_err(|error| error.to_string());
    Ok(ListRead {
        listed,
        restore_paused,
        last,
        kept,
    })
}

/// GET /machines.
pub(super) async fn list(
    State(state): State<Arc<Context>>,
    Query(cursor): Query<Cursor>,
    session: Option<Extension<Arc<Session>>>,
) -> Response {
    let session = session.as_ref().map(|Extension(session)| session.as_ref());
    let now = Timestamp::now();
    let read = match read_within(read_list(&state)).await {
        Ok(read) => read,
        Err(problem) => return problem.page(&state, session, Section::Machines(None)),
    };
    match list_page(&state, session, &cursor, read, now) {
        Ok(page) => respond(
            &state,
            session,
            Section::Machines(None),
            StatusCode::OK,
            page.render(),
        ),
        Err(error) => Problem::from(error).page(&state, session, Section::Machines(None)),
    }
}

fn list_page(
    state: &Context,
    session: Option<&Session>,
    cursor: &Cursor,
    read: ListRead,
    now: Timestamp,
) -> Result<List, Error> {
    let mut problems = Vec::new();
    let mut support = Vec::new();
    if let Err(error) = &read.listed {
        problems.push(Notice {
            tone: Tone::Problem,
            title: format::settings_unreadable().into(),
            text: tr("admin-machines-set-up-or-removed-since-analyte-started-may").into(),
        });
        support.push(error.clone());
    }
    let mut notices = Vec::new();
    match read.kept {
        Ok(lines) => notices.extend(lines.iter().map(|line| Notice {
            tone: line.tone(),
            title: line.text().to_owned(),
            text: String::new(),
        })),
        Err(error) => {
            problems.push(Notice {
                tone: Tone::Problem,
                title: crate::admin::removed::count_failed().into(),
                text: String::new(),
            });
            support.push(error);
        }
    }
    if read.restore_paused {
        notices.push(Notice {
            tone: Tone::Attention,
            title: format::restore_paused().into(),
            text: format::restore_advice().into(),
        });
    }
    let entries = roster(state, read.listed.as_deref().ok());
    for entry in &entries {
        if let Entry::Running { machine, .. } = entry {
            notices.extend(peer_notices(machine, now, false));
        }
    }
    let after = usize::try_from(cursor.after)?;
    let shown = entries.iter().skip(after).take(PAGE);
    let rows: Vec<Row> = shown.map(|entry| row(entry, &read.last, now)).collect();
    let end = after
        .checked_add(rows.len())
        .ok_or(Error::MachineCursorOverflow)?;
    Ok(List {
        frame: Frame::new(
            state,
            Tab::Machines,
            Tab::Machines.label(),
            None,
            session,
            now,
        ),
        summary: format::machines_summary(&counts(&entries)),
        problems,
        waiting: entries.iter().any(|entry| {
            matches!(entry, Entry::Running { machine, .. }
                if machine.running.load(Ordering::Acquire)
                    && machine.status.borrow().light() == Light::Waiting)
        }),
        empty: if entries.is_empty() {
            Some(Empty {
                good: false,
                title: tr("admin-no-machine-is-set-up-yet"),
                text: tr("admin-add-one-then-set-it-up-step-by-step"),
                link: Some(("/setup/add", format::add_machine().into())),
            })
        } else if rows.is_empty() {
            Some(Empty {
                good: false,
                title: tr("admin-no-more-machines"),
                text: tr("admin-this-page-is-past-the-end-of-the-list"),
                link: None,
            })
        } else {
            None
        },
        rows,
        notices,
        next: (end < entries.len()).then(|| format!("{}?after={end}", Tab::Machines.href())),
        support,
    })
}

/// How the machines stand, for the line under the title.
fn counts(entries: &[Entry<'_>]) -> format::MachineCounts {
    let mut counts = format::MachineCounts::default();
    for entry in entries {
        let count = match entry {
            Entry::Running { machine, settings } => {
                counts.running = counts.running.saturating_add(1);
                if *settings == InSettings::Removed {
                    counts.removed = counts.removed.saturating_add(1);
                }
                match (
                    machine.running.load(Ordering::Acquire),
                    machine.status.borrow().light(),
                ) {
                    (true, Light::Live) => &mut counts.open,
                    (true, Light::Waiting) => &mut counts.waiting,
                    (false, _) | (true, Light::Off) => &mut counts.problems,
                }
            }
            Entry::Waiting { set_up: false, .. } => &mut counts.not_set_up,
            Entry::Waiting { set_up: true, .. } => &mut counts.saved,
        };
        *count = count.saturating_add(1);
    }
    counts
}

/// One machine's row on the list.
fn row(
    entry: &Entry<'_>,
    last: &std::collections::BTreeMap<String, Timestamp>,
    now: Timestamp,
) -> Row {
    let id = entry.id();
    let running = match entry {
        Entry::Running { machine, .. } => Some(*machine),
        Entry::Waiting { .. } => None,
    };
    let status = running.map(|machine| machine.status.borrow().clone());
    Row {
        id: id.to_owned(),
        href: format::machine_href(id),
        model: running.and_then(|machine| machine.model.clone()),
        status: entry.status(now, false),
        last_data: status.as_ref().and_then(|status| {
            status
                .last_data()
                .max(last.get(id).copied())
                .map(|at| format::moment(at, now))
        }),
        link: running.map(link),
        since: status
            .filter(|status| status.light() == Light::Live)
            .map(|status| format::moment(status.changed_at(), now)),
        lab_app: running.and_then(|machine| {
            machine
                .lab_app
                .as_ref()
                .map(|name| name.as_str().to_owned())
        }),
        recording: running.and_then(|machine| {
            machine
                .status
                .borrow()
                .recording()
                .map(format::recording_counts)
        }),
    }
}

/// A time as the Machines list shows it, with its exact value and RFC 3339 form.
#[derive(serde::Serialize)]
struct LiveTime {
    datetime: String,
    shown: String,
    exact: String,
}

impl From<format::Moment> for LiveTime {
    fn from(moment: format::Moment) -> Self {
        Self {
            datetime: moment.datetime,
            shown: moment.shown,
            exact: moment.exact,
        }
    }
}

/// One row of the Machines list, in the page's own words.
#[derive(serde::Serialize)]
struct LiveRow {
    id: String,
    /// Analyte runs it now, so its port or address is in use.
    running: bool,
    /// The status word: the link state while it runs ("Connected", "Waiting",
    /// "Port open"...), otherwise where its setup stands.
    status: String,
    /// The link and its state as the service names them; empty when it does not run.
    exact: String,
    /// What Analyte observed, as the page says it under the status.
    note: Option<String>,
    last_data: Option<LiveTime>,
    /// When its open link opened.
    since: Option<LiveTime>,
    model: Option<String>,
    lab_app: Option<String>,
    /// The newest connection refused by its `allowed_peers`, in the page's
    /// words with the device's address (ADR 0026); none since the start.
    refused: Option<String>,
    /// Record only (ADR 0034): what it kept since start; none for a normal machine.
    recording: Option<String>,
}

/// The whole answer: the rows, and whether the settings file could be read
/// (if not, machines set up or removed since the start may be missing).
#[derive(serde::Serialize)]
struct Live {
    settings_read: bool,
    machines: Vec<LiveRow>,
}

/// GET /machines.json: the Machines list as data, for the `machines list`
/// command. Like every page but sign-in and `/status`, it needs the admin key
/// or a signed-in session; it says what the page says and never a payload.
pub(super) async fn live(State(state): State<Arc<Context>>) -> Response {
    let now = Timestamp::now();
    let Ok(read) = read_within(read_list(&state)).await else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let entries = roster(&state, read.listed.as_deref().ok());
    let machines = entries
        .iter()
        .map(|entry| {
            let row = row(entry, &read.last, now);
            let refused = match entry {
                Entry::Running { machine, .. } => machine
                    .status
                    .borrow()
                    .refused()
                    .first()
                    .map(|newest| format::refused(newest, &format::clock(newest.at, now))),
                Entry::Waiting { .. } => None,
            };
            LiveRow {
                refused,
                running: matches!(entry, Entry::Running { .. }),
                id: row.id,
                status: row.status.word,
                exact: row.status.exact,
                note: row.status.note,
                last_data: row.last_data.map(LiveTime::from),
                since: row.since.map(LiveTime::from),
                model: row.model,
                lab_app: row.lab_app,
                recording: row.recording,
            }
        })
        .collect();
    let mut response = axum::Json(Live {
        settings_read: read.listed.is_ok(),
        machines,
    })
    .into_response();
    response.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    response
}

/// GET /machines/{id}.
pub(super) async fn detail(
    State(state): State<Arc<Context>>,
    Path(id): Path<String>,
    session: Option<Extension<Arc<Session>>>,
) -> Response {
    let session = session.as_ref().map(|Extension(session)| session.as_ref());
    page(&state, session, &id, None).await
}

/// The machine's page with the Remove dialog drawn over it.
pub(in crate::admin) async fn with_dialog(
    state: &Arc<Context>,
    session: &Session,
    id: &str,
    dialog: RemoveDialog,
) -> Response {
    page(state, Some(session), id, Some(dialog)).await
}

/// What a machine's page reads: the settings file, then its newest messages.
async fn read_detail(
    state: &Arc<Context>,
    id: &str,
) -> Result<(Result<Vec<Listed>, String>, Vec<RecentReceipt>), Error> {
    let listed = crate::admin::setup::listed(state)
        .await
        .map_err(|error| error.to_string());
    // A name the store cannot hold has no stored messages.
    let recent = match Identifier::new(id) {
        Ok(machine) => {
            state
                .store
                .read::<_, CommandError>(deadline()?, move |store| {
                    Ok(store.recent_receipts(&machine, RECENT.saturating_add(1))?)
                })
                .await?
        }
        Err(_) => Vec::new(),
    };
    Ok((listed, recent))
}

async fn page(
    state: &Arc<Context>,
    session: Option<&Session>,
    id: &str,
    dialog: Option<RemoveDialog>,
) -> Response {
    let now = Timestamp::now();
    let (listed, mut recent) = match read_within(read_detail(state, id)).await {
        Ok(read) => read,
        Err(problem) => return problem.page(state, session, Section::Machines(Some(id))),
    };
    let entries = roster(state, listed.as_deref().ok());
    let Some(entry) = entries.iter().find(|entry| entry.id() == id) else {
        return match listed {
            Ok(_) => Problem::NotFound.page(state, session, Section::Machines(None)),
            Err(error) => Problem::Settings(error).page(state, session, Section::Machines(None)),
        };
    };
    let more = recent.len() > RECENT;
    recent.truncate(RECENT);
    let detail = detail_page(state, session, entry, &recent, now, dialog);
    let software = match read_within(software_version::detail(state, session, id)).await {
        Ok(software) => software,
        Err(problem) => return problem.page(state, session, Section::Machines(Some(id))),
    };
    let detail = Detail {
        software,
        recent_note: more.then(|| {
            tr_args(
                "admin-the-newest-messages-are-listed-older-ones-stay-stored",
                [("RECENT", (RECENT).to_string().into())],
            )
        }),
        notices: listed
            .as_ref()
            .err()
            .map(|_| Notice {
                tone: Tone::Problem,
                title: format::settings_unreadable().into(),
                text: tr("admin-whether-this-machine-is-still-set-up-is-not").into(),
            })
            .into_iter()
            .chain(detail.notices)
            .collect(),
        support: detail.support.into_iter().chain(listed.err()).collect(),
        ..detail
    };
    respond(
        state,
        session,
        Section::Machines(Some(id)),
        StatusCode::OK,
        detail.render(),
    )
}

/// A machine's page from what was read.
fn detail_page(
    state: &Context,
    session: Option<&Session>,
    entry: &Entry<'_>,
    recent: &[RecentReceipt],
    now: Timestamp,
    dialog: Option<RemoveDialog>,
) -> Detail {
    let id = entry.id();
    let status = entry.status(now, true);
    let mut support = vec![tr_args(
        "admin-machine-2",
        [("id", (id).to_string().into())],
    )];
    let (facts, rules, lede, notices) = match entry {
        Entry::Running { machine, settings } => {
            let (facts, lede) = running_facts(entry, machine, recent, now, &mut support);
            let mut notices: Vec<Notice> = (*settings == InSettings::Removed)
                .then(|| Notice {
                    tone: Tone::Attention,
                    title: tr("admin-removed-from-analyte-s-settings").into(),
                    text: format::removed_advice().into(),
                })
                .into_iter()
                .collect();
            notices.extend(peer_notices(machine, now, true));
            (facts, Some(machine.rules.as_slice()), Some(lede), notices)
        }
        Entry::Waiting { set_up, .. } => {
            let facts = vec![
                Fact {
                    term: format::model(),
                    value: Value::Text(
                        if *set_up {
                            tr("admin-shown-after-analyte-restarts")
                        } else {
                            format::not_chosen()
                        }
                        .into(),
                    ),
                },
                Fact {
                    term: format::in_analyte(),
                    value: Value::Text(format::not_running().into()),
                },
            ];
            (facts, None, None, Vec::new())
        }
    };
    support.extend(recent.iter().map(|message| {
        tr_args(
            "admin-message-received",
            [
                (
                    "id",
                    (uuid::Uuid::from(message.metadata.id)).to_string().into(),
                ),
                (
                    "received-at",
                    (format::exact_time(message.metadata.received_at))
                        .to_string()
                        .into(),
                ),
            ],
        )
    }));
    Detail {
        frame: Frame::new(state, Tab::Machines, id.to_owned(), None, session, now),
        head: Head {
            crumbs: vec![
                Crumb::link(Tab::Machines.label(), Tab::Machines.href()),
                Crumb::here(id),
            ],
            title: id.to_owned(),
            status: Some(Status {
                note: None,
                ..status
            }),
            lede,
        },
        actions: actions(entry),
        notices,
        software: None,
        todo: matches!(entry, Entry::Waiting { set_up: false, .. }).then(|| Todo {
            text: format::set_up_advice(),
            href: format::setup_href(id, ""),
            label: tr("admin-set-it-up"),
        }),
        facts,
        declarations: rules
            .unwrap_or_default()
            .iter()
            .map(|rule| rule.declaration.clone())
            .collect(),
        rules: rules.map(<[Rule]>::to_vec),
        recent: recent
            .iter()
            .map(|message| recent_message(message, now))
            .collect(),
        recent_note: None,
        support,
        dialog,
    }
}

/// What a running machine's page says about its setup, and the line under its
/// name; what only support needs goes to `support`.
/// The machine's link and settings version, exactly, for support.
fn support_lines(link: &str, digest: &[u8]) -> [String; 2] {
    [
        tr_args("admin-support-link", [("link", link.into())]),
        tr_args(
            "admin-settings-version",
            [("digest", format::digest_hex(digest).into())],
        ),
    ]
}

fn running_facts(
    entry: &Entry<'_>,
    machine: &Machine,
    recent: &[RecentReceipt],
    now: Timestamp,
    support: &mut Vec<String>,
) -> (Vec<Fact>, format::Line) {
    let observed = machine.status.borrow().clone();
    let last = observed
        .last_data()
        .max(recent.first().map(|newest| newest.metadata.received_at));
    support.extend(support_lines(observed.link().as_str(), &machine.digest));
    let running = machine.running.load(Ordering::Acquire);
    let mut facts = vec![
        Fact {
            term: format::model(),
            value: Value::Text(
                machine
                    .model
                    .clone()
                    .unwrap_or_else(|| tr("admin-not-named-in-its-definition").into()),
            ),
        },
        Fact {
            term: format::connected_by(),
            value: Value::Link(link(machine)),
        },
        Fact {
            term: format::connection(),
            value: Value::Status(entry.status(now, true)),
        },
    ];
    if let Some(accepts) = format::accepts(&machine.connection, machine.allowed_peers.as_ref()) {
        facts.push(Fact {
            term: format::accepts_from(),
            value: Value::Text(accepts),
        });
    }
    if let Some((cable, suggested)) = &machine.cable {
        facts.push(Fact {
            term: format::cable_settings(),
            value: Value::Text(super::setup::cable::summary(cable, suggested.as_ref())),
        });
    }
    if let Some(choices) = super::setup::choices::summary(&machine.choices) {
        facts.push(Fact {
            term: format::analyzer_settings(),
            value: Value::Text(choices),
        });
    }
    if let Some(repeats) = machine.repeated_results {
        facts.push(repeated_results(repeats));
    }
    facts.extend([
        Fact {
            term: format::last_data(),
            value: Value::Time(last.map(|at| format::moment(at, now))),
        },
        Fact {
            term: format::sends_results_to(),
            value: match &machine.lab_app {
                Some(lab_app) => Value::Page {
                    href: Tab::LabApps.href().to_owned(),
                    label: lab_app.as_str().to_owned(),
                },
                None => Value::Text(format::recording_only().into()),
            },
        },
        Fact {
            term: tr("admin-recorded"),
            value: Value::Text(observed.recording().map_or_else(
                || tr("admin-only-when-its-program-entry-has-capture-true").to_owned(),
                format::recording_counts,
            )),
        },
        Fact {
            term: format::in_analyte(),
            value: Value::Text(
                if running {
                    tr("admin-running")
                } else {
                    tr("admin-stopped-2")
                }
                .into(),
            ),
        },
        Fact {
            term: tr("admin-settings-version-2"),
            value: Value::Code {
                shown: format::digest_short(&machine.digest),
                exact: format::digest_hex(&machine.digest),
            },
        },
    ]);
    let lede = format::machine_lede(
        machine.model.as_deref(),
        last.map(|at| format::moment(at, now)),
    );
    (facts, lede)
}

fn repeated_results(repeats: analyte_config::RepeatedResults) -> Fact {
    Fact {
        term: tr("admin-repeated-results"),
        value: Value::Text(
            match repeats {
                analyte_config::RepeatedResults::Automatic => {
                    tr("admin-handled-automatically-a-resend-is-kept-once-a-correction")
                }
                analyte_config::RepeatedResults::AlwaysAsk => {
                    tr("admin-always-ask-me-every-repeat-waits-in-needs-review")
                }
            }
            .into(),
        ),
    }
}

/// What a listening machine's page says about the devices that may connect
/// (ADR 0026): a warning when any device is accepted, and each device refused
/// since Analyte started, newest first. `all` lists every refused device, for
/// the machine's own page; the list of machines names only the newest.
fn peer_notices(machine: &Machine, now: Timestamp, all: bool) -> Vec<Notice> {
    let named = |title: String| {
        if all {
            title
        } else {
            format!("{}: {title}", machine.id.as_str())
        }
    };
    let mut notices = Vec::new();
    if machine.accepts_any_device() {
        notices.push(Notice {
            tone: Tone::Attention,
            title: named(format::open_to_any().into()),
            text: format::open_to_any_advice().into(),
        });
    }
    let status = machine.status.borrow().clone();
    let shown = if all { status.refused().len() } else { 1 };
    notices.extend(status.refused().iter().take(shown).map(|refusal| Notice {
        tone: Tone::Attention,
        title: named(format::refused(refusal, &format::clock(refusal.at, now))),
        text: format::refused_advice().into(),
    }));
    notices
}

/// What can be done with a machine: only a listed one can be set up, tested or removed.
fn actions(entry: &Entry<'_>) -> Vec<Button> {
    let id = entry.id();
    let set_up = match entry {
        Entry::Running {
            settings: InSettings::Listed,
            ..
        }
        | Entry::Waiting { set_up: true, .. } => true,
        Entry::Waiting { set_up: false, .. } => false,
        Entry::Running {
            settings: InSettings::Removed | InSettings::Unknown,
            ..
        } => return Vec::new(),
    };
    let mut actions = Vec::new();
    if set_up {
        actions.push(Button {
            href: format::setup_href(id, "test"),
            label: tr("admin-test-connection"),
            class: "btn--outline",
        });
        actions.push(Button {
            href: format::setup_href(id, ""),
            label: tr("admin-edit-setup"),
            class: "btn--outline",
        });
    }
    actions.push(Button {
        href: format::setup_href(id, "remove"),
        label: crate::admin::setup::remove_label(),
        class: "btn--danger-quiet",
    });
    actions
}

/// One stored message as its machine's page lists it.
fn recent_message(message: &RecentReceipt, now: Timestamp) -> Message {
    let outcome = message.reading.as_ref().map(|reading| &reading.outcome);
    let (tone, exact) = match &message.reading {
        None => (Tone::Neutral, tr("admin-no-reading-stored").to_owned()),
        Some(reading) => (
            match reading.outcome {
                RecentOutcome::Results { .. } => Tone::Good,
                RecentOutcome::Failed => Tone::Problem,
            },
            format!("reading {}", reading.version),
        ),
    };
    Message {
        when: format::moment(message.metadata.received_at, now),
        sample: match outcome {
            Some(RecentOutcome::Results { samples, .. }) => format::samples(samples),
            Some(RecentOutcome::Failed) | None => None,
        },
        reading: Status {
            tone,
            icon: tone.icon(),
            word: format::reading(outcome),
            exact,
            note: None,
        },
        review: message.review.map(|item| Button {
            href: format!("{}/{item}", Tab::NeedsReview.href()),
            label: tr("admin-in-needs-review"),
            class: "link",
        }),
        delivery: format::deliveries(&message.deliveries).map(|(worst, words, exact)| Delivery {
            look: format::delivery_look(worst),
            words,
            exact,
        }),
        no_delivery: if message.reading.is_some() {
            format::not_applicable()
        } else {
            format::nothing_yet()
        },
        href: message.reading.as_ref().map(|reading| {
            format!(
                "/receipts/{}/departures?version={}",
                uuid::Uuid::from(message.metadata.id),
                reading.version
            )
        }),
    }
}

/// The page after a machine was removed from the settings: what changed, what
/// went wrong after, and Restart now.
pub(in crate::admin) fn removed(
    state: &Context,
    session: &Session,
    id: &str,
    removed: Removed,
) -> Response {
    let title = tr("admin-machine-removed");
    // A machine that still runs keeps its page until the restart; one that did
    // not run has none any more.
    let page_left = state
        .machines
        .iter()
        .any(|machine| machine.id.as_str() == id);
    let section = Section::Machines(page_left.then_some(id));
    let mut links: Vec<Button> = page_left
        .then(|| Button {
            href: format::machine_href(id),
            label: format::open_machine(),
            class: "btn--outline",
        })
        .into_iter()
        .collect();
    links.push(Button {
        href: Tab::Machines.href().to_owned(),
        label: Tab::Machines.back(),
        class: "btn--outline",
    });
    let page = Report {
        lines: std::iter::once(removed.kept).chain(removed.lines).collect(),
        problems: removed.problems,
        restart: Some(removed.csrf),
        links,
        support: removed.support,
        ..Report::new(
            section.frame(state, title, Some(session)),
            section,
            Tone::Good,
            tr_args(
                "admin-is-removed-from-analyte-s-settings",
                [("id", (id).to_string().into())],
            ),
            format::removed_advice(),
        )
    };
    respond(
        state,
        Some(session),
        Section::Machines(Some(id)),
        StatusCode::OK,
        page.render(),
    )
}

/// The departures a machine's settings switch on, read once at startup.
pub(in crate::admin) fn rules(config: &analyte_config::Validated) -> Result<Vec<Rule>, Error> {
    config
        .effective()
        .machine()
        .deviations
        .iter()
        .map(|rule| {
            Ok(Rule {
                words: format::departure(rule),
                meaning: rule.meaning(),
                declaration: rule.evidence_snapshot()?.as_str().to_owned(),
            })
        })
        .collect()
}
pub(in crate::admin) mod software_version;
