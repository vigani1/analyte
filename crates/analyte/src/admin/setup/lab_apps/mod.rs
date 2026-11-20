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
mod page;

use super::{Csrf, RELOAD, SetupError, audit, copies, entries, off_thread, site, stored};
use crate::admin::machines::{DialogItem, Hover, RemoveDialog};
use crate::admin::overview::{self, Delivery, Standing};
use crate::admin::removed;
use crate::admin::{
    server::{Context, sessions::Session},
    view::{
        self, Tone, format,
        icon::Icon,
        report::{Crumb, Problem, Report, Section, Status, respond},
    },
};
use crate::installation::{
    config::{
        folder,
        lab_apps::{self as file, Conflict, Written},
        plain_name, read,
    },
    credentials::{CredentialError, Locked, Secret, Stored},
    secrets,
};
use analyte_config::{Destination, HttpsUrl, LabApps};
use analyte_model::Identifier;
use analyte_store::{FailingDestination, Health, LastDelivered};
use askama::Template as _;
use axum::{
    Extension, Form,
    extract::{Path, State},
    http::StatusCode,
    response::Response,
};
use jiff::Timestamp;
use page::{Card, Entry, Key, LabApp, List, Name, Notice, Orders, Sender, Todo};
use serde::Deserialize;
use std::{path::PathBuf, sync::Arc};

/// Where the page lives; the overview links here too.
pub(in crate::admin) const PAGE: &str = "/setup/lab-apps";

/// One lab app as the installation writes it today, and who uses it.
pub(super) struct Known {
    pub(super) settings: Destination,
    /// Listed in `lab-apps.toml`.
    in_file: bool,
    /// Kept in the program file by an older setup page.
    kept: bool,
    /// Machines whose site file writes it as its own `[destination]`.
    sites: Vec<String>,
    /// Machines that send to it: by name, by their own table, or as the only one.
    pub(super) users: Vec<String>,
}

/// Every lab app of the installation, read leniently for display; the checked
/// commit after any write is what decides.
pub(super) struct Scan {
    program: String,
    pub(super) file: LabApps,
    pub(super) known: Vec<Known>,
    /// Where each name was first written, by the loader's rule.
    written: Written<Place>,
    /// Every name written differently in two places.
    conflicts: Vec<Conflict<Place>>,
}

/// Where the page found a lab app written.
#[derive(Clone)]
enum Place {
    File,
    Kept,
    /// The site file of this machine.
    Site(String),
}

/// The page's sentence for a name written differently in two places; it names
/// the place read later.
fn conflict_sentence(conflict: &Conflict<Place>) -> String {
    format!(
        "Lab app {} has different settings in two places (one is {}). Analyte cannot start until they are the same.",
        conflict.name.as_str(),
        match &conflict.second {
            Place::File => "this page".into(),
            Place::Kept => "kept from a removed machine".into(),
            Place::Site(id) => format!("machine {id}'s own settings"),
        }
    )
}

impl Scan {
    fn note(&mut self, settings: &Destination, place: Place) {
        match self.written.note(settings, place.clone()) {
            Ok(true) => self.known.push(Known {
                settings: settings.clone(),
                in_file: false,
                kept: false,
                sites: Vec::new(),
                users: Vec::new(),
            }),
            Ok(false) => {}
            Err(conflict) => self.conflicts.push(conflict),
        }
        let Some(known) = self
            .known
            .iter_mut()
            .find(|known| known.settings.name == settings.name)
        else {
            return;
        };
        match place {
            Place::File => known.in_file = true,
            Place::Kept => known.kept = true,
            Place::Site(id) => known.sites.push(id),
        }
    }

    fn uses(&mut self, name: &str, machine: &str) {
        if let Some(known) = self
            .known
            .iter_mut()
            .find(|known| known.settings.name.as_str() == name)
        {
            known.users.push(machine.to_owned());
        }
    }

    /// The settings of every lab app listed, wherever written.
    pub(super) fn settings(&self) -> Vec<Destination> {
        self.known
            .iter()
            .map(|known| known.settings.clone())
            .collect()
    }

    /// The lab app called `name`.
    pub(super) fn get(&self, name: &str) -> Option<&Known> {
        self.known
            .iter()
            .find(|known| known.settings.name.as_str() == name)
    }
}

/// The lab app destinations an older setup page kept in the program file.
fn kept(program: &str) -> Result<Vec<Destination>, SetupError> {
    let table: toml::Table = program.parse().map_err(SetupError::ProgramFile)?;
    match table.get("retired_destinations") {
        Some(listed) => listed.clone().try_into().map_err(SetupError::ProgramFile),
        None => Ok(Vec::new()),
    }
}

/// The lab app a site file's settings choose, read leniently for display: its
/// own complete `[destination]`, else the one `lab_app` names, else none
/// named (then the only lab app listed, if there is exactly one). The loader
/// refuses what this lets through, such as both written.
pub(super) enum SiteLabApp<'a> {
    Own(Destination),
    Named(&'a str),
    Unnamed,
}

/// Reads which lab app the site file settings `site` choose.
pub(super) fn site_lab_app(site: &toml::Table) -> SiteLabApp<'_> {
    let own = site
        .get("destination")
        .cloned()
        .and_then(|own| own.try_into::<Destination>().ok());
    match (own, site.get("lab_app").and_then(toml::Value::as_str)) {
        (Some(own), _) => SiteLabApp::Own(own),
        (None, Some(name)) => SiteLabApp::Named(name),
        (None, None) => SiteLabApp::Unnamed,
    }
}

/// Reads the program file, `lab-apps.toml` and each machine's site file.
pub(super) fn scan(config: &std::path::Path) -> Result<Scan, SetupError> {
    let (program, listed) = site::entries(config)?;
    let lab_apps = file::read(config)?;
    let kept = kept(&program)?;
    let mut scan = Scan {
        program,
        file: lab_apps.clone(),
        known: Vec::new(),
        written: Written::new(),
        conflicts: Vec::new(),
    };
    for settings in lab_apps.listed() {
        scan.note(settings, Place::File);
    }
    for settings in &kept {
        scan.note(settings, Place::Kept);
    }
    let base = folder(config);
    let mut by_default = Vec::new();
    for entry in &listed {
        let id = entry.id.get_ref();
        let Some(table) = read(&base.join(entry.site.get_ref()))
            .ok()
            .and_then(|text| text.parse::<toml::Table>().ok())
        else {
            continue;
        };
        match site_lab_app(&table) {
            SiteLabApp::Own(own) => {
                scan.note(&own, Place::Site(id.clone()));
                scan.uses(own.name.as_str(), id);
            }
            SiteLabApp::Named(name) => scan.uses(name, id),
            SiteLabApp::Unnamed if entry.definition.is_some() => by_default.push(id.clone()),
            SiteLabApp::Unnamed => {}
        }
    }
    if let [only] = lab_apps.listed() {
        for id in by_default {
            scan.uses(only.name.as_str(), &id);
        }
    }
    Ok(scan)
}

/// Checks a new lab app's name: it also names its keys and appears in links, so
/// it uses only ASCII letters, digits, `-` and `_`, at most 64 characters.
fn new_name(text: &str) -> Result<Identifier, SetupError> {
    let text = text.trim();
    if text.is_empty() || text.len() > 64 || !plain_name(text) {
        return Err(SetupError::LabAppName);
    }
    Identifier::new(text).map_err(|_| SetupError::LabAppName)
}

fn seconds(text: &str, default: u64, what: &'static str) -> Result<u64, SetupError> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(default);
    }
    text.parse().map_err(|_| SetupError::InvalidNumber(what))
}

/// Results and order messages the lab app has not accepted yet, by lab app name.
async fn waiting(state: &Context) -> Result<Vec<(String, u64)>, SetupError> {
    let open = stored(state, analyte_store::Store::undelivered_by_destination)
        .await
        .map_err(SetupError::Store)?;
    Ok(open
        .into_iter()
        .map(|open| (open.destination.as_str().to_owned(), open.waiting()))
        .collect())
}

fn count(waiting: &[(String, u64)], name: &str) -> u64 {
    waiting
        .iter()
        .find(|(destination, _)| destination == name)
        .map_or(0, |(_, count)| *count)
}

/// Where the settings still write a lab app that is not in `lab-apps.toml`,
/// in words, with the exact place on hover.
fn where_written(known: &Known) -> Vec<(String, &'static str)> {
    let mut places = Vec::new();
    if !known.sites.is_empty() {
        places.push((
            format!("Still kept in the settings of {}", known.sites.join(", ")),
            "[destination] in the machine's site file; saving the machine moves it to lab-apps.toml",
        ));
    }
    if known.kept {
        places.push((
            "Kept from a removed machine".to_owned(),
            "[[retired_destinations]] in the program file; saving it here moves it to lab-apps.toml",
        ));
    }
    places
}

/// The two keys of a lab app: their label, form field and field id.
const API_KEY: (&str, &str, &str) = ("API key", "api_key", "la-api");
const SIGNING_KEY: (&str, &str, &str) = ("Signing key", "signing_key", "la-signing");

/// Where a page finds keys, by the service's own rule (`secrets::source`):
/// the service environment, then the locked keys file, read once per page.
struct KeySource {
    environment: fn(&str) -> Option<String>,
    stored: Result<Stored, CredentialError>,
}

impl KeySource {
    /// This service's environment and locked keys file.
    fn read(state: &Context) -> Self {
        Self {
            environment: |variable| std::env::var(variable).ok(),
            stored: Locked::new(&state.setup.data_directory).read(),
        }
    }

    /// Why the locked keys file could not be read, for support.
    fn support(&self) -> Option<String> {
        self.stored
            .as_ref()
            .err()
            .map(|error| format!("Keys file: {error}"))
    }

    /// One key's state, never its value. An unreadable file still lets a set
    /// environment variable show, as it wins at startup too.
    fn key(
        &self,
        (label, field, input): (&'static str, &'static str, &'static str),
        name: &Identifier,
        now: Timestamp,
    ) -> Key {
        let none = Stored::default();
        let file = self.stored.as_ref().unwrap_or(&none);
        let named = name.as_str().to_owned();
        let (tone, word, exact, entry) =
            match secrets::source(name.as_str(), self.environment, file) {
                secrets::Source::Environment(_) => (
                    Tone::Good,
                    format::set_in_environment(&secrets::variable(name.as_str())),
                    named,
                    Entry::Replace {
                        hint: "The service environment’s key wins over one stored here.",
                    },
                ),
                secrets::Source::Stored { set_at, .. } => (
                    Tone::Good,
                    format::set_on(set_at, now),
                    format!("{named}, stored {}", format::moment_unix(set_at, now).exact),
                    Entry::Replace {
                        hint: "Leave it empty to keep the stored key.",
                    },
                ),
                secrets::Source::Missing => match self.stored {
                    Ok(_) => (
                        Tone::Problem,
                        "Missing".into(),
                        named,
                        Entry::Needed(format::KEY_NEEDED),
                    ),
                    Err(_) => (
                        Tone::Attention,
                        "Cannot be checked".into(),
                        named,
                        Entry::Needed(format::KEY_UNCHECKED),
                    ),
                },
            };
        Key {
            label,
            field,
            input,
            tone,
            word,
            exact,
            entry,
        }
    }

    /// What removing a lab app does to its keys, from what the locked file holds.
    fn kept(&self, settings: &Destination) -> &'static str {
        match &self.stored {
            Ok(stored)
                if stored.value(settings.api_key_name.as_str()).is_some()
                    || stored.value(settings.signing_key_name.as_str()).is_some() =>
            {
                "Its keys stay stored, locked."
            }
            Ok(_) => "No key is stored for it, so none is kept.",
            Err(_) => "Whether keys are stored for it cannot be checked; any that are stay locked.",
        }
    }

    /// A lab app's two keys.
    fn keys(&self, settings: &Destination, now: Timestamp) -> Vec<Key> {
        vec![
            self.key(API_KEY, &settings.api_key_name, now),
            self.key(SIGNING_KEY, &settings.signing_key_name, now),
        ]
    }
}

/// One key on the Settings page: whose it is, which one and where it is
/// found, never its value.
pub(in crate::admin) struct Found {
    pub(in crate::admin) owner: String,
    pub(in crate::admin) label: &'static str,
    pub(in crate::admin) status: Status,
}

/// Where Analyte finds its keys, by the service's own rule and read once, off
/// the async threads: each lab app's two keys, then each of `alerts` (an alert
/// target and its key's name). Also why the lab apps could not be listed and
/// why the locked keys file could not be read, both for support.
pub(in crate::admin) async fn key_sources(
    state: &Arc<Context>,
    alerts: Vec<(Identifier, Identifier)>,
    now: Timestamp,
) -> Result<(Vec<Found>, Option<String>, Option<String>), SetupError> {
    off_thread(state, move |state| {
        let source = KeySource::read(state);
        let status = |name: &Identifier| {
            let key = source.key(("", "", ""), name, now);
            Status {
                tone: key.tone,
                icon: key.tone.icon(),
                word: key.word,
                exact: key.exact,
                note: None,
            }
        };
        let mut found = Vec::new();
        let listed = scan(&state.config_path).map(|scan| {
            for settings in scan.settings() {
                for (label, name) in [
                    (API_KEY.0, &settings.api_key_name),
                    (SIGNING_KEY.0, &settings.signing_key_name),
                ] {
                    found.push(Found {
                        owner: format!("Lab app {}", settings.name.as_str()),
                        label,
                        status: status(name),
                    });
                }
            }
        });
        for (target, name) in &alerts {
            found.push(Found {
                owner: format!("Alert target {}", target.as_str()),
                label: "Signing key",
                status: status(name),
            });
        }
        Ok((
            found,
            listed.err().map(|error| error.to_string()),
            source.support(),
        ))
    })
    .await
}

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
        let scan = scan(&state.config_path)?;
        Ok((scan, KeySource::read(state)))
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
    let Some(Extension(session)) = session.filter(|Extension(s)| s.authenticated) else {
        return Problem::SignIn.page(&state, None, section);
    };
    let now = Timestamp::now();
    let read = match tokio::time::timeout(view::READ_BUDGET, read_list(&state, now)).await {
        Ok(Ok(read)) => read,
        Ok(Err(error)) => {
            return Problem::of(&error).page(&state, Some(&session), section);
        }
        Err(_) => return Problem::Time.page(&state, Some(&session), section),
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
    let frame = Section::LabApps(None).frame(state, "Lab apps", Some(session));
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
                    title: "The lab apps file could not be read".into(),
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
                title: "Results still waiting from removed machines could not be counted".into(),
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
            label: "Add keys",
        })
    } else if runs && delivery == Delivery::NotDelivering {
        Some(Todo {
            text: format::FAILING_ADVICE,
            href: view::Tab::Deliveries.href().to_owned(),
            label: "Deliveries",
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
        || "Add a lab app".to_owned(),
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
    let Some(Extension(session)) = session.filter(|Extension(s)| s.authenticated) else {
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
    let Some(Extension(session)) = session.filter(|Extension(s)| s.authenticated) else {
        return Problem::SignIn.page(&state, None, Section::LabApps(None));
    };
    let open = if removing {
        Some(waiting(&state).await)
    } else {
        None
    };
    let read = off_thread(&state, |state| {
        let scan = scan(&state.config_path)?;
        Ok((scan, KeySource::read(state)))
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
                text: "Analyte keeps a dated copy of the old settings.".into(),
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
            "Before you remove it"
        },
        items,
        anyway: false,
        button: (!blocked).then_some("Remove"),
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

/// The posted settings for the lab app called `name` with key names `keys`.
fn settings(
    posted: &Posted,
    name: Identifier,
    keys: [Identifier; 2],
) -> Result<Destination, SetupError> {
    let [api_key_name, signing_key_name] = keys;
    Ok(Destination {
        freshness_seconds: seconds(
            &posted.freshness_seconds,
            Destination::DEFAULT_FRESHNESS_SECONDS,
            "the order list time",
        )?,
        request_timeout_seconds: seconds(
            &posted.request_timeout_seconds,
            Destination::DEFAULT_REQUEST_TIMEOUT_SECONDS,
            "the request wait",
        )?,
        name,
        base_url: HttpsUrl::parse(posted.base_url.trim()).map_err(SetupError::InvalidUrl)?,
        api_key_name,
        signing_key_name,
    })
}

/// Turns the posted keys into checked values; errors name only the key.
fn keys(
    settings: &Destination,
    posted: &mut Posted,
    required: bool,
) -> Result<Vec<(Identifier, Secret)>, SetupError> {
    let api = std::mem::take(&mut posted.api_key);
    let signing = std::mem::take(&mut posted.signing_key);
    if required && (api.is_empty() || signing.is_empty()) {
        return Err(SetupError::KeysRequired);
    }
    let mut updates = Vec::new();
    if !api.is_empty() {
        let name = settings.api_key_name.clone();
        if !analyte_contract::api_key_valid(&api) {
            return Err(SetupError::InvalidKey(name.as_str().into()));
        }
        let secret = Secret::new(&name, api)?;
        updates.push((name, secret));
    }
    if !signing.is_empty() {
        let name = settings.signing_key_name.clone();
        if analyte_contract::SigningSecret::parse(&signing).is_err() {
            return Err(SetupError::InvalidKey(name.as_str().into()));
        }
        let secret = Secret::new(&name, signing)?;
        updates.push((name, secret));
    }
    Ok(updates)
}

/// What a save wrote, for the page and the audit row; never a key value.
struct Saved {
    settings: Destination,
    replaced: Vec<String>,
    backups: Vec<PathBuf>,
    changed: bool,
    /// Machines that use it after the save.
    users: Vec<String>,
}

/// Writes the files (checked commit), then stores the keys. A refused or rolled
/// back write never changes a key.
fn write(
    state: &Context,
    settings: Destination,
    changes: &[(PathBuf, String)],
    updates: Vec<(Identifier, Secret)>,
) -> Result<Saved, SetupError> {
    let replaced = updates
        .iter()
        .map(|(name, _)| name.as_str().to_owned())
        .collect();
    let backups = if changes.is_empty() {
        Vec::new()
    } else {
        site::commit(&state.config_path, changes)?.backups
    };
    if !updates.is_empty() {
        Locked::new(&state.setup.data_directory)
            .replace(updates, Timestamp::now().as_second())
            .map_err(SetupError::KeysNotStored)?;
    }
    let users = scan(&state.config_path)
        .ok()
        .and_then(|scan| {
            scan.get(settings.name.as_str())
                .map(|known| known.users.clone())
        })
        .unwrap_or_default();
    Ok(Saved {
        settings,
        replaced,
        backups,
        changed: !changes.is_empty(),
        users,
    })
}

fn add_blocking(state: &Context, mut posted: Posted) -> Result<Saved, SetupError> {
    let _writing = state.setup.writing()?;
    let name = new_name(&posted.name)?;
    let scan = scan(&state.config_path)?;
    if scan.get(name.as_str()).is_some() {
        return Err(SetupError::LabAppTaken(name.as_str().into()));
    }
    file::refuse_lab_app_clash(scan.known.iter().map(|known| &known.settings.name), &name)?;
    let keys_named = [
        Identifier::new(&format!("{}-api", name.as_str())).map_err(|_| SetupError::LabAppName)?,
        Identifier::new(&format!("{}-signing", name.as_str()))
            .map_err(|_| SetupError::LabAppName)?,
    ];
    let settings = settings(&posted, name, keys_named)?;
    let after = scan.file.with(settings.clone())?;
    let updates = keys(&settings, &mut posted, true)?;
    let changes = [(file::path(&state.config_path), after.text()?)];
    write(state, settings, &changes, updates)
}

fn save_blocking(state: &Context, name: &str, mut posted: Posted) -> Result<Saved, SetupError> {
    let _writing = state.setup.writing()?;
    let scan = scan(&state.config_path)?;
    let known = scan
        .get(name)
        .ok_or_else(|| SetupError::UnknownLabApp(name.into()))?;
    let current = &known.settings;
    let settings = settings(
        &posted,
        current.name.clone(),
        [
            current.api_key_name.clone(),
            current.signing_key_name.clone(),
        ],
    )?;
    if settings != *current && !known.sites.is_empty() {
        return Err(SetupError::LabAppInSiteFile(
            name.into(),
            known.sites.join(", "),
        ));
    }
    let after = scan.file.with(settings.clone())?;
    let updates = keys(&settings, &mut posted, false)?;
    let mut changes = Vec::new();
    if after != scan.file {
        changes.push((file::path(&state.config_path), after.text()?));
    }
    // lab-apps.toml now holds it, so an older copy in the program file goes.
    if known.kept {
        changes.push((
            state.config_path.clone(),
            entries::without_retired(&scan.program, name)?,
        ));
    }
    write(state, settings, &changes, updates)
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
    let mut lines = vec![
        format!("Address: {}", saved.settings.base_url.as_str()),
        if saved.users.is_empty() {
            "Used by no machine yet: pick it on a machine’s setup page.".to_owned()
        } else {
            format!("Used by {}.", saved.users.join(", "))
        },
    ];
    if !saved.changed {
        lines.push("The settings were already the same; no file was written.".into());
    }
    lines.extend(
        saved
            .replaced
            .iter()
            .map(|key| format!("Key stored: {key}")),
    );
    if !saved.replaced.is_empty() {
        lines.push(format::KEYS_APPLY.into());
    }
    if !saved.backups.is_empty() {
        lines.push("A dated copy of the old settings was kept.".into());
    }
    let page = Report {
        frame: section.frame(state, title, Some(session)),
        head: section.head(title),
        tone: Tone::Good,
        title: format!("{name} is {}", change.verb()),
        text: "Analyte uses these settings after it restarts.".into(),
        lines,
        problems: audited
            .err()
            .map(|error| format::sentence(&error.to_string()))
            .into_iter()
            .collect(),
        restart: Some(session.csrf.form_value().to_owned()),
        links: section.links(),
        support: copies(&saved.backups),
    };
    respond(state, Some(session), section, StatusCode::OK, page.render())
}

/// A refused lab app form: nothing was written; the reason in words.
fn refused(state: &Context, session: &Session, name: Option<&str>, error: &SetupError) -> Response {
    // A lab app that does not exist has no page to go back to.
    let name = name.filter(|_| !matches!(error, SetupError::UnknownLabApp(_)));
    Problem::Refused(error.to_string()).page(state, Some(session), Section::LabApps(name))
}

/// What a lab app form did, for the audit row and the log.
#[derive(Clone, Copy)]
enum Change {
    Added,
    Saved,
}

impl Change {
    const fn action(self) -> &'static str {
        match self {
            Self::Added => "lab-app-add",
            Self::Saved => "lab-app-save",
        }
    }

    const fn verb(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Saved => "saved",
        }
    }
}

fn reason(change: Change, saved: &Saved) -> String {
    format!(
        "lab app page {} {}: address {}, order list current for {} s, request wait {} s, keys replaced: {}",
        change.verb(),
        saved.settings.name.as_str(),
        saved.settings.base_url.as_str(),
        saved.settings.freshness_seconds,
        saved.settings.request_timeout_seconds,
        if saved.replaced.is_empty() {
            "none".into()
        } else {
            saved.replaced.join(", ")
        }
    )
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
    let saved = off_thread(&state, move |state| add_blocking(state, posted)).await;
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
    let saved = off_thread(&state, move |state| save_blocking(state, &lab_app, posted)).await;
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
        reason(change, &saved),
    )
    .await;
    tracing::info!(lab_app = name, "lab app page {} a lab app", change.verb());
    saved_page(state, session, change, &saved, audited)
}

/// Why `known` cannot be removed now: machines use it or results wait for it.
fn blockers(known: &Known, waiting: Result<u64, &SetupError>) -> Vec<String> {
    let mut why = Vec::new();
    if !known.users.is_empty() {
        why.push(format!(
            "Machines send results to it: {}. Pick another lab app on their setup pages, or remove them, first.",
            known.users.join(", ")
        ));
    }
    match waiting {
        Ok(0) => {}
        Ok(count) => why.push(format!(
            "{} still wait for it. They are delivered first; remove it when the Overview shows nothing waiting.",
            format::amount(count, "result or order message", "results or order messages")
        )),
        Err(error) => why.push(format!(
            "What still waits for it could not be counted ({error}), so it is kept."
        )),
    }
    why
}

fn remove_blocking(
    state: &Context,
    name: &str,
    waiting: &Result<u64, SetupError>,
) -> Result<(Vec<PathBuf>, &'static str), SetupError> {
    let _writing = state.setup.writing()?;
    let scan = scan(&state.config_path)?;
    let known = scan
        .get(name)
        .ok_or_else(|| SetupError::UnknownLabApp(name.into()))?;
    let why = blockers(known, waiting.as_ref().copied());
    if !why.is_empty() {
        return Err(SetupError::LabAppInUse(name.into(), why.join(" ")));
    }
    let mut changes = Vec::new();
    if known.in_file {
        changes.push((
            file::path(&state.config_path),
            scan.file.without(name).text()?,
        ));
    }
    if known.kept {
        changes.push((
            state.config_path.clone(),
            entries::without_retired(&scan.program, name)?,
        ));
    }
    let keys = KeySource::read(state).kept(&known.settings);
    Ok((site::commit(&state.config_path, &changes)?.backups, keys))
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
    let open = waiting(&state).await.map(|open| count(&open, &name));
    let lab_app = name.clone();
    let removed = off_thread(&state, move |state| remove_blocking(state, &lab_app, &open)).await;
    let (backups, keys) = match removed {
        Ok(removed) => removed,
        Err(error) => return refused(&state, &session, Some(&name), &error),
    };
    let audited = audit(
        &state,
        &session,
        "lab-app-remove",
        &name,
        format!("lab app page removed lab app {name}; no machine used it and nothing waited for it; its keys stay in the locked file"),
    )
    .await;
    tracing::info!(lab_app = name.as_str(), "lab app page removed a lab app");
    let section = Section::LabApps(None);
    let title = "Lab app removed";
    let page = Report {
        frame: section.frame(&state, title, Some(&session)),
        head: section.head(title),
        tone: Tone::Good,
        title: format!("{name} is no longer listed"),
        text: keys.into(),
        lines: if backups.is_empty() {
            Vec::new()
        } else {
            vec!["A dated copy of the old settings was kept.".into()]
        },
        problems: audited
            .err()
            .map(|error| format::sentence(&error.to_string()))
            .into_iter()
            .collect(),
        restart: Some(session.csrf.form_value().to_owned()),
        links: section.links(),
        support: copies(&backups),
    };
    respond(
        &state,
        Some(&session),
        section,
        StatusCode::OK,
        page.render(),
    )
}

/// Which of `lab_app`'s keys neither the service environment nor the locked
/// keys file holds, said with when keys apply; none when both are found or
/// the keys file cannot be read, since then it cannot be told. Reads the keys
/// file, so call it off the async workers. Never reads out a value.
pub(super) fn missing_keys(state: &Context, lab_app: &Destination) -> Option<String> {
    let source = KeySource::read(state);
    let stored = source.stored.as_ref().ok()?;
    let missing: Vec<&str> = [
        ("API key", &lab_app.api_key_name),
        ("signing key", &lab_app.signing_key_name),
    ]
    .into_iter()
    .filter(|(_, name)| {
        matches!(
            secrets::source(name.as_str(), source.environment, stored),
            secrets::Source::Missing
        )
    })
    .map(|(label, _)| label)
    .collect();
    (!missing.is_empty())
        .then(|| format!("{} {}", format::missing_keys(&missing), format::KEYS_APPLY))
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
