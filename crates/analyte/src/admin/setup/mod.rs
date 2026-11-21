//! Serves the setup page: add or remove machines, set one up step by step (model, connection and lab app, test, save), restart.
//!
//! The steps ([`wizard`]) write the machine's site file (and, when the definition
//! changes, that one value in the program file); adding and removing change
//! program entries; every write keeps backups. Lab apps and their keys are set
//! once on the Lab apps page ([`lab_apps`]); a machine only picks one. Applying a
//! change needs a restart; there is no hot reload.
mod addresses;
mod definitions;
pub(in crate::admin) use definitions::model_name;
mod command;
mod entries;
mod lab_apps;
mod machines;
mod page;
mod plan;
mod site;
mod wizard;
// Only `analyte doctor` on Windows reads the firewall rule's names.
#[cfg(windows)]
pub(in crate::admin) use wizard::verdict::{FIREWALL_RULE, FIREWALL_RULE_NAME};

pub use command::{LabApps, Machines, Outcome, run_lab_apps, run_machines};
pub(in crate::admin) use lab_apps::{Found, key_sources};
pub(super) use lab_apps::{
    PAGE as LAB_APPS, add as add_lab_app, add_form as add_lab_app_form,
    confirm_remove as confirm_remove_lab_app, edit_form as edit_lab_app, list as lab_apps,
    remove as remove_lab_app, save as save_lab_app,
};
pub(super) use machines::{add, add_form, confirm_remove, first, remove};
pub(crate) use wizard::listen::{Limits, Open};
pub(super) use wizard::{Step, listen_start, listen_stop, listen_view, save, step};

use super::{
    server::{Context, sessions::Session},
    view::{
        Tab, Tone, format,
        frame::Frame,
        icon::Icon,
        parts::{Hover, Notice},
        report::{Problem, Section, Status, respond},
    },
};
use crate::installation::{
    config::{Program, folder},
    credentials::CredentialError,
};
use crate::machine::link::status::Link;
use analyte_config::Endpoint;
use analyte_model::{Identifier, Reason};
use analyte_transport::serial;
use askama::Template as _;
use axum::{
    Extension, Form,
    extract::State,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use definitions::{Definition, State as Shipped};
use jiff::Timestamp;
use serde::Deserialize;
use std::{io, path::PathBuf, sync::Arc};
use tokio::sync::watch;

/// Largest setup or Lab app form body; an API key alone may be 4096 bytes.
pub(super) const FORM_LIMIT: usize = 16_384;

/// Lists the operating system's serial ports.
pub(crate) type Ports = Arc<dyn Fn() -> io::Result<Vec<serial::Listed>> + Send + Sync>;

/// Lists this PC's IPv4 addresses an analyzer on the network could send to.
pub(crate) type Addresses = Arc<dyn Fn() -> io::Result<Vec<Address>> + Send + Sync>;
pub(crate) use addresses::Address;

/// Outside dependencies of the setup page; tests pass fakes.
#[derive(Clone)]
pub(crate) struct Seams {
    /// The shipped `machines` folder; `None` when it cannot be named.
    pub(crate) definitions: Option<PathBuf>,
    pub(crate) ports: Ports,
    pub(crate) addresses: Addresses,
    pub(crate) open: Open,
    pub(crate) limits: Limits,
}

impl Seams {
    /// The operating system's ports and the `machines` folder beside the
    /// program file `config`, where installation copies the shipped definitions.
    pub(crate) fn system(config: &std::path::Path) -> Self {
        Self {
            definitions: config.parent().map(|folder| folder.join("machines")),
            ports: Arc::new(serial::list),
            addresses: Arc::new(addresses::list),
            open: Arc::new(|name, settings| settings.open(name)),
            limits: Limits::STANDARD,
        }
    }
}

/// What the running service loaded for one machine.
struct Running {
    id: Identifier,
    digest: [u8; 32],
    endpoint: Endpoint,
    link: Link,
    /// The lab app it sends results to.
    lab_app: Identifier,
}

/// The setup files and what guards them: the program file, the data directory
/// that holds the locked keys file, the outside seams, what runs now and the
/// write lock. The setup page and the `machines` and `lab-apps` commands both
/// change the installation through it, so they share every check and write.
pub(crate) struct Setup {
    seams: Seams,
    running: Vec<Running>,
    /// The program file (`analyte.toml`) the installation was loaded from.
    config: PathBuf,
    data_directory: PathBuf,
    /// Held while one save, add or remove reads and writes the configuration files,
    /// so two of them never interleave; only taken on blocking threads, never across an await.
    writing: std::sync::Mutex<()>,
    /// At most one listen test; the guard is never held across an await.
    listen: std::sync::Mutex<Option<wizard::Test>>,
    restart: watch::Sender<bool>,
}

impl Setup {
    pub(crate) fn new(program: &Program, seams: Seams) -> Self {
        let running = program
            .machines()
            .iter()
            .map(|machine| {
                let effective = machine.snapshot().effective();
                Running {
                    id: machine.id().clone(),
                    digest: machine.snapshot().digest(),
                    endpoint: effective.site().endpoint.clone(),
                    link: Link::of(effective.stack()),
                    lab_app: effective.site().destination.name.clone(),
                }
            })
            .collect();
        Self::with(
            seams,
            running,
            &program.source_path,
            program.data_directory(),
        )
    }

    fn with(
        seams: Seams,
        running: Vec<Running>,
        config: &std::path::Path,
        data_directory: &std::path::Path,
    ) -> Self {
        Self {
            seams,
            running,
            config: config.to_owned(),
            data_directory: data_directory.to_owned(),
            writing: std::sync::Mutex::new(()),
            listen: std::sync::Mutex::new(None),
            restart: watch::channel(false).0,
        }
    }

    /// Leaves the connection test state poisoned, as a thread that panics while
    /// holding it would. Only the admin server tests, built with test hooks, use it.
    #[cfg(all(test, feature = "test-hooks"))]
    pub(crate) fn poison_listen(&self) {
        std::thread::scope(|scope| {
            let _ = scope
                .spawn(|| {
                    let _held = self.listen.lock();
                    std::panic::resume_unwind(Box::new("poisoned by a test"));
                })
                .join();
        });
    }

    /// Keeps as running only the machines `runs` names, for a command line
    /// that learned from the engine what it runs.
    fn keep_running(&mut self, runs: impl Fn(&str) -> bool) {
        self.running.retain(|running| runs(running.id.as_str()));
    }

    /// The program file the installation was loaded from.
    pub(crate) fn config(&self) -> &std::path::Path {
        &self.config
    }

    /// Serializes changes to the configuration files.
    fn writing(&self) -> Result<std::sync::MutexGuard<'_, ()>, SetupError> {
        self.writing.lock().map_err(|_| SetupError::Busy)
    }

    /// Becomes true when an operator asks for a restart.
    pub(crate) fn restart_requested(&self) -> watch::Receiver<bool> {
        self.restart.subscribe()
    }

    /// Setup state for admin server tests that do not exercise the setup page.
    #[cfg(all(test, feature = "test-hooks"))]
    pub(crate) fn for_test(config: &std::path::Path, data_directory: &std::path::Path) -> Self {
        let seams = Seams {
            definitions: None,
            ports: Arc::new(|| Ok(Vec::new())),
            addresses: Arc::new(|| Ok(Vec::new())),
            open: Arc::new(|_, _| Err(io::ErrorKind::Unsupported.into())),
            limits: Limits::STANDARD,
        };
        Self::with(seams, Vec::new(), config, data_directory)
    }
}

/// Why a setup step was refused. The text is shown on the page; it never holds a key.
#[derive(Debug, thiserror::Error)]
pub(crate) enum SetupError {
    #[error("no installed machine is called {0}")]
    UnknownMachine(String),
    #[error("choose one of the listed machine models")]
    UnknownDefinition,
    #[error("this machine is not set up yet: choose its machine model first")]
    NoDefinitionYet,
    #[error(
        "model {0} is not ready yet: facts or recordings are still missing (its README lists them)"
    )]
    BlockedDefinition(String),
    #[error("{0}")]
    Definitions(#[from] definitions::ListError),
    #[error("the lab app address is not accepted: {0}")]
    InvalidUrl(&'static str),
    #[error("choose the lab app this machine sends results to; add one on the Lab apps page first")]
    NoLabAppChosen,
    #[error("there is no lab app called {0}; add it on the Lab apps page")]
    UnknownLabApp(String),
    #[error(
        "lab app {0} is already set up with other settings; make them the same on the Lab apps page, then save again"
    )]
    LabAppDiffers(String),
    #[error(
        "a lab app name uses only letters A to Z, digits, - and _ (at most 64), because it also names its keys"
    )]
    LabAppName,
    #[error("a lab app called {0} already exists; edit it, or choose another name")]
    LabAppTaken(String),
    #[error("enter both keys for a new lab app: the API key and the signing key")]
    KeysRequired,
    #[error("enter {0} as a whole number of seconds")]
    InvalidNumber(&'static str),
    #[error(
        "lab app {0} is still kept in the settings of {1}; save that machine on its setup page first, which moves the lab app to the Lab apps page, then change it there"
    )]
    LabAppInSiteFile(String, String),
    #[error("lab app {0} cannot be removed: {1}")]
    LabAppInUse(String, String),
    #[error("{0}")]
    Connection(&'static str),
    #[error("the form was incomplete; {0}")]
    IncompleteForm(&'static str),
    #[error("the {0} key is not in the expected format, so nothing was saved")]
    InvalidKey(String),
    #[error("Analyte's main settings file cannot be read: {0}")]
    ProgramFile(#[source] toml::de::Error),
    #[error("Analyte's main settings file cannot be read: {0}")]
    ProgramLayout(&'static str),
    #[error("this machine's settings file cannot be read: {0}")]
    SiteFile(#[source] toml::de::Error),
    #[error(
        "this machine's settings file cannot be read: its [destination] cannot be moved to the Lab apps page: {0}"
    )]
    SiteDestination(#[source] toml::de::Error),
    #[error("this machine's settings file cannot be read: {0}")]
    SiteText(#[source] toml::ser::Error),
    #[error("the settings are not valid: {0}")]
    Config(#[from] analyte_config::Error),
    #[error("{0}")]
    Installation(#[from] crate::installation::config::Error),
    #[error("the saved settings did not pass the check, so the previous files were put back: {0}")]
    RolledBack(#[source] crate::installation::config::Error),
    #[error("{operation} {path} failed: {source}")]
    Io {
        operation: &'static str,
        path: PathBuf,
        source: io::Error,
    },
    #[error("{0}")]
    Credentials(#[from] CredentialError),
    #[error("the settings were saved, but the keys were not stored: {0}")]
    KeysNotStored(CredentialError),
    #[error(
        "machine {0} is using this connection; the test cannot share it with the running service"
    )]
    InUse(String),
    #[error("a watched folder has no connection to listen on")]
    NothingToListen,
    #[error("the connection test state is unavailable; restart the service")]
    ListenState,
    #[error("the serial port settings cannot be used: {0}")]
    SerialSettings(#[source] crate::machine::protocols::Error),
    #[error("{0}")]
    Lookup(#[from] serial::Lookup),
    #[error("the setup step stopped unexpectedly: {0}")]
    Join(#[from] tokio::task::JoinError),
    #[error("the record of who made this change could not be written: {0}")]
    Audit(#[source] AuditFailure),
    #[error("the machine name is not accepted: {0}")]
    InvalidName(analyte_model::TextError),
    #[error(
        "a machine name uses only letters A to Z, digits, - and _, because it also names the machine's settings file"
    )]
    NameCharacters,
    #[error("the name {0} is one the setup page uses itself; choose another name")]
    ReservedName(String),
    #[error("a machine called {0} is already listed; choose another name")]
    NameInUse(String),
    #[error("the settings file {0} already belongs to machine {1}; choose another name")]
    SiteInUse(String, String),
    #[error(
        "machine name {0} has stored results or history from an earlier machine. If this is the same analyzer, tick \"same analyzer\" to add it under that name; for a different analyzer choose a new name"
    )]
    HistoryNotConfirmed(String),
    #[error(
        "machine {0} still has unfinished work ({1}); nothing was changed. Review it on the remove page and press \"Remove anyway\" to remove the machine regardless"
    )]
    UnfinishedWork(String, String),
    #[error("Analyte's stored data could not be read: {0}")]
    Store(#[source] StoreFailure),
    #[error("an earlier setup change failed while writing; reload the page and check the files")]
    Busy,
}

/// Refuses a setup request in `error`'s own words: nothing was changed (409).
fn refused(
    state: &Context,
    session: Option<&Session>,
    id: Option<&str>,
    error: &SetupError,
) -> Response {
    // A machine that does not exist has no setup steps to go back to.
    let id = id.filter(|_| !matches!(error, SetupError::UnknownMachine(_)));
    Problem::Refused(error.to_string()).page(state, session, Section::Setup(id))
}

/// Setup changes settings, so its pages need a signed-in browser session (403).
fn needs_login(state: &Context) -> Response {
    Problem::SignIn.page(state, None, Section::Setup(None))
}

/// The running settings version: the short form, with the full digest on
/// hover; none while it runs nothing.
fn settings_version(running: Option<&Running>) -> Option<Hover> {
    running.map(|running| Hover {
        shown: format::digest_short(&running.digest),
        exact: format::digest_hex(&running.digest),
    })
}

/// A program entry's machine model for people, with its definition path on hover;
/// the path itself when the definition names no model; none when no model is
/// chosen. Reads the definition and its folder's README, so call it off the
/// async workers.
fn model(
    config: &std::path::Path,
    root: Option<&std::path::Path>,
    definition: Option<&str>,
) -> Option<Hover> {
    let definition = definition?;
    Some(Hover {
        shown: model_name(&folder(config).join(definition), root)
            .unwrap_or_else(|| definition.to_owned()),
        exact: definition.to_owned(),
    })
}

/// One row per machine in the program file, then any machine that still runs but
/// is no longer listed (removed, waiting for the restart). `models` holds each
/// listed entry's model, in the same order.
fn machine_rows(
    state: &Context,
    listed: &[site::EntrySpans],
    models: Vec<Option<Hover>>,
) -> Vec<page::Machine> {
    let mut rows: Vec<page::Machine> = listed
        .iter()
        .zip(models)
        .map(|(entry, model)| {
            let id = entry.id.get_ref();
            let running = running(state, id);
            let (tone, word) = match (running, &entry.definition) {
                (Some(_), _) => (Tone::Good, "Running"),
                (None, None) => (Tone::Neutral, "Not set up yet"),
                (None, Some(_)) => (Tone::Attention, "Saved: starts after a restart"),
            };
            page::Machine {
                id: id.clone(),
                links: Some((format::setup_href(id, ""), format::setup_href(id, "remove"))),
                model,
                status: status(tone, word),
                version: settings_version(running),
            }
        })
        .collect();
    rows.extend(
        state
            .setup
            .running
            .iter()
            .filter(|r| !listed.iter().any(|e| e.id.get_ref() == r.id.as_str()))
            .map(|gone| page::Machine {
                id: gone.id.as_str().to_owned(),
                links: None,
                model: None,
                status: status(Tone::Attention, "Removed: stops at the restart"),
                version: settings_version(Some(gone)),
            }),
    );
    rows
}

/// A status with its word only: nothing more was observed.
fn status(tone: Tone, word: &str) -> Status {
    Status {
        tone,
        icon: tone.icon(),
        word: word.to_owned(),
        exact: String::new(),
        note: None,
    }
}

/// The shipped machine models in the `machines` folder `seams` names.
fn definitions(seams: &Seams) -> Result<Vec<Definition>, SetupError> {
    let root = seams
        .definitions
        .clone()
        .ok_or_else(|| definitions::ListError::Missing(PathBuf::from("machines")))?;
    Ok(definitions::list(&root)?)
}

/// A definition's evidence level in words (the level itself on hover), its
/// status, with the reason under it when it cannot be chosen, and the
/// status's icon: a model not ready yet has the design's slashed circle.
fn level_and_state(definition: &Definition) -> (Hover, Status, Icon) {
    let hover = |shown: &str, exact: String| Hover {
        shown: shown.to_owned(),
        exact,
    };
    match &definition.state {
        Shipped::Ready { level, .. } => (
            hover(evidence_words(level), format!("Evidence level: {level}")),
            status(Tone::Good, "Ready"),
            Icon::Ok,
        ),
        Shipped::Blocked => (
            hover(
                "Not tested yet",
                "Evidence level: none (blocked definition)".into(),
            ),
            Status {
                note: Some("Facts or recordings are still missing; its README lists them.".into()),
                ..status(Tone::Neutral, "Not ready yet")
            },
            Icon::Off,
        ),
        Shipped::Unreadable(why) => (
            hover(
                format::NOT_APPLICABLE,
                "Evidence level unknown: the definition cannot be read".into(),
            ),
            Status {
                note: Some(format::sentence(why)),
                ..status(Tone::Problem, "Cannot be read")
            },
            Icon::Problem,
        ),
    }
}

/// How a model was checked, in words; an unknown level shows as written.
fn evidence_words(level: &str) -> &str {
    match level {
        "documented" => "Built from the manuals",
        "simulated" => "Tested with a simulator",
        "proven" => "Tested on the real machine",
        other => other,
    }
}

/// GET /setup: the lab apps, the machines in the program file with what runs
/// now, and the shipped models.
pub(super) async fn index(
    State(state): State<Arc<Context>>,
    session: Option<Extension<Arc<Session>>>,
) -> Response {
    let session = session.map(|Extension(session)| session);
    // A fresh installation has none yet; a machine not set up needs none, but
    // setting one up does, so the page says where to add it. These files hold
    // key names only, never key values.
    let lab_apps = match off_thread(&state, |state| lab_apps::scan(state.setup.config())).await {
        Ok(scan) if scan.known.is_empty() => page::LabApps::None,
        Ok(_) => page::LabApps::Some,
        Err(error) => page::LabApps::Unreadable(format::sentence(&error.to_string())),
    };
    let notices = match crate::admin::removed::lines(&state).await {
        Ok(lines) => lines
            .iter()
            .map(|line| Notice {
                tone: line.tone(),
                title: line.text().to_owned(),
                text: String::new(),
            })
            .collect(),
        Err(error) => vec![Notice {
            tone: Tone::Problem,
            title: "Results still waiting from removed machines could not be counted".into(),
            text: format::sentence(&error.to_string()),
        }],
    };
    let listed = off_thread(&state, |state| {
        site::entries(state.setup.config()).map(|(_, listed)| {
            let models: Vec<Option<Hover>> = listed
                .iter()
                .map(|entry| {
                    model(
                        state.setup.config(),
                        state.setup.seams.definitions.as_deref(),
                        entry.definition.as_ref().map(|d| d.get_ref().as_str()),
                    )
                })
                .collect();
            (listed, models)
        })
    })
    .await;
    let machines = listed
        .map(|(listed, models)| machine_rows(&state, &listed, models))
        .map_err(|error| format::sentence(&format!("Analyte's settings cannot be read: {error}")));
    let models = off_thread(&state, |state| definitions(&state.setup.seams))
        .await
        .map(|list| {
            list.iter()
                .map(|definition| {
                    let (checked, status, icon) = level_and_state(definition);
                    page::Model {
                        name: definition_name(definition),
                        checked,
                        status,
                        icon,
                    }
                })
                .collect()
        })
        .map_err(|error| format::sentence(&error.to_string()));
    let page = page::Index {
        frame: Frame::new(
            &state,
            Tab::Setup,
            "Setup",
            None,
            session.as_deref(),
            Timestamp::now(),
        ),
        lab_apps,
        notices,
        machines,
        models,
    };
    respond(
        &state,
        session.as_deref(),
        Section::Setup(None),
        StatusCode::OK,
        page.render(),
    )
}

/// A definition's name for people, with its folder key (and definition name) on hover.
fn definition_name(definition: &Definition) -> Hover {
    Hover {
        shown: definition.display.clone(),
        exact: match &definition.state {
            Shipped::Ready { name, .. } => format!("{} (name {name})", definition.key),
            Shipped::Blocked | Shipped::Unreadable(_) => definition.key.clone(),
        },
    }
}

/// One machine the settings file lists, as the Machines pages show it.
pub(in crate::admin) struct Listed {
    pub(in crate::admin) id: String,
    /// Its model is chosen, so a restart runs it if it does not run now.
    pub(in crate::admin) set_up: bool,
}

/// The machines the settings file lists, in its order. It reads the file, so
/// it runs off the async workers.
pub(in crate::admin) async fn listed(state: &Arc<Context>) -> Result<Vec<Listed>, SetupError> {
    off_thread(state, |state| {
        site::entries(state.setup.config()).map(|(_, listed)| {
            listed
                .iter()
                .map(|entry| Listed {
                    id: entry.id.get_ref().clone(),
                    set_up: entry.definition.is_some(),
                })
                .collect()
        })
    })
    .await
}

/// What the service runs for `id`, or `None` when it runs nothing for it yet:
/// not set up, or added or completed since the last start.
fn running<'a>(state: &'a Context, id: &str) -> Option<&'a Running> {
    state.setup.running.iter().find(|r| r.id.as_str() == id)
}

/// Records who made a page change: the signed-in person of `session`.
async fn audit(
    state: &Context,
    session: &Session,
    action: &str,
    id: &str,
    reason: String,
) -> Result<(), SetupError> {
    let actor = session
        .actor()
        .ok_or(SetupError::Audit(AuditFailure::NotSignedIn))?;
    audit_as(&state.store, actor, action, id, &reason).await
}

/// Records that `actor` made the setup change `action` on `id`, and why; the
/// reason is cut to 1000 characters. Pages and the command line both use it.
async fn audit_as(
    store: &analyte_store::Handle,
    actor: &Identifier,
    action: &str,
    id: &str,
    reason: &str,
) -> Result<(), SetupError> {
    let failed = |error| SetupError::Audit(AuditFailure::Text(error));
    let audit = analyte_store::Audit {
        actor: actor.clone(),
        action: Identifier::new(action).map_err(failed)?,
        targets: vec![Identifier::new(id).map_err(failed)?],
        time: Timestamp::now().as_second(),
        reason: Reason::new(&reason.chars().take(1000).collect::<String>()).map_err(failed)?,
    };
    let written = async {
        let until = crate::deadline()?;
        Ok::<_, StoreFailure>(
            store
                .request::<_, analyte_store::StorageError>(until, move |store| store.audit(&audit))
                .await?,
        )
    };
    written
        .await
        .map_err(|error| SetupError::Audit(AuditFailure::Store(error)))
}

/// Where a setup change was made; its audit row says so in words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Via {
    /// The Setup or Lab apps page in a signed-in browser.
    Page,
    /// The `machines` or `lab-apps` command.
    CommandLine,
}

impl Via {
    /// Who changed a machine, as its audit reason starts.
    const fn machines(self) -> &'static str {
        match self {
            Self::Page => "setup page",
            Self::CommandLine => "command line",
        }
    }

    /// Who changed a lab app, as its audit reason starts.
    const fn lab_apps(self) -> &'static str {
        match self {
            Self::Page => "lab app page",
            Self::CommandLine => "command line",
        }
    }
}

/// Why a store request from a setup page did not complete.
#[derive(Debug, thiserror::Error)]
pub(crate) enum StoreFailure {
    #[error(transparent)]
    Deadline(#[from] crate::DeadlineError),
    #[error(transparent)]
    Storage(#[from] analyte_store::StorageError),
    #[error(transparent)]
    Operator(#[from] analyte_store::OperatorError),
    #[error(transparent)]
    Delivery(#[from] analyte_store::DeliveryError),
}

/// Why the record of who made a change could not be written.
#[derive(Debug, thiserror::Error)]
pub(crate) enum AuditFailure {
    #[error(transparent)]
    Text(analyte_model::TextError),
    /// The admin key was sent with a session nobody signed in on, so the
    /// change would have no one to record.
    #[error(
        "the browser session is not signed in, so the change names no one; sign in on this page"
    )]
    NotSignedIn,
    #[error(transparent)]
    Store(StoreFailure),
}

/// Runs one store read within the shared store deadline; it stops when the
/// page gives up on it.
async fn stored<T, E>(
    store: &analyte_store::Handle,
    work: impl FnOnce(&analyte_store::Store) -> Result<T, E> + Send + 'static,
) -> Result<T, StoreFailure>
where
    T: Send + 'static,
    E: From<analyte_store::WorkerError> + Send + 'static,
    StoreFailure: From<E>,
{
    let until = crate::deadline()?;
    Ok(store.read(until, work).await?)
}

/// Runs `work` on the setup files off the async workers: it reads or writes
/// files, the port list or the credentials file, which block.
async fn on_files<T: Send + 'static>(
    setup: &Arc<Setup>,
    work: impl FnOnce(&Setup) -> Result<T, SetupError> + Send + 'static,
) -> Result<T, SetupError> {
    let setup = setup.clone();
    tokio::task::spawn_blocking(move || work(&setup))
        .await
        .map_err(SetupError::from)
        .and_then(|done| done)
}

/// Runs `work` off the async workers: it reads or writes files, the port list
/// or the credentials file, which block.
async fn off_thread<T: Send + 'static>(
    state: &Arc<Context>,
    work: impl FnOnce(&Context) -> Result<T, SetupError> + Send + 'static,
) -> Result<T, SetupError> {
    let worker = state.clone();
    tokio::task::spawn_blocking(move || work(&worker))
        .await
        .map_err(SetupError::from)
        .and_then(|done| done)
}

/// See Other to `location`; a machine name that cannot be a header value (only
/// possible in a hand-written program file) goes to the setup page instead.
fn see_other(location: &str) -> Response {
    let mut response = StatusCode::SEE_OTHER.into_response();
    let location = header::HeaderValue::from_str(location)
        .unwrap_or_else(|_| header::HeaderValue::from_static("/setup"));
    response.headers_mut().insert(header::LOCATION, location);
    response
}

/// The dated copies of the old settings, for support: one line each, with its path.
fn copies(backups: &[PathBuf]) -> Vec<String> {
    backups
        .iter()
        .map(|backup| format!("Copy of the old settings: {}", backup.display()))
        .collect()
}

/// What to do after a Lab app, add or remove form could not be read.
const RELOAD: &str = "reload the page";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Csrf {
    #[expect(dead_code, reason = "the CSRF middleware has already checked it")]
    csrf: String,
}

/// POST /setup/restart: stop gracefully so the service manager starts Analyte again.
pub(super) async fn restart(
    State(state): State<Arc<Context>>,
    Extension(session): Extension<Arc<Session>>,
    Form(_): Form<Csrf>,
) -> Response {
    if let Err(error) = audit(
        &state,
        &session,
        "setup-restart",
        "service",
        "restart requested on the setup page to apply saved settings".into(),
    )
    .await
    {
        return refused(&state, Some(&session), None, &error);
    }
    tracing::info!("restart requested on the setup page");
    state.setup.restart.send_replace(true);
    let page = page::Restart {
        frame: Frame::new(
            &state,
            Tab::Setup,
            "Restarting",
            None,
            Some(&session),
            Timestamp::now(),
        ),
    };
    let mut response = respond(
        &state,
        Some(&session),
        Section::Setup(None),
        StatusCode::OK,
        page.render(),
    );
    response.headers_mut().insert(
        header::CONNECTION,
        header::HeaderValue::from_static("close"),
    );
    response
}
