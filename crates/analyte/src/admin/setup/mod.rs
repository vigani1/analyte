//! Serves the setup page: add or remove machines, set one up step by step (model, connection and lab app, test, save), restart.
//!
//! The steps ([`wizard`]) write the machine's site file (and, when the definition
//! changes, that one value in the program file); adding and removing change
//! program entries; every write keeps backups. Lab apps and their keys are set
//! once on the Lab apps page ([`lab_apps`]); a machine only picks one. Applying a
//! change needs a restart; there is no hot reload.
mod addresses;
pub(in crate::admin) mod cable;
pub(in crate::admin) mod choices;
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
pub(super) use lab_apps::relay::{
    LIMIT as COMMAND_LAB_APPS_LIMIT, PATH as COMMAND_LAB_APPS, post as command_lab_apps,
};
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
use analyte_i18n::{message as tr_args, text as tr};
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
    /// The lab app it sends results to; none while it records only (ADR 0034).
    lab_app: Option<Identifier>,
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
                    lab_app: effective.site().lab_app_name().cloned(),
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

    /// Finds the settings the service currently runs for this machine.
    fn running(&self, id: &str) -> Option<&Running> {
        self.running
            .iter()
            .find(|running| running.id.as_str() == id)
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
    #[error("{}", analyte_i18n::message("setup-error-unknown-machine", [("value", (.0).to_string().into())]))]
    UnknownMachine(String),
    #[error("{}", analyte_i18n::text("setup-error-unknown-definition"))]
    UnknownDefinition,
    #[error("{}", analyte_i18n::text("setup-error-no-definition-yet"))]
    NoDefinitionYet,
    #[error("{}", analyte_i18n::message("setup-error-blocked-definition", [("value", (.0).to_string().into())]))]
    BlockedDefinition(String),
    #[error("{0}")]
    DefinitionFile(#[from] Arc<definitions::FileError>),
    #[error("{}", analyte_i18n::message("setup-error-record-only-model", [("value", (.0).to_string().into())]))]
    RecordOnlyModel(String),
    #[error("{}", analyte_i18n::text("setup-error-lab-app-while-recording"))]
    LabAppWhileRecording,
    #[error("{0}")]
    RecordingAnswer(&'static str),
    #[error("{0}")]
    Definitions(#[from] definitions::ListError),
    #[error("{}", analyte_i18n::message("setup-error-invalid-url", [("value", (.0).to_string().into())]))]
    InvalidUrl(&'static str),
    #[error("{}", analyte_i18n::text("setup-error-no-lab-app-chosen"))]
    NoLabAppChosen,
    #[error("{}", analyte_i18n::message("setup-error-unknown-lab-app", [("value", (.0).to_string().into())]))]
    UnknownLabApp(String),
    #[error("{}", analyte_i18n::message("setup-error-lab-app-differs", [("value", (.0).to_string().into())]))]
    LabAppDiffers(String),
    #[error("{}", analyte_i18n::text("setup-error-lab-app-name"))]
    LabAppName,
    #[error("{}", analyte_i18n::message("setup-error-lab-app-taken", [("value", (.0).to_string().into())]))]
    LabAppTaken(String),
    #[error("{}", analyte_i18n::text("setup-error-keys-required"))]
    KeysRequired,
    #[error("{}", analyte_i18n::message("setup-error-invalid-number", [("value", (.0).to_string().into())]))]
    InvalidNumber(&'static str),
    #[error("{}", analyte_i18n::text("setup-error-review-notices-choice"))]
    ReviewNoticesChoice,
    #[error("{}", analyte_i18n::message("setup-error-lab-app-in-site-file", [("value", (.0).to_string().into()), ("value-2", (.1).to_string().into())]))]
    LabAppInSiteFile(String, String),
    #[error("{}", analyte_i18n::message("setup-error-lab-app-in-use", [("value", (.0).to_string().into()), ("value-2", (.1).to_string().into())]))]
    LabAppInUse(String, String),
    #[error("{0}")]
    Connection(&'static str),
    #[error("{}", analyte_i18n::message("setup-error-unknown-choice", [("value", (.0).to_string().into())]))]
    UnknownChoice(String),
    #[error("{}", analyte_i18n::message("setup-error-choice-value", [("choice", (choice).to_string().into()), ("allowed", (allowed).to_string().into())]))]
    ChoiceValue { choice: String, allowed: String },
    #[error("{}", analyte_i18n::message("setup-error-incomplete-form", [("value", (.0).to_string().into())]))]
    IncompleteForm(&'static str),
    #[error("{}", analyte_i18n::message("setup-error-invalid-key", [("value", (.0).to_string().into())]))]
    InvalidKey(String),
    #[error("{}", analyte_i18n::message("setup-error-program-file", [("value", (.0).to_string().into())]))]
    ProgramFile(#[source] toml::de::Error),
    #[error("{}", analyte_i18n::message("setup-error-program-file", [("value", (.0).to_string().into())]))]
    ProgramLayout(&'static str),
    #[error("{}", analyte_i18n::message("setup-error-site-read", [("path", (path.display()).to_string().into()), ("source", (source).to_string().into())]))]
    SiteRead {
        path: PathBuf,
        source: Box<crate::installation::config::Error>,
    },
    #[error("{}", analyte_i18n::message("setup-error-site-file", [("value", (.0).to_string().into())]))]
    SiteFile(#[source] Box<analyte_config::Error>),
    #[error("{}", analyte_i18n::message("setup-error-site-destination", [("value", (.0).to_string().into())]))]
    SiteDestination(#[source] toml::de::Error),
    #[error("{}", analyte_i18n::message("setup-error-site-text", [("value", (.0).to_string().into())]))]
    SiteText(#[source] toml::ser::Error),
    #[error("{}", analyte_i18n::message("setup-error-config", [("value", (.0).to_string().into())]))]
    Config(#[from] analyte_config::Error),
    #[error("{0}")]
    Installation(#[from] crate::installation::config::Error),
    #[error("{}", analyte_i18n::message("setup-error-rolled-back", [("value", (.0).to_string().into())]))]
    RolledBack(#[source] crate::installation::config::Error),
    #[error("{}", analyte_i18n::message("setup-error-io", [("operation", (operation).to_string().into()), ("path", (path.display()).to_string().into()), ("source", (source).to_string().into())]))]
    Io {
        operation: &'static str,
        path: PathBuf,
        source: io::Error,
    },
    #[error("{0}")]
    Credentials(#[from] CredentialError),
    #[error("{}", analyte_i18n::message("setup-error-keys-not-stored", [("value", (.0).to_string().into())]))]
    KeysNotStored(CredentialError),
    #[error("{}", analyte_i18n::message("setup-error-in-use", [("value", (.0).to_string().into())]))]
    InUse(String),
    #[error("{}", analyte_i18n::text("setup-error-nothing-to-listen"))]
    NothingToListen,
    #[error("{}", analyte_i18n::text("setup-error-listen-state"))]
    ListenState,
    #[error("{}", analyte_i18n::message("setup-error-serial-settings", [("value", (.0).to_string().into())]))]
    SerialSettings(#[source] crate::machine::protocols::Error),
    #[error("{0}")]
    Lookup(#[from] serial::Lookup),
    #[error("{}", analyte_i18n::message("setup-error-join", [("value", (.0).to_string().into())]))]
    Join(#[from] tokio::task::JoinError),
    #[error("{}", analyte_i18n::message("setup-error-audit", [("value", (.0).to_string().into())]))]
    Audit(#[source] AuditFailure),
    #[error("{}", analyte_i18n::message("setup-error-invalid-name", [("value", (.0).to_string().into())]))]
    InvalidName(analyte_model::TextError),
    #[error("{}", analyte_i18n::text("setup-error-name-characters"))]
    NameCharacters,
    #[error("{}", analyte_i18n::message("setup-error-reserved-name", [("value", (.0).to_string().into())]))]
    ReservedName(String),
    #[error("{}", analyte_i18n::message("setup-error-name-in-use", [("value", (.0).to_string().into())]))]
    NameInUse(String),
    #[error("{}", analyte_i18n::message("setup-error-site-in-use", [("value", (.0).to_string().into()), ("value-2", (.1).to_string().into())]))]
    SiteInUse(String, String),
    #[error("{}", analyte_i18n::message("setup-error-history-not-confirmed", [("value", (.0).to_string().into())]))]
    HistoryNotConfirmed(String),
    #[error("{}", analyte_i18n::message("setup-error-unfinished-work", [("value", (.0).to_string().into()), ("value-2", (.1).to_string().into())]))]
    UnfinishedWork(String, String),
    #[error("{}", analyte_i18n::message("setup-error-store", [("value", (.0).to_string().into())]))]
    Store(#[source] StoreFailure),
    #[error("{}", analyte_i18n::text("setup-error-busy"))]
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
            let running = state.setup.running(id);
            let (tone, word) = match (running, &entry.definition) {
                (Some(_), _) => (Tone::Good, tr("admin-running")),
                (None, None) => (Tone::Neutral, format::not_set_up()),
                (None, Some(_)) => (Tone::Attention, format::starts_after_restart()),
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
                status: status(Tone::Attention, format::stops_at_restart()),
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
            hover(
                evidence_words(*level),
                tr_args(
                    "setup-evidence-level",
                    [("level", (level).to_string().into())],
                ),
            ),
            status(Tone::Good, tr("setup-ready")),
            Icon::Ok,
        ),
        Shipped::Blocked { record } => (
            hover(
                tr("setup-not-tested-yet"),
                tr("setup-evidence-level-none-blocked-definition").into(),
            ),
            Status {
                note: Some(if record.is_some() {
                    tr("setup-facts-or-recordings-are-still-missing-its-readme-lists").into()
                } else {
                    tr("setup-facts-or-recordings-are-still-missing-its-readme-lists-2").into()
                }),
                ..status(Tone::Neutral, tr("setup-not-ready-yet"))
            },
            Icon::Off,
        ),
        Shipped::Unreadable { error, .. } => (
            hover(
                format::not_applicable(),
                tr("setup-evidence-level-unknown-the-definition-cannot-be-read").into(),
            ),
            Status {
                note: Some(format::sentence(&error.to_string())),
                ..status(Tone::Problem, tr("setup-cannot-be-read"))
            },
            Icon::Problem,
        ),
    }
}

/// How a model was checked, in words.
fn evidence_words(level: analyte_config::EvidenceLevel) -> &'static str {
    match level {
        analyte_config::EvidenceLevel::Documented => tr("setup-built-from-the-manuals"),
        analyte_config::EvidenceLevel::Simulated => tr("setup-tested-with-a-simulator"),
        analyte_config::EvidenceLevel::Proven => tr("setup-tested-on-the-real-machine"),
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
            title: crate::admin::removed::count_failed().into(),
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
        .map_err(|error| {
            format::sentence(&tr_args(
                "setup-analyte-s-settings-cannot-be-read",
                [("error", (error).to_string().into())],
            ))
        });
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
            Tab::Setup.label(),
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
            Shipped::Ready { name, .. } => format!("{} (name {})", definition.key, name.as_str()),
            Shipped::Blocked { .. } | Shipped::Unreadable { .. } => definition.key.clone(),
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
    #[error("{}", analyte_i18n::text("setup-error-not-signed-in"))]
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

/// Runs `work` off the async workers: it reads or writes files, the port list
/// or the credentials file, which block.
async fn off_thread<S: Send + Sync + 'static, T: Send + 'static>(
    state: &Arc<S>,
    work: impl FnOnce(&S) -> Result<T, SetupError> + Send + 'static,
) -> Result<T, SetupError> {
    let worker = state.clone();
    let language = analyte_i18n::Language::current();
    tokio::task::spawn_blocking(move || language.sync_scope(|| work(&worker)))
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
        .map(|backup| {
            tr_args(
                "setup-copy-of-the-old-settings",
                [("backup", (backup.display()).to_string().into())],
            )
        })
        .collect()
}

/// What to do after a Lab app, add or remove form could not be read.
fn reload_advice() -> &'static str {
    tr("setup-reload-advice")
}

/// When saved settings start taking effect.
fn restart_required() -> &'static str {
    tr("setup-restart-required")
}

/// Shared confirmation for a save that leaves the settings file alone.
fn settings_unchanged() -> &'static str {
    tr("setup-settings-unchanged")
}

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
            restarting(),
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

/// Confirms that setup kept a dated backup.
pub(in crate::admin) fn backup_kept() -> &'static str {
    tr("setup-saved-backup-kept")
}

/// Explains what happens to old settings when setup removes an entry.
pub(in crate::admin) fn backup_advice() -> &'static str {
    tr("setup-backup-advice")
}

/// Heads the checks before a settings entry is removed.
pub(in crate::admin) fn before_removal() -> &'static str {
    tr("setup-before-removal")
}

/// Explains what a reused machine name does with the earlier history.
pub(in crate::admin) fn reused_machine_history() -> &'static str {
    tr("setup-reused-machine-history")
}

/// Confirms that saving a machine moved its lab app into the shared list.
pub(in crate::admin) fn lab_app_moved() -> &'static str {
    tr("setup-lab-app-moved")
}

/// Labels the action to remove a setup entry.
pub(in crate::admin) fn remove_label() -> &'static str {
    tr("setup-remove")
}

/// Labels the action to save settings.
pub(in crate::admin) fn save_label() -> &'static str {
    tr("setup-save")
}

/// Heads the page shown during an engine restart.
pub(in crate::admin) fn restarting() -> &'static str {
    tr("setup-restarting")
}

/// Labels the action to add a lab app in Setup.
pub(in crate::admin) fn add_lab_app_label() -> &'static str {
    tr("setup-add-lab-app")
}

/// Says the site has no lab app configured.
pub(in crate::admin) fn no_lab_app() -> &'static str {
    tr("setup-no-lab-app")
}

/// Heads the warning when the lab apps file cannot be read.
pub(in crate::admin) fn lab_apps_unreadable() -> &'static str {
    tr("setup-lab-apps-unreadable")
}

/// Labels the action to enter lab app keys.
pub(in crate::admin) fn add_keys() -> &'static str {
    tr("setup-add-keys")
}
