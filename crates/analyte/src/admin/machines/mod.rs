//! Serves the Machines pages: every machine Analyte runs or its settings list (GET /machines), one machine's page (GET /machines/{id}) with the Remove dialog drawn over it, and how one stored message was read (GET /receipts/{id}/departures).
//!
//! Pages show only what Analyte observed or stored: the link light in each
//! connection type's own words, the model named at startup, and for each
//! stored message its time, its sample IDs as stored, how it was read and how
//! its deliveries stand. Test codes and result values never show; raw IDs only
//! appear under "For support". Reads stop after [`view::READ_BUDGET`] with the
//! error page in the same look.
pub(super) mod departures;
mod page;

pub(super) use page::{DialogItem, Hover, RemoveDialog, Removed, Rule};

use crate::admin::pages::{Cursor, PAGE};
use crate::admin::server::{Context, Machine};
use crate::admin::sessions::Session;
use crate::admin::setup::Listed;
use crate::admin::view::report::{Button, Crumb, Head, Problem, Report, Section, Status, respond};
use crate::admin::view::{self, Tab, Tone, format, frame::Frame, icon::Icon};
use crate::admin::{Error, error::CommandError};
use crate::deadline;
use crate::machine::link::status::Light;
use analyte_model::Identifier;
use analyte_store::{DeliveryState, RecentOutcome, RecentReceipt};
use askama::Template as _;
use axum::{
    Extension,
    extract::{Path, Query, State},
    http::StatusCode,
    response::Response,
};
use jiff::Timestamp;
use page::{Detail, Empty, Fact, Link, List, Message, Notice, Row, Todo, Value};
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
                Self::Waiting { set_up: false, .. } => (Tone::Neutral, "Not set up yet"),
                Self::Waiting { set_up: true, .. } | Self::Running { .. } => {
                    (Tone::Attention, "Saved: starts after a restart")
                }
            };
            return Status {
                tone,
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
            format::STOPPED_SENTENCE.to_owned()
        };
        let exact = format!("{}: {}", link.link().as_str(), link.state());
        let (tone, word, note) = match (settings, running, light) {
            (InSettings::Removed, ..) => (
                Tone::Attention,
                "Removed: stops at the restart",
                Some(observed),
            ),
            (_, false, _) => (Tone::Problem, format::STOPPED, Some(observed)),
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
        .request(until, |store| store.restore_unreconciled())
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
    let read = match tokio::time::timeout(view::READ_BUDGET, read_list(&state)).await {
        Ok(Ok(read)) => read,
        Ok(Err(error)) => {
            return Problem::Store(error.to_string()).page(
                &state,
                session,
                Section::Machines(None),
            );
        }
        Err(_) => return Problem::Time.page(&state, session, Section::Machines(None)),
    };
    match list_page(&state, session, &cursor, read, now) {
        Ok(page) => respond(
            &state,
            session,
            Section::Machines(None),
            StatusCode::OK,
            page.render(),
        ),
        Err(error) => {
            Problem::Store(error.to_string()).page(&state, session, Section::Machines(None))
        }
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
            title: "Analyte’s settings could not be read".into(),
            text: "Machines set up or removed since Analyte started may be missing here.".into(),
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
                title: "Results still waiting from removed machines could not be counted".into(),
                text: String::new(),
            });
            support.push(error);
        }
    }
    if read.restore_paused {
        notices.push(Notice {
            tone: Tone::Attention,
            title: "Order sending is paused after a restore".into(),
            text: format::RESTORE_ADVICE.into(),
        });
    }
    let entries = roster(state, read.listed.as_deref().ok());
    let after = usize::try_from(cursor.after)?;
    let shown = entries.iter().skip(after).take(PAGE);
    let rows: Vec<Row> = shown.map(|entry| row(entry, &read.last, now)).collect();
    let end = after
        .checked_add(rows.len())
        .ok_or(Error::MachineCursorOverflow)?;
    Ok(List {
        frame: Frame::new(state, Tab::Machines, "Machines", None, session, now),
        summary: format::machines_summary(&counts(&entries)),
        problems,
        waiting: entries.iter().any(|entry| {
            matches!(entry, Entry::Running { machine, .. }
                if machine.running.load(Ordering::Acquire)
                    && machine.status.borrow().light() == Light::Waiting)
        }),
        empty: if entries.is_empty() {
            Some(Empty {
                title: "No machine is set up yet",
                text: "Add one, then set it up step by step: its model, its connection, a listen-only test and the save.",
                add: true,
            })
        } else if rows.is_empty() {
            Some(Empty {
                title: "No more machines",
                text: "This page is past the end of the list.",
                add: false,
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
        lab_app: running.map(|machine| machine.lab_app.as_str().to_owned()),
    }
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
                .request::<_, CommandError>(deadline()?, move |store| {
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
    let (listed, mut recent) =
        match tokio::time::timeout(view::READ_BUDGET, read_detail(state, id)).await {
            Ok(Ok(read)) => read,
            Ok(Err(error)) => {
                return Problem::Store(error.to_string()).page(
                    state,
                    session,
                    Section::Machines(Some(id)),
                );
            }
            Err(_) => return Problem::Time.page(state, session, Section::Machines(Some(id))),
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
    let detail = Detail {
        recent_note: more
            .then(|| format!("The newest {RECENT} messages are listed; older ones stay stored.")),
        notices: listed
            .as_ref()
            .err()
            .map(|_| Notice {
                tone: Tone::Problem,
                title: "Analyte’s settings could not be read".into(),
                text: "Whether this machine is still set up is not known just now.".into(),
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
    let mut support = vec![format!("Machine: {id}")];
    let (facts, rules, lede, notices) = match entry {
        Entry::Running { machine, settings } => {
            let (facts, lede) = running_facts(entry, machine, recent, now, &mut support);
            let notices = (*settings == InSettings::Removed)
                .then(|| Notice {
                    tone: Tone::Attention,
                    title: "Removed from Analyte’s settings".into(),
                    text: format::REMOVED_ADVICE.into(),
                })
                .into_iter()
                .collect();
            (facts, Some(machine.rules.as_slice()), Some(lede), notices)
        }
        Entry::Waiting { set_up, .. } => {
            let facts = vec![
                Fact {
                    term: "Model",
                    value: Value::Text(
                        if *set_up {
                            "Shown after Analyte restarts"
                        } else {
                            "Not chosen yet"
                        }
                        .into(),
                    ),
                },
                Fact {
                    term: "In Analyte",
                    value: Value::Text("Not running yet".into()),
                },
            ];
            (facts, None, None, Vec::new())
        }
    };
    support.extend(recent.iter().map(|message| {
        format!(
            "Message {} received {}",
            uuid::Uuid::from(message.metadata.id),
            format::exact_time(message.metadata.received_at)
        )
    }));
    Detail {
        frame: Frame::new(state, Tab::Machines, id.to_owned(), None, session, now),
        head: Head {
            crumbs: vec![
                Crumb {
                    label: Tab::Machines.label().to_owned(),
                    href: Some(Tab::Machines.href().to_owned()),
                },
                Crumb {
                    label: id.to_owned(),
                    href: None,
                },
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
        todo: matches!(entry, Entry::Waiting { set_up: false, .. }).then(|| Todo {
            text: format::SET_UP_ADVICE,
            href: format::setup_href(id, ""),
            label: "Set it up",
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
    support.push(format!("Link: {}", observed.link().as_str()));
    support.push(format!(
        "Settings version: {}",
        format::digest_hex(&machine.digest)
    ));
    let running = machine.running.load(Ordering::Acquire);
    let facts = vec![
        Fact {
            term: "Model",
            value: Value::Text(
                machine
                    .model
                    .clone()
                    .unwrap_or_else(|| "Not named in its definition".into()),
            ),
        },
        Fact {
            term: "Connected by",
            value: Value::Link(link(machine)),
        },
        Fact {
            term: "Connection",
            value: Value::Status(entry.status(now, true)),
        },
        Fact {
            term: "Last data",
            value: Value::Time(last.map(|at| format::moment(at, now))),
        },
        Fact {
            term: "Sends results to",
            value: Value::Page {
                href: Tab::LabApps.href().to_owned(),
                label: machine.lab_app.as_str().to_owned(),
            },
        },
        Fact {
            term: "In Analyte",
            value: Value::Text(if running { "Running" } else { "Stopped" }.into()),
        },
        Fact {
            term: "Settings version",
            value: Value::Code {
                shown: format::digest_short(&machine.digest),
                exact: format::digest_hex(&machine.digest),
            },
        },
    ];
    let lede = format::machine_lede(
        machine.model.as_deref(),
        last.map(|at| format::moment(at, now)),
    );
    (facts, lede)
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
            label: "Test connection",
            class: "btn--outline",
        });
        actions.push(Button {
            href: format::setup_href(id, ""),
            label: "Edit setup",
            class: "btn--outline",
        });
    }
    actions.push(Button {
        href: format::setup_href(id, "remove"),
        label: "Remove",
        class: "btn--danger-quiet",
    });
    actions
}

/// One stored message as its machine's page lists it.
fn recent_message(message: &RecentReceipt, now: Timestamp) -> Message {
    let outcome = message.reading.as_ref().map(|reading| &reading.outcome);
    let (tone, exact) = match &message.reading {
        None => (Tone::Neutral, "no reading stored".to_owned()),
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
            word: format::reading(outcome),
            exact,
            note: None,
        },
        review: message.review.map(|item| Button {
            href: format!("{}/{item}", Tab::NeedsReview.href()),
            label: "In Needs review",
            class: "link",
        }),
        delivery: format::deliveries(&message.deliveries).map(|(worst, word, exact)| Status {
            tone: delivery_tone(worst),
            word,
            exact,
            note: None,
        }),
        href: message.reading.as_ref().map(|reading| {
            format!(
                "/receipts/{}/departures?version={}",
                uuid::Uuid::from(message.metadata.id),
                reading.version
            )
        }),
    }
}

/// The tone of a delivery state: delivered is good, waiting is neutral, paused
/// needs a look, rejected or in conflict is a problem.
const fn delivery_tone(state: DeliveryState) -> Tone {
    match state {
        DeliveryState::Done => Tone::Good,
        DeliveryState::Pending | DeliveryState::InFlight => Tone::Neutral,
        DeliveryState::Paused => Tone::Attention,
        DeliveryState::Parked | DeliveryState::Conflict => Tone::Problem,
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
    let title = "Machine removed";
    // A machine that still runs keeps its page until the restart; one that did
    // not run has none any more.
    let page_left = state
        .machines
        .iter()
        .any(|machine| machine.id.as_str() == id);
    let mut links: Vec<Button> = page_left
        .then(|| Button {
            href: format::machine_href(id),
            label: "Open the machine",
            class: "btn--outline",
        })
        .into_iter()
        .collect();
    links.push(Button {
        href: Tab::Machines.href().to_owned(),
        label: "Back to Machines",
        class: "btn--outline",
    });
    let page = Report {
        frame: Frame::new(
            state,
            Tab::Machines,
            title,
            None,
            Some(session),
            Timestamp::now(),
        ),
        head: Section::Machines(page_left.then_some(id)).head(title),
        tone: Tone::Good,
        title: format!("{id} is removed from Analyte’s settings"),
        text: format::REMOVED_ADVICE.into(),
        lines: std::iter::once(removed.kept).chain(removed.lines).collect(),
        problems: removed.problems,
        restart: Some(removed.csrf),
        links,
        support: removed.support,
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
