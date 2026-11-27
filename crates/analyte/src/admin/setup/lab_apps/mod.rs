//! Serves the Lab apps pages: each lab app is set once (address, timing, keys) and machines pick it.
//!
//! Lab apps live in `lab-apps.toml` beside the program file. Older setups also
//! write one inside a machine's site file or keep one in the program file; the
//! pages list those too, and a save moves them into `lab-apps.toml`. Every write
//! goes through the same checked commit as the machine setup (backup, safe
//! write, reload, rollback) and applies after a restart. Keys go only to the
//! locked credentials file and are never shown: a page says only where the
//! service finds each one (`secrets::source`). How each lab app stands follows
//! the Overview's lab app rules; reads stop after [`view::READ_BUDGET`].
pub(super) mod change;
mod keys;
mod page;
pub(super) mod relay;
mod scan;

use change::{
    Change, Entered, Saved, add_blocking, blockers, count, reason, remove_blocking, removed_reason,
    save_blocking, waiting,
};
pub(super) use keys::{API_KEY, KeySource, SIGNING_KEY, missing_keys};
pub(in crate::admin) use keys::{Found, key_sources};
use scan::Known;
pub(super) use scan::{
    Scan, SiteLabApp, conflict_sentence, scan, scan_with_site, site_lab_app, where_written,
};

use super::{Csrf, RELOAD, RESTART_REQUIRED, SetupError, Via, audit, copies, off_thread};
use crate::admin::machines::{DialogItem, RemoveDialog};
use crate::admin::overview::{self, Delivery, Standing};
use crate::admin::removed;
use crate::admin::view::parts::{Hover, Notice};
use crate::admin::{
    server::{Context, sessions::Session},
    view::{
        self, Tone, format,
        icon::Icon,
        report::{Crumb, Problem, Report, Section, Status, read_within, respond},
    },
};
#[cfg(test)]
use crate::installation::credentials::{CredentialError, Locked, Secret};
use analyte_config::Destination;
#[cfg(test)]
use analyte_model::Identifier;
use analyte_store::{FailingDestination, Health, LastDelivered};
use askama::Template as _;
use axum::{
    Extension, Form,
    extract::{Path, State},
    http::StatusCode,
    response::Response,
};
#[cfg(test)]
use change::new_name;
use jiff::Timestamp;
use page::{Card, Entry, Key, LabApp, List, Name, Orders, Sender, Todo};
use serde::Deserialize;
use std::sync::Arc;

/// Where the page lives; the overview links here too.
pub(in crate::admin) const PAGE: &str = "/setup/lab-apps";

/// The machines whose settings send to a lab app, each linked to its page.
fn senders(users: &[String]) -> Vec<Sender> {
    users
        .iter()
        .map(|id| Sender {
            id: id.clone(),
            href: format::machine_href(id),
        })
        .collect()
}

/// How Analyte runs a lab app now, as startup set it up.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Runs {
    /// A running machine sends to it, so its order list is polled too.
    Polled,
    /// No running machine uses it: a delivery-only worker sends what waits.
    Kept,
    /// Added or changed since Analyte started; it runs after a restart.
    No,
}

/// What the store says about one lab app and how Analyte runs it.
struct Read {
    health: Health,
    /// For a kept lab app whose keys were missing at startup: which ones the
    /// key source lacks now.
    keys: Option<overview::Keys>,
    runs: Runs,
}

/// What the list reads: the settings and keys file, then the store.
struct ListRead {
    settings: Result<(Scan, KeySource), SetupError>,
    delivered: LastDelivered,
    failing: Vec<FailingDestination>,
    /// One per listed lab app, in the list's order.
    reads: Vec<Read>,
    /// Results waiting for lab apps no running machine uses, or why they could not be counted.
    removed: Result<Vec<removed::Line>, String>,
}

async fn read_list(state: &Arc<Context>, now: Timestamp) -> Result<ListRead, crate::admin::Error> {
    let settings = off_thread(state, |state| {
        let scan = scan(state.setup.config())?;
        Ok((scan, KeySource::read(&state.setup)))
    })
    .await;
    let (delivered, failing) = state
        .store
        .read::<_, crate::admin::error::CommandError>(crate::deadline()?, |store| {
            Ok((store.last_delivered()?, store.failing_destinations()?))
        })
        .await?;
    let mut reads = Vec::new();
    for known in settings.iter().flat_map(|(scan, _)| &scan.known) {
        let name = &known.settings.name;
        let serving = state
            .destinations
            .iter()
            .find(|destination| destination.id() == name);
        reads.push(match serving {
            Some(destination) => Read {
                health: destination.health(&state.store, now.as_second()).await?,
                keys: None,
                runs: Runs::Polled,
            },
            None => {
                let kept = state.retired.get(name.as_str());
                let (health, keys) = overview::kept_health(state, name, kept, now).await?;
                Read {
                    health,
                    keys,
                    runs: if kept.is_some() { Runs::Kept } else { Runs::No },
                }
            }
        });
    }
    let removed = removed::lines(state)
        .await
        .map_err(|error| error.to_string());
    Ok(ListRead {
        settings,
        delivered,
        failing,
        reads,
        removed,
    })
}

/// GET /setup/lab-apps: every lab app, how it stands, who uses it, its keys and what waits for it.
pub(in crate::admin) async fn list(
    State(state): State<Arc<Context>>,
    session: Option<Extension<Arc<Session>>>,
) -> Response {
    let section = Section::LabApps(None);
    let Some(Extension(session)) = session.filter(|Extension(s)| s.is_signed_in()) else {
        return Problem::SignIn.page(&state, None, section);
    };
    let now = Timestamp::now();
    let read = match read_within(read_list(&state, now)).await {
        Ok(read) => read,
        Err(problem) => return problem.page(&state, Some(&session), section),
    };
    let (status, page) = list_page(&state, &session, read, now);
    respond(&state, Some(&session), section, status, page.render())
}

fn list_page(
    state: &Context,
    session: &Session,
    read: ListRead,
    now: Timestamp,
) -> (StatusCode, List) {
    let frame = Section::LabApps(None).frame(
        state,
        crate::admin::view::Tab::LabApps.label(),
        Some(session),
    );
    let ListRead {
        settings,
        delivered,
        failing,
        reads,
        removed,
    } = read;
    let (scan, keys) = match settings {
        Ok(settings) => settings,
        Err(error) => {
            let page = List {
                frame,
                problems: vec![Notice {
                    tone: Tone::Problem,
                    title: crate::admin::setup::LAB_APPS_UNREADABLE.into(),
                    text: format::sentence(&error.to_string()),
                }],
                cards: Vec::new(),
                add: false,
                notices: Vec::new(),
                support: Vec::new(),
            };
            return (StatusCode::SERVICE_UNAVAILABLE, page);
        }
    };
    let mut problems: Vec<Notice> = scan
        .conflicts
        .iter()
        .map(|conflict| Notice {
            tone: Tone::Problem,
            title: conflict_sentence(conflict),
            text: String::new(),
        })
        .collect();
    let mut support: Vec<String> = keys.support().into_iter().collect();
    let mut notices = Vec::new();
    match removed {
        // A lab app the settings still write has its own card.
        Ok(lines) => notices.extend(
            lines
                .iter()
                .filter(|line| scan.get(line.destination().as_str()).is_none())
                .map(|line| Notice {
                    tone: line.tone(),
                    title: line.text().to_owned(),
                    text: String::new(),
                }),
        ),
        Err(error) => {
            problems.push(Notice {
                tone: Tone::Problem,
                title: crate::admin::removed::COUNT_FAILED.into(),
                text: String::new(),
            });
            support.push(error);
        }
    }
    let cards = scan
        .known
        .iter()
        .zip(reads)
        .map(|(known, read)| {
            let (card, more) = card(known, read, (&delivered, &failing), &keys, now);
            support.extend(more);
            card
        })
        .collect();
    let page = List {
        frame,
        problems,
        cards,
        add: true,
        notices,
        support,
    };
    (StatusCode::OK, page)
}

/// One lab app's card, by the Overview's lab app rules, and what only support needs.
fn card(
    known: &Known,
    read: Read,
    (delivered, failing): (&LastDelivered, &[FailingDestination]),
    source: &KeySource,
    now: Timestamp,
) -> (Card, Vec<String>) {
    let settings = &known.settings;
    let name = settings.name.as_str();
    let Standing {
        delivery,
        keys: startup,
        failing,
        delivered,
        waiting,
    } = overview::standing(
        &settings.name,
        &read.health,
        delivered,
        failing,
        read.keys,
        now,
    );
    let runs = read.runs != Runs::No;
    let (tone, word) = if runs {
        (delivery.tone(), delivery.word())
    } else {
        (Tone::Attention, "Starts after a restart")
    };
    let keys = source.keys(settings, now);
    let missing = keys.iter().any(|key| matches!(key.entry, Entry::Needed(_)));
    let href = format::lab_app_href(name);
    let todo = if missing {
        Some(Todo {
            text: format::KEYS_ADVICE,
            href: href.clone(),
            label: crate::admin::setup::ADD_KEYS,
        })
    } else if runs && delivery == Delivery::NotDelivering {
        Some(Todo {
            text: format::FAILING_ADVICE,
            href: view::Tab::Deliveries.href().to_owned(),
            label: crate::admin::view::Tab::Deliveries.label(),
        })
    } else {
        None
    };
    let (note, support) = startup.map_or((None, None), |keys| (Some(keys.line), keys.support));
    let card = Card {
        name: name.to_owned(),
        remove: format!("{href}/remove"),
        href,
        status: Status {
            tone,
            icon: tone.icon(),
            word: word.into(),
            exact: String::new(),
            note: None,
        },
        bad: tone == Tone::Problem,
        delivered,
        address: settings.base_url.as_str().to_owned(),
        senders: senders(&known.users),
        keys,
        waiting,
        failing: failing.filter(|_| runs),
        orders: (read.runs == Runs::Polled).then(|| {
            let (_, word, checked) = overview::order_list(&read.health);
            Orders {
                word,
                checked: checked.map(|at| format::moment(at, now)),
            }
        }),
        written: if known.in_file {
            Vec::new()
        } else {
            where_written(known)
        },
        // The missing keys are already on the card with what to do.
        note: note.filter(|_| !missing),
        todo,
    };
    (card, support.into_iter().collect())
}

/// The form for a new lab app (`existing` none) or for one that exists, with
/// the Remove dialog over it when asked.
fn form_page(
    state: &Context,
    session: &Session,
    existing: Option<(&Known, &KeySource)>,
    dialog: Option<RemoveDialog>,
) -> LabApp {
    let now = Timestamp::now();
    let (existing, source) = existing.unzip();
    let settings = existing.map(|known| &known.settings);
    let title = settings.map_or_else(
        || crate::admin::setup::ADD_LAB_APP.to_owned(),
        |settings| format!("Edit {}", settings.name.as_str()),
    );
    let mut head = Section::LabApps(None).head(title.clone());
    head.crumbs
        .extend(settings.map(|settings| Crumb::here(settings.name.as_str())));
    let new = |(label, field, input)| Key {
        label,
        field,
        input,
        tone: Tone::Neutral,
        word: String::new(),
        exact: String::new(),
        entry: Entry::New,
    };
    LabApp {
        frame: Section::LabApps(None).frame(state, title, Some(session)),
        head,
        action: settings.map_or_else(
            || format!("{PAGE}/new"),
            |settings| format::lab_app_href(settings.name.as_str()),
        ),
        csrf: session.csrf.form_value().to_owned(),
        name: settings.map_or_else(
            || Name::New("lab-app".into()),
            |settings| Name::Fixed(settings.name.as_str().to_owned()),
        ),
        address: settings.map_or_else(String::new, |settings| {
            settings.base_url.as_str().to_owned()
        }),
        freshness: settings
            .map_or(Destination::DEFAULT_FRESHNESS_SECONDS, |settings| {
                settings.freshness_seconds
            })
            .to_string(),
        timeout: settings
            .map_or(Destination::DEFAULT_REQUEST_TIMEOUT_SECONDS, |settings| {
                settings.request_timeout_seconds
            })
            .to_string(),
        defaults: format!(
            "The defaults suit most labs: {} and {} seconds.",
            Destination::DEFAULT_FRESHNESS_SECONDS,
            Destination::DEFAULT_REQUEST_TIMEOUT_SECONDS
        ),
        senders: existing.map_or_else(Vec::new, |known| senders(&known.users)),
        kept: existing
            .filter(|known| !known.sites.is_empty())
            .map(|known| known.sites.join(", ")),
        keys: match (settings, source) {
            (Some(settings), Some(source)) => source.keys(settings, now),
            (Some(_) | None, _) => vec![new(API_KEY), new(SIGNING_KEY)],
        },
        remove: settings
            .map(|settings| format!("{}/remove", format::lab_app_href(settings.name.as_str()))),
        support: source.and_then(KeySource::support).into_iter().collect(),
        dialog,
    }
}

/// GET /setup/lab-apps/new: the form for one more lab app.
pub(in crate::admin) async fn add_form(
    State(state): State<Arc<Context>>,
    session: Option<Extension<Arc<Session>>>,
) -> Response {
    let section = Section::LabApps(None);
    let Some(Extension(session)) = session.filter(|Extension(s)| s.is_signed_in()) else {
        return Problem::SignIn.page(&state, None, section);
    };
    let page = form_page(&state, &session, None, None);
    respond(
        &state,
        Some(&session),
        section,
        StatusCode::OK,
        page.render(),
    )
}

/// GET /setup/lab-apps/{name}: the edit form for one lab app.
pub(in crate::admin) async fn edit_form(
    State(state): State<Arc<Context>>,
    session: Option<Extension<Arc<Session>>>,
    Path(name): Path<String>,
) -> Response {
    edit(state, session, name, false).await
}

/// GET /setup/lab-apps/{name}/remove: the edit form with the Remove dialog
/// over it, saying what keeps the lab app or that nothing does.
pub(in crate::admin) async fn confirm_remove(
    State(state): State<Arc<Context>>,
    session: Option<Extension<Arc<Session>>>,
    Path(name): Path<String>,
) -> Response {
    edit(state, session, name, true).await
}

/// The edit form for lab app `name`, with the Remove dialog when `removing`.
async fn edit(
    state: Arc<Context>,
    session: Option<Extension<Arc<Session>>>,
    name: String,
    removing: bool,
) -> Response {
    let Some(Extension(session)) = session.filter(|Extension(s)| s.is_signed_in()) else {
        return Problem::SignIn.page(&state, None, Section::LabApps(None));
    };
    let open = if removing {
        Some(waiting(&state.store).await)
    } else {
        None
    };
    let read = off_thread(&state, |state| {
        let scan = scan(state.setup.config())?;
        Ok((scan, KeySource::read(&state.setup)))
    })
    .await;
    let (scan, source) = match read {
        Ok(read) => read,
        Err(error) => {
            return Problem::Settings(error.to_string()).page(
                &state,
                Some(&session),
                Section::LabApps(None),
            );
        }
    };
    let Some(known) = scan.get(&name) else {
        return Problem::Refused(SetupError::UnknownLabApp(name).to_string()).page(
            &state,
            Some(&session),
            Section::LabApps(None),
        );
    };
    let dialog = open.map(|open| {
        dialog(
            session.csrf.form_value(),
            known,
            source.kept(&known.settings),
            open.as_ref().map(|open| count(open, &name)),
        )
    });
    let page = form_page(&state, &session, Some((known, &source)), dialog);
    respond(
        &state,
        Some(&session),
        Section::LabApps(Some(&name)),
        StatusCode::OK,
        page.render(),
    )
}

/// The Remove dialog: what removing keeps, and what stops it now or that
/// nothing does. It is a real form with the session's form token.
fn dialog(
    csrf: &str,
    known: &Known,
    keys: &'static str,
    waiting: Result<u64, &SetupError>,
) -> RemoveDialog {
    let name = known.settings.name.as_str();
    let href = format::lab_app_href(name);
    let why = blockers(known, waiting);
    let blocked = !why.is_empty();
    let items = if blocked {
        let counted = waiting.is_ok();
        why.into_iter()
            .map(|text| DialogItem {
                tone: if counted {
                    Tone::Attention
                } else {
                    Tone::Problem
                },
                icon: if counted {
                    Icon::Warning
                } else {
                    Icon::Problem
                },
                text,
            })
            .collect()
    } else {
        vec![
            DialogItem {
                tone: Tone::Good,
                icon: Icon::Ok,
                text: "No machine uses it and nothing waits for it.".into(),
            },
            DialogItem {
                tone: Tone::Neutral,
                icon: Icon::Ok,
                text: crate::admin::setup::BACKUP_ADVICE.into(),
            },
        ]
    };
    RemoveDialog {
        action: format!("{href}/remove"),
        csrf: csrf.to_owned(),
        cancel: href,
        title: format!("Remove {name}?"),
        text: "Results already sent and their history stay.",
        sending: Hover {
            shown: keys.into(),
            exact: String::new(),
        },
        list_title: if blocked {
            "It cannot be removed now"
        } else {
            crate::admin::setup::BEFORE_REMOVAL
        },
        items,
        anyway: false,
        button: (!blocked).then_some(crate::admin::setup::REMOVE),
    }
}

/// The add or edit form as posted. Keys are moved into [`Secret`] at once.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::admin) struct Posted {
    #[expect(dead_code, reason = "the CSRF middleware has already checked it")]
    csrf: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    base_url: String,
    #[serde(default)]
    freshness_seconds: String,
    #[serde(default)]
    request_timeout_seconds: String,
    #[serde(default)]
    api_key: String,
    #[serde(default)]
    signing_key: String,
}

impl Posted {
    /// The entered values; the CSRF value stays behind.
    fn entered(self) -> Entered {
        Entered {
            name: self.name,
            base_url: self.base_url,
            freshness_seconds: self.freshness_seconds,
            request_timeout_seconds: self.request_timeout_seconds,
            api_key: self.api_key,
            signing_key: self.signing_key,
        }
    }
}

/// The page that says what a save wrote: no key value, only which keys were
/// stored, with Restart now; an audit that could not be written shows.
fn saved_page(
    state: &Context,
    session: &Session,
    change: Change,
    saved: &Saved,
    audited: Result<(), SetupError>,
) -> Response {
    let name = saved.settings.name.as_str();
    let section = Section::LabApps(Some(name));
    let title = match change {
        Change::Added => "Lab app added",
        Change::Saved => "Lab app saved",
    };
    let mut lines = saved.details("Used by no machine yet: pick it on a machine’s setup page.");
    if !saved.backups.is_empty() {
        lines.push(crate::admin::setup::BACKUP_KEPT.into());
    }
    let page = Report {
        lines,
        problems: audited
            .err()
            .map(|error| format::sentence(&error.to_string()))
            .into_iter()
            .collect(),
        restart: Some(session.csrf.form_value().to_owned()),
        support: copies(&saved.backups),
        ..Report::new(
            section.frame(state, title, Some(session)),
            section,
            Tone::Good,
            format!("{name} is {}", change.verb()),
            RESTART_REQUIRED,
        )
    };
    respond(state, Some(session), section, StatusCode::OK, page.render())
}

/// A refused lab app form: nothing was written; the reason in words.
fn refused(state: &Context, session: &Session, name: Option<&str>, error: &SetupError) -> Response {
    // A lab app that does not exist has no page to go back to.
    let name = name.filter(|_| !matches!(error, SetupError::UnknownLabApp(_)));
    Problem::Refused(error.to_string()).page(state, Some(session), Section::LabApps(name))
}

/// POST /setup/lab-apps/new: check, write `lab-apps.toml`, store the keys.
pub(in crate::admin) async fn add(
    State(state): State<Arc<Context>>,
    Extension(session): Extension<Arc<Session>>,
    form: Result<Form<Posted>, axum::extract::rejection::FormRejection>,
) -> Response {
    let Ok(Form(posted)) = form else {
        return refused(&state, &session, None, &SetupError::IncompleteForm(RELOAD));
    };
    let saved = off_thread(&state.setup, move |setup| {
        add_blocking(setup, posted.entered())
    })
    .await;
    finish(&state, &session, Change::Added, None, saved).await
}

/// POST /setup/lab-apps/{name}: check, write `lab-apps.toml`, store any new keys.
pub(in crate::admin) async fn save(
    State(state): State<Arc<Context>>,
    Extension(session): Extension<Arc<Session>>,
    Path(name): Path<String>,
    form: Result<Form<Posted>, axum::extract::rejection::FormRejection>,
) -> Response {
    let Ok(Form(posted)) = form else {
        return refused(
            &state,
            &session,
            Some(&name),
            &SetupError::IncompleteForm(RELOAD),
        );
    };
    let lab_app = name.clone();
    let saved = off_thread(&state.setup, move |setup| {
        save_blocking(setup, &lab_app, posted.entered())
    })
    .await;
    finish(&state, &session, Change::Saved, Some(&name), saved).await
}

/// Audits a written lab app and shows what was saved, or why nothing was.
async fn finish(
    state: &Context,
    session: &Session,
    change: Change,
    name: Option<&str>,
    saved: Result<Saved, SetupError>,
) -> Response {
    let saved = match saved {
        Ok(saved) => saved,
        Err(error) => return refused(state, session, name, &error),
    };
    let name = saved.settings.name.as_str();
    let audited = audit(
        state,
        session,
        change.action(),
        name,
        reason(Via::Page, change, &saved),
    )
    .await;
    tracing::info!(lab_app = name, "lab app page {} a lab app", change.verb());
    saved_page(state, session, change, &saved, audited)
}

/// POST /setup/lab-apps/{name}/remove: refused while a machine uses it or results wait.
pub(in crate::admin) async fn remove(
    State(state): State<Arc<Context>>,
    Extension(session): Extension<Arc<Session>>,
    Path(name): Path<String>,
    form: Result<Form<Csrf>, axum::extract::rejection::FormRejection>,
) -> Response {
    if form.is_err() {
        return refused(
            &state,
            &session,
            Some(&name),
            &SetupError::IncompleteForm(RELOAD),
        );
    }
    let open = waiting(&state.store).await.map(|open| count(&open, &name));
    let lab_app = name.clone();
    let removed = off_thread(&state.setup, move |setup| {
        remove_blocking(setup, &lab_app, &open)
    })
    .await;
    let (backups, keys) = match removed {
        Ok(removed) => removed,
        Err(error) => return refused(&state, &session, Some(&name), &error),
    };
    let audited = audit(
        &state,
        &session,
        "lab-app-remove",
        &name,
        removed_reason(Via::Page, &name),
    )
    .await;
    tracing::info!(lab_app = name.as_str(), "lab app page removed a lab app");
    let section = Section::LabApps(None);
    let title = "Lab app removed";
    let page = Report {
        lines: if backups.is_empty() {
            Vec::new()
        } else {
            vec![crate::admin::setup::BACKUP_KEPT.into()]
        },
        problems: audited
            .err()
            .map(|error| format::sentence(&error.to_string()))
            .into_iter()
            .collect(),
        restart: Some(session.csrf.form_value().to_owned()),
        support: copies(&backups),
        ..Report::new(
            section.frame(&state, title, Some(&session)),
            section,
            Tone::Good,
            format!("{name} is no longer listed"),
            keys,
        )
    };
    respond(
        &state,
        Some(&session),
        section,
        StatusCode::OK,
        page.render(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lab app `name` as the settings write it, used by `users`.
    fn known(name: &str, users: &[&str]) -> Result<Known, Box<dyn std::error::Error>> {
        Ok(Known {
            settings: toml::from_str(&format!(
                "name = '{name}'
base_url = 'https://{name}.example.invalid'
api_key_name = '{name}-api'
signing_key_name = '{name}-signing'
"
            ))?,
            in_file: true,
            kept: false,
            sites: Vec::new(),
            users: users.iter().map(|&user| user.to_owned()).collect(),
        })
    }
    // Owner request 2026-11-19 (lab app set once): old names may hold any
    // character, so links carry them percent-encoded; new names are plain.
    #[test]
    fn names_are_plain_and_links_encoded() {
        let href = format::lab_app_href;
        assert_eq!(href("lab app/ü"), "/setup/lab-apps/lab%20app%2F%C3%BC");
        assert_eq!(href("lab-app_2"), "/setup/lab-apps/lab-app_2");
        for good in ["lab-app", "main_LIS", "A1"] {
            assert!(new_name(good).is_ok(), "{good}");
        }
        for bad in ["", "two words", "../up", "a.b", &"x".repeat(65)] {
            assert!(new_name(bad).is_err(), "{bad}");
        }
    }

    // Owner decisions 2026-11-20 (admin restyle b3): a key shows only where
    // the service finds it, by its own rule: the environment wins, then the
    // locked file with the day it was stored; missing, or not checkable when
    // the file cannot be read, with why the field is needed. Never the value.
    #[test]
    fn keys_say_where_the_service_finds_them_never_their_value()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let stored = Identifier::new("main-lab-signing")?;
        Locked::new(directory.path()).replace(
            vec![(
                stored.clone(),
                Secret::new(&stored, "stored-value-7Q".into())?,
            )],
            1_795_046_077,
        )?;
        let source = KeySource {
            environment: |variable| {
                (variable == "ANALYTE_SECRET_MAIN_LAB_API")
                    .then(|| "environment-value-9Z".to_owned())
            },
            stored: Locked::new(directory.path()).read(),
        };
        let now = Timestamp::from_second(1_795_046_077)?;
        let lab = known("main-lab", &[])?;
        let [api, signing] =
            <[Key; 2]>::try_from(source.keys(&lab.settings, now)).map_err(|_| "two keys")?;
        assert_eq!(
            api.word,
            "Set in the service environment (ANALYTE_SECRET_MAIN_LAB_API)"
        );
        assert_eq!((api.tone, api.exact.as_str()), (Tone::Good, "main-lab-api"));
        assert!(matches!(api.entry, Entry::Replace { hint } if hint.contains("wins")));
        assert!(signing.word.starts_with("Set on "), "{}", signing.word);
        assert!(
            signing
                .exact
                .starts_with("main-lab-signing, stored 2026-11-18T")
        );
        assert!(
            matches!(signing.entry, Entry::Replace { hint } if hint.contains("keep the stored key"))
        );
        let other = known("other-lab", &[])?;
        let missing = source.keys(&other.settings, now);
        assert!(
            missing
                .iter()
                .all(|key| key.tone == Tone::Problem && key.word == "Missing")
        );
        assert!(
            missing
                .iter()
                .all(|key| matches!(key.entry, Entry::Needed(why) if why == format::KEY_NEEDED))
        );
        let unreadable = KeySource {
            environment: |_| None,
            stored: Err(CredentialError::Exposed("credentials.toml".into())),
        };
        let unchecked = unreadable.key(API_KEY, &other.settings.api_key_name, now);
        assert_eq!(
            (unchecked.tone, unchecked.word.as_str()),
            (Tone::Attention, "Cannot be checked")
        );
        assert!(matches!(unchecked.entry, Entry::Needed(why) if why == format::KEY_UNCHECKED));
        // What removing does to the keys, from what the locked file holds.
        assert_eq!(source.kept(&lab.settings), "Its keys stay stored, locked.");
        assert_eq!(
            source.kept(&other.settings),
            "No key is stored for it, so none is kept."
        );
        assert!(
            unreadable
                .kept(&other.settings)
                .starts_with("Whether keys are stored")
        );
        assert!(
            unreadable
                .support()
                .is_some_and(|line| line.starts_with("Keys file: "))
        );
        for key in source.keys(&lab.settings, now).iter().chain(&missing) {
            let shown = format!("{} {} {}", key.word, key.exact, key.label);
            assert!(!shown.contains("value"), "{shown}");
        }
        Ok(())
    }

    // Owner request 2026-11-19 (lab app set once) and owner decisions
    // 2026-11-20 (admin restyle b3): the Remove dialog says why a lab app is
    // kept (machines use it, items wait, or they could not be counted) and
    // then offers no Remove; otherwise it says nothing keeps it.
    #[test]
    fn remove_dialog_says_what_keeps_a_lab_app() -> Result<(), Box<dyn std::error::Error>> {
        let used = dialog("token", &known("main-lab", &["chem-1"])?, "kept", Ok(2));
        assert_eq!(used.button, None);
        assert_eq!(used.list_title, "It cannot be removed now");
        let texts: Vec<&str> = used.items.iter().map(|item| item.text.as_str()).collect();
        assert_eq!(
            texts,
            [
                "Machines send results to it: chem-1. Pick another lab app on their setup pages, or remove them, first.",
                "2 results or order messages still wait for it. They are delivered first; remove it when the Overview shows nothing waiting.",
            ]
        );
        assert!(used.items.iter().all(|item| item.tone == Tone::Attention));
        let uncounted = dialog(
            "token",
            &known("main-lab", &[])?,
            "kept",
            Err(&SetupError::Busy),
        );
        assert_eq!(uncounted.button, None);
        assert!(
            uncounted
                .items
                .iter()
                .all(|item| item.tone == Tone::Problem)
        );
        assert!(uncounted.items.iter().any(|item| {
            item.text
                .starts_with("What still waits for it could not be counted")
        }));
        let free = dialog("token", &known("spare", &[])?, "kept", Ok(0));
        assert_eq!(free.button, Some("Remove"));
        assert_eq!(free.cancel, "/setup/lab-apps/spare");
        assert_eq!(free.action, "/setup/lab-apps/spare/remove");
        assert!(!free.anyway);
        assert_eq!(
            free.items
                .first()
                .map(|item| (item.tone, item.text.as_str())),
            Some((Tone::Good, "No machine uses it and nothing waits for it."))
        );
        Ok(())
    }
}
