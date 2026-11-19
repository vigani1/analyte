//! Serves the setup page: add or remove machines, set one up step by step (model, connection and lab app, test, save), restart.
//!
//! The steps ([`wizard`]) write the machine's site file (and, when the definition
//! changes, that one value in the program file); adding and removing change
//! program entries; every write keeps backups. Lab apps and their keys are set
//! once on the Lab app page ([`lab_apps`]); a machine only picks one. Applying a
//! change needs a restart; there is no hot reload.
mod definitions;
mod entries;
mod lab_apps;
mod listen;
mod machines;
mod plan;
mod site;
mod verdict;
mod wizard;

pub(super) use lab_apps::{
    PAGE as LAB_APPS, add as add_lab_app, add_form as add_lab_app_form,
    confirm_remove as confirm_remove_lab_app, edit_form as edit_lab_app, list as lab_apps,
    remove as remove_lab_app, save as save_lab_app,
};
pub(crate) use listen::{Limits, Open};
pub(super) use machines::{add, add_form, confirm_remove, first, remove};
pub(super) use wizard::{
    connection, listen_start, listen_stop, listen_view, machine, review, save, test,
};

use super::{
    markup::Markup,
    server::Context,
    sessions::Session,
    view::{self, Refresh, Tab, Tone, pill},
};
use crate::installation::{config::Program, credentials::CredentialError};
use crate::machine::link::status::Link;
use analyte_config::Endpoint;
use analyte_model::{Identifier, Reason};
use analyte_transport::serial;
use axum::{
    Extension, Form,
    extract::State,
    http::{StatusCode, header},
    response::{Html, IntoResponse, Response},
};
use definitions::{Definition, State as Shipped};
use serde::Deserialize;
use std::{io, path::PathBuf, sync::Arc};
use tokio::sync::watch;

/// Largest setup or Lab app form body; an API key alone may be 4096 bytes.
pub(super) const FORM_LIMIT: usize = 16_384;

/// Lists the operating system's serial ports.
pub(crate) type Ports = Arc<dyn Fn() -> io::Result<Vec<serial::Listed>> + Send + Sync>;

/// Outside dependencies of the setup page; tests pass fakes.
#[derive(Clone)]
pub(crate) struct Seams {
    /// The shipped `machines` folder; `None` when it cannot be named.
    pub(crate) definitions: Option<PathBuf>,
    pub(crate) ports: Ports,
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

/// Setup state owned by the admin server.
pub(crate) struct Setup {
    seams: Seams,
    running: Vec<Running>,
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
        Self {
            seams,
            running,
            data_directory: program.data_directory().to_owned(),
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
    pub(crate) fn for_test(data_directory: &std::path::Path) -> Self {
        Self {
            seams: Seams {
                definitions: None,
                ports: Arc::new(|| Ok(Vec::new())),
                open: Arc::new(|_, _| Err(io::ErrorKind::Unsupported.into())),
                limits: Limits::STANDARD,
            },
            running: Vec::new(),
            data_directory: data_directory.to_owned(),
            writing: std::sync::Mutex::new(()),
            listen: std::sync::Mutex::new(None),
            restart: watch::channel(false).0,
        }
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
    #[error("{0} is not ready yet: facts or recordings are still missing (its README lists them)")]
    BlockedDefinition(String),
    #[error("{0}")]
    Definitions(#[from] definitions::ListError),
    #[error("the lab app address is not accepted: {0}")]
    InvalidUrl(&'static str),
    #[error("choose the lab app this machine sends results to; add one on the Lab app page first")]
    NoLabAppChosen,
    #[error("there is no lab app called {0}; add it on the Lab app page")]
    UnknownLabApp(String),
    #[error(
        "lab app {0} is already set up with other settings; make them the same on the Lab app page, then save again"
    )]
    LabAppDiffers(String, String),
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
        "lab app {0} is still kept in the settings of {1}; save that machine on its setup page first, which moves the lab app to the Lab app page, then change it there"
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
    ProgramFile(String),
    #[error("this machine's settings file cannot be read: {0}")]
    SiteFile(String),
    #[error("the settings are not valid: {0}")]
    Config(#[from] analyte_config::Error),
    #[error("{0}")]
    Installation(#[from] crate::installation::config::Error),
    #[error("the saved settings did not pass the check, so the previous files were put back: {0}")]
    RolledBack(String),
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
    SerialSettings(String),
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
    #[error("{0} is a name the setup page uses itself; choose another name")]
    ReservedName(String),
    #[error("a machine called {0} is already listed; choose another name")]
    NameInUse(String),
    #[error("the settings file {0} already belongs to machine {1}; choose another name")]
    SiteInUse(String, String),
    #[error(
        "{0} has stored results or history from an earlier machine. If this is the same analyzer, tick \"same analyzer\" to add it under that name; for a different analyzer choose a new name"
    )]
    HistoryNotConfirmed(String),
    #[error(
        "{0} still has unfinished work ({1}); nothing was changed. Review it on the remove page and press \"Remove anyway\" to remove the machine regardless"
    )]
    UnfinishedWork(String, String),
    #[error("Analyte's stored data could not be read: {0}")]
    Store(#[source] StoreFailure),
    #[error("an earlier setup change failed while writing; reload the page and check the files")]
    Busy,
}

/// Opens a page under the Setup tab; every setup step belongs to it.
fn page(title: &str) -> Markup {
    view::page(Tab::Setup, title)
}

fn done(html: Markup) -> Response {
    Html(html.markup(view::END)).into_response()
}

fn refused(error: &SetupError) -> Response {
    (
        StatusCode::CONFLICT,
        Html(
            page("Setup could not continue")
                .markup("<p class=\"notice bad\">")
                .text(error)
                .markup(concat!(
                    "</p><p><a class=\"button secondary\" href=\"/setup\">",
                    "Back to setup</a></p>",
                ))
                .markup(view::END),
        ),
    )
        .into_response()
}

fn needs_login() -> Response {
    done(page("Set up a machine").markup(concat!(
        "<p class=\"notice info\">Setup changes settings, so sign in first: ",
        "<a href=\"/login\">sign in</a>.</p>",
    )))
}

/// The running settings version: the short form, with the full digest on hover.
fn settings_version(running: Option<&Running>) -> Markup {
    match running {
        Some(running) => view::version(&running.digest),
        None => Markup::new()
            .markup("<span class=\"muted\">")
            .text(NO_DIGEST)
            .markup("</span>"),
    }
}

/// What the page says for a machine that runs nothing yet.
const NO_DIGEST: &str = "Not running yet";

/// A program entry's machine model for people, with its definition path on hover;
/// reads the definition folder's README, so call it off the async workers.
fn model(config: &std::path::Path, definition: Option<&str>) -> Markup {
    let Some(definition) = definition else {
        return Markup::new().markup("<span class=\"muted\">Not chosen yet</span>");
    };
    let folder = site::folder(config)
        .join(definition)
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_default();
    let name = |path: Option<&std::path::Path>| {
        path.and_then(std::path::Path::file_name)
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    let key = format!("{}/{}", name(folder.parent()), name(Some(folder.as_path())));
    view::exact(&definitions::display_name(&folder, &key), definition)
}

/// One row per machine in the program file, then any machine that still runs but
/// is no longer listed (removed, waiting for the restart). `models` holds each
/// listed entry's model, in the same order.
fn machine_rows(state: &Context, listed: &[site::EntrySpans], models: &[Markup]) -> Markup {
    let mut rows = Markup::new();
    for (entry, model) in listed.iter().zip(models) {
        let id = entry.id.get_ref();
        let running = running(state, id);
        let status = match (running, &entry.definition) {
            (Some(_), _) => pill(Tone::Good, "Running"),
            (None, None) => pill(Tone::Neutral, "Not set up yet"),
            (None, Some(_)) => pill(Tone::Attention, "Saved: starts after a restart"),
        };
        rows.push_fragment(
            &Markup::new()
                .markup("<tr><td><strong>")
                .text(id)
                .markup("</strong></td><td data-label=\"Model\">")
                .fragment(model)
                .markup("</td><td data-label=\"Status\">")
                .fragment(&status)
                .markup("</td><td data-label=\"Settings version\">")
                .fragment(&settings_version(running))
                .markup("</td><td class=\"actions\"><div class=\"buttons\"><a class=\"button secondary\" href=\"/setup/")
                .text(id)
                .markup("\">Set up</a><form method=\"get\" action=\"/setup/")
                .text(id)
                .markup(concat!(
                    "/remove\"><button class=\"quiet-danger\">Remove</button>",
                    "</form></div></td></tr>",
                )),
        );
    }
    for gone in state
        .setup
        .running
        .iter()
        .filter(|r| !listed.iter().any(|e| e.id.get_ref() == r.id.as_str()))
    {
        rows.push_fragment(
            &Markup::new()
                .markup("<tr><td><strong>")
                .text(gone.id.as_str())
                .markup("</strong></td><td data-label=\"Model\"></td><td data-label=\"Status\">")
                .fragment(&pill(Tone::Attention, "Removed: stops at the restart"))
                .markup("</td><td data-label=\"Settings version\">")
                .fragment(&settings_version(Some(gone)))
                .markup("</td><td></td></tr>"),
        );
    }
    rows
}

fn definitions(state: &Context) -> Result<Vec<Definition>, SetupError> {
    let root = state
        .setup
        .seams
        .definitions
        .clone()
        .ok_or_else(|| definitions::ListError::Missing(PathBuf::from("machines")))?;
    Ok(definitions::list(&root)?)
}

/// A definition's evidence level in words (the level itself on hover) and its
/// status, with the reason when it cannot be chosen.
fn level_and_state(definition: &Definition) -> (Markup, Markup) {
    match &definition.state {
        Shipped::Ready { level, .. } => (
            view::exact(evidence_words(level), &format!("Evidence level: {level}")),
            pill(Tone::Good, "Ready"),
        ),
        Shipped::Blocked => (
            view::exact(
                "Not tested yet",
                "Evidence level: none (blocked definition)",
            ),
            pill(Tone::Attention, "Not ready yet").markup(concat!(
                " <span class=\"muted\" title=\"Blocked definition; see its README\">Facts or ",
                "recordings are still missing</span>",
            )),
        ),
        Shipped::Unreadable(why) => (
            view::exact("Not known", "Evidence level unknown"),
            pill(Tone::Problem, "Cannot be read")
                .markup(" <span class=\"muted\">")
                .text(why)
                .markup("</span>"),
        ),
    }
}

/// The setup page's Lab apps section, with a link to add the first lab app.
async fn lab_apps_section(state: &Arc<Context>) -> Markup {
    let mut html = Markup::new().markup(concat!(
        "<section><div class=\"heading\"><h2>Lab apps</h2>",
        "<a class=\"button secondary\" href=\"/setup/lab-apps\">Lab apps</a></div>",
        "<p>Where results are sent. Each lab app is set once, with its address and keys; ",
        "each machine below picks one.</p>"
    ));
    // A fresh installation has none yet; a machine not set up needs none, but
    // setting one up does, so say where to add it.
    let scanned = off_thread(state, |state| lab_apps::scan(&state.config_path)).await;
    match scanned {
        Ok(scan) if scan.known.is_empty() => html.push_markup(concat!(
            "<p class=\"notice info\">No lab app yet: ",
            "<a href=\"/setup/lab-apps/new\">add one</a>.</p>"
        )),
        Ok(_) => {}
        // These files hold key names only, never key values.
        Err(error) => html.push_fragment(
            &Markup::new()
                .markup("<p class=\"notice bad\">The lab apps file could not be read: ")
                .text(error)
                .markup("</p>"),
        ),
    }
    html.markup("</section>")
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

/// GET /setup: the machines in the program file, what runs now, and the shipped definitions.
pub(super) async fn index(State(state): State<Arc<Context>>) -> Response {
    let mut html = page("Setup").markup(concat!(
        "<p class=\"lead\">Add a machine, or choose one to set its connection and pick its ",
        "lab app. Changes apply after Analyte restarts.</p>",
    ));
    html.push_fragment(&lab_apps_section(&state).await);
    match crate::admin::removed::lines(&state).await {
        Ok(lines) => {
            for line in lines {
                html.push_fragment(&line.notice(""));
            }
        }
        Err(error) => html.push_fragment(
            &Markup::new()
                .markup("<p class=\"notice bad\">Results still waiting from removed machines could not be counted: ")
                .text(error)
                .markup("</p>"),
        ),
    }
    html.push_markup(concat!(
        "<section><div class=\"heading\"><h2>Your machines</h2>",
        "<form method=\"get\" action=\"/setup/add\"><button>Add machine</button></form>",
        "</div><div class=\"table\"><table class=\"stack\"><tr><th>Machine</th><th>Model</th>",
        "<th>Status</th><th title=\"Running configuration digest\">Settings version</th>",
        "<th></th></tr>"
    ));
    let listed = off_thread(&state, |state| {
        site::entries(&state.config_path).map(|(_, listed)| {
            let models: Vec<Markup> = listed
                .iter()
                .map(|entry| {
                    model(
                        &state.config_path,
                        entry.definition.as_ref().map(|d| d.get_ref().as_str()),
                    )
                })
                .collect();
            (listed, models)
        })
    })
    .await;
    match listed {
        Ok((listed, models)) => html.push_fragment(&machine_rows(&state, &listed, &models)),
        Err(error) => html.push_fragment(
            &Markup::new()
                .markup("<tr><td colspan=\"5\">Analyte's settings cannot be read: ")
                .text(error)
                .markup("</td></tr>"),
        ),
    }
    html.push_markup(concat!(
        "</table></div></section><section><h2 title=\"Shipped machine definitions\">",
        "Supported machine models</h2>",
        "<p class=\"lead\">The models this Analyte can be set up for. A model that is not ",
        "ready yet cannot be chosen until its missing facts or recordings are added.</p>",
        "<div class=\"table\"><table class=\"stack\"><tr><th>Model</th>",
        "<th>How it was checked</th><th>Status</th></tr>"
    ));
    let listed = off_thread(&state, definitions).await;
    match listed {
        Ok(list) => {
            for definition in &list {
                let (level, status) = level_and_state(definition);
                html.push_fragment(
                    &Markup::new()
                        .markup("<tr><td><strong>")
                        .fragment(&definition_name(definition))
                        .markup("</strong></td><td data-label=\"How it was checked\">")
                        .fragment(&level)
                        .markup("</td><td data-label=\"Status\">")
                        .fragment(&status)
                        .markup("</td></tr>"),
                );
            }
            html.push_markup("</table></div></section>");
        }
        Err(error) => {
            html.push_fragment(
                &Markup::new()
                    .markup("</table></div><p class=\"notice bad\">")
                    .text(error)
                    .markup("</p></section>"),
            );
        }
    }
    done(html)
}

/// A definition's name for people, with its folder key (and definition name) on hover.
fn definition_name(definition: &Definition) -> Markup {
    let technical = match &definition.state {
        Shipped::Ready { name, .. } => format!("{} (name {name})", definition.key),
        Shipped::Blocked | Shipped::Unreadable(_) => definition.key.clone(),
    };
    view::exact(&definition.display, &technical)
}

/// What the service runs for `id`, or `None` when it runs nothing for it yet:
/// not set up, or added or completed since the last start.
fn running<'a>(state: &'a Context, id: &str) -> Option<&'a Running> {
    state.setup.running.iter().find(|r| r.id.as_str() == id)
}

async fn audit(
    state: &Context,
    session: &Session,
    action: &str,
    id: &str,
    reason: String,
) -> Result<(), SetupError> {
    let failed = |error| SetupError::Audit(AuditFailure::Text(error));
    let audit = analyte_store::Audit {
        actor: session.actor.clone(),
        action: Identifier::new(action).map_err(failed)?,
        targets: vec![Identifier::new(id).map_err(failed)?],
        time: jiff::Timestamp::now().as_second(),
        reason: Reason::new(&reason.chars().take(1000).collect::<String>()).map_err(failed)?,
    };
    stored::<_, analyte_store::StorageError>(state, move |store| store.audit(&audit))
        .await
        .map_err(|error| SetupError::Audit(AuditFailure::Store(error)))
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
    #[error(transparent)]
    Store(StoreFailure),
}

/// Runs one store request within the shared store deadline.
async fn stored<T, E>(
    state: &Context,
    work: impl FnOnce(&mut analyte_store::Store) -> Result<T, E> + Send + 'static,
) -> Result<T, StoreFailure>
where
    T: Send + 'static,
    E: From<analyte_store::WorkerError> + Send + 'static,
    StoreFailure: From<E>,
{
    let until = crate::deadline()?;
    Ok(state.store.request(until, work).await?)
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

/// What to do after a Lab app, add or remove form could not be read.
const RELOAD: &str = "reload the page";

/// The refusal of a posted form that could not be read; `hint` says what to do.
fn incomplete(hint: &'static str) -> Response {
    refused(&SetupError::IncompleteForm(hint))
}

/// Ends a page that wrote files: the audit failure, if the record of who made
/// the change could not be written (shown, never hidden), then Restart now.
fn after_write(mut html: Markup, audited: Result<(), SetupError>, session: &Session) -> Response {
    if let Err(error) = audited {
        html.push_fragment(
            &Markup::new()
                .markup("<p class=\"notice bad\">")
                .text(error)
                .markup("</p>"),
        );
    }
    html.push_fragment(&restart_form(session));
    done(html)
}

fn restart_form(session: &Session) -> Markup {
    Markup::new()
        .markup(concat!(
            "<section><h2>Restart to apply</h2><p>Restarting stops every machine connection for ",
            "a few seconds; machines resend what they could not deliver. After the ",
            "restart, sign in again: the setup page shows the running settings version.</p>",
            "<form method=\"post\" action=\"/setup/restart\"><input type=\"hidden\" ",
            "name=\"csrf\" value=\""
        ))
        .text(session.csrf.form_value())
        .markup("\"><button>Restart now</button></form></section>")
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
        return refused(&error);
    }
    tracing::info!("restart requested on the setup page");
    state.setup.restart.send_replace(true);
    let mut response = Html(
        view::refreshing(Tab::Setup, "Restarting", Refresh::SetupAfterRestart)
            .markup(concat!(
                "<p class=\"notice info\">Analyte is stopping ",
                "and the service manager starts it again within about 15 seconds. ",
                "If Analyte was started by hand in a terminal, start it again there. ",
                "This page then asks you to sign in.</p>"
            ))
            .markup(view::END),
    )
    .into_response();
    response.headers_mut().insert(
        header::CONNECTION,
        header::HeaderValue::from_static("close"),
    );
    response
}
