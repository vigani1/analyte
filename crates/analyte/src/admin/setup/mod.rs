//! Serves the setup page: add or remove machines, choose a definition, a port and the lab app, test by listening, save, restart.
//!
//! The page writes the machine's site file (and, when the definition changes, that
//! one value in the program file), adds and removes program entries, keeps backups,
//! and stores keys in the locked credentials file. Applying a change needs a
//! restart; there is no hot reload.
mod definitions;
mod entries;
mod listen;
mod machines;
mod site;

pub(crate) use listen::{Limits, Open};
pub(super) use machines::{add, add_form, confirm_remove, remove};

use super::{markup::Markup, pages::NAV, server::Context, sessions::Session};
use crate::installation::{
    config::{Program, connections},
    credentials::{CredentialError, Locked, Secret},
    secrets,
};
use crate::machine::link::status::Link;
use analyte_config::{Endpoint, HttpsUrl};
use analyte_model::{Identifier, Reason};
use analyte_transport::serial;
use axum::{
    Extension, Form,
    extract::{Path, Query, State},
    http::{StatusCode, header},
    response::{Html, IntoResponse, Response},
};
use definitions::{Definition, State as Shipped};
use serde::Deserialize;
use site::Connection;
use std::{io, path::PathBuf, sync::Arc};
use tokio::sync::watch;

/// Largest setup form body; an API key alone may be 4096 bytes.
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
    keys: [Identifier; 2],
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
    listen: std::sync::Mutex<Option<listen::Session>>,
    restart: watch::Sender<bool>,
}

impl Setup {
    pub(crate) fn new(program: &Program, seams: Seams) -> Self {
        let running = program
            .machines()
            .iter()
            .map(|machine| {
                let effective = machine.snapshot().effective();
                let destination = &effective.site().destination;
                Running {
                    id: machine.id().clone(),
                    digest: machine.snapshot().digest(),
                    endpoint: effective.site().endpoint.clone(),
                    link: Link::of(effective.stack()),
                    keys: [
                        destination.api_key_name.clone(),
                        destination.signing_key_name.clone(),
                    ],
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
    #[error("choose one of the shipped machine definitions")]
    UnknownDefinition,
    #[error("this machine is not set up yet: choose its machine definition first")]
    NoDefinitionYet,
    #[error("{0} is a blocked machine definition; its README lists what is missing")]
    BlockedDefinition(String),
    #[error("{0}")]
    Definitions(#[from] definitions::ListError),
    #[error("the lab app address is not accepted: {0}")]
    InvalidUrl(&'static str),
    #[error("the destination name is not accepted: {0}")]
    InvalidDestination(analyte_model::TextError),
    #[error("{0}")]
    Connection(&'static str),
    #[error("the {0} key is not in the expected format, so nothing was saved")]
    InvalidKey(String),
    #[error("the program file cannot be read for setup: {0}")]
    ProgramFile(String),
    #[error("the current site file cannot be read for setup: {0}")]
    SiteFile(String),
    #[error("the settings do not pass check-config: {0}")]
    Config(#[from] analyte_config::Error),
    #[error("{0}")]
    Installation(#[from] crate::installation::config::Error),
    #[error("the saved files did not pass check-config, so the previous files were put back: {0}")]
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
    #[error("the serial port settings cannot be used: {0}")]
    SerialSettings(String),
    #[error("{0}")]
    Lookup(#[from] serial::Lookup),
    #[error("the setup task failed: {0}")]
    Join(#[from] tokio::task::JoinError),
    #[error("the audit record could not be written: {0}")]
    Audit(String),
    #[error("the machine name is not accepted: {0}")]
    InvalidName(analyte_model::TextError),
    #[error(
        "a machine name uses only letters A to Z, digits, - and _, because it also names the machine's site file"
    )]
    NameCharacters,
    #[error("{0} is a name the setup page uses itself; choose another name")]
    ReservedName(String),
    #[error("a machine called {0} is already listed; choose another name")]
    NameInUse(String),
    #[error("the site file {0} already belongs to machine {1}; choose another name")]
    SiteInUse(String, String),
    #[error(
        "{0} has stored results or history from an earlier machine. If this is the same analyzer, tick \"same analyzer\" to add it under that name; for a different analyzer choose a new name"
    )]
    HistoryNotConfirmed(String),
    #[error(
        "{0} still has unfinished work ({1}); nothing was changed. Review it on the remove page and press \"Remove anyway\" to remove the machine regardless"
    )]
    UnfinishedWork(String, String),
    #[error("the store could not be read: {0}")]
    Store(String),
    #[error("an earlier setup change failed while writing; reload the page and check the files")]
    Busy,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn page(title: &str) -> Markup {
    super::pages::start(title)
}

fn done(mut html: Markup) -> Response {
    html.push_markup("</html>");
    Html(html).into_response()
}

fn refused(error: &SetupError) -> Response {
    (
        StatusCode::CONFLICT,
        Html(
            Markup::new()
                .markup("<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><title>")
                .markup("Setup refused</title>")
                .markup(NAV)
                .markup("<h1>Setup refused</h1><p>")
                .text(error)
                .markup("</p><p><a href=\"/setup\">Back to setup</a></p></html>"),
        ),
    )
        .into_response()
}

fn needs_login() -> Response {
    done(page("Set up a machine").markup(
        "<p>Setup changes settings, so it needs a browser login: <a href=\"/login\">log in</a>.</p>",
    ))
}

/// One row per machine in the program file, then any machine that still runs but
/// is no longer listed (removed, waiting for the restart).
fn machine_rows(state: &Context, listed: &[site::EntrySpans]) -> Markup {
    let mut rows = Markup::new();
    for entry in listed {
        let id = entry.id.get_ref();
        let status = match (running(state, id), &entry.definition) {
            (Some(running), _) => Markup::new()
                .markup("<code>")
                .text(hex(&running.digest))
                .markup("</code>"),
            (None, None) => Markup::new().text("Not set up yet"),
            (None, Some(_)) => Markup::new().text("Not running: saved, starts after a restart"),
        };
        rows.push_fragment(
            &Markup::new()
                .markup("<tr><td>")
                .text(id)
                .markup("</td><td>")
                .fragment(&status)
                .markup("</td><td><a href=\"/setup/")
                .text(id)
                .markup("\">Set up</a></td><td><form method=\"get\" action=\"/setup/")
                .text(id)
                .markup("/remove\"><button>Remove machine</button></form></td></tr>"),
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
                .markup("<tr><td>")
                .text(gone.id.as_str())
                .markup("</td><td>Removed: runs until the restart</td><td></td><td></td></tr>"),
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

fn level_and_state(definition: &Definition) -> (String, String) {
    match &definition.state {
        Shipped::Ready { level, .. } => (level.clone(), "Ready to choose".into()),
        Shipped::Blocked => (
            "none".into(),
            "Blocked: facts or recordings are missing (see its README)".into(),
        ),
        Shipped::Unreadable(why) => ("unknown".into(), format!("Cannot be read: {why}")),
    }
}

/// GET /setup: the machines in the program file, what runs now, and the shipped definitions.
pub(super) async fn index(State(state): State<Arc<Context>>) -> Response {
    let mut html = page("Set up a machine").markup(concat!(
        "<p>Choose a machine below to set its connection and lab app. ",
        "Saving writes the machine's site file; Analyte then restarts to use it.</p>",
    ));
    match crate::admin::removed::lines(&state).await {
        Ok(lines) => {
            for line in lines {
                html.push_fragment(
                    &Markup::new()
                        .markup("<p><strong>")
                        .text(line)
                        .markup("</strong></p>"),
                );
            }
        }
        Err(error) => html.push_fragment(
            &Markup::new()
                .markup("<p>Results waiting for removed machines could not be counted: ")
                .text(error)
                .markup("</p>"),
        ),
    }
    html.push_markup(concat!(
        "<h2>Installed machines</h2><form method=\"get\" action=\"/setup/add\">",
        "<button>Add machine</button></form><table><tr><th>Machine</th><th>Running ",
        "configuration digest</th><th></th><th></th></tr>"
    ));
    let worker = state.clone();
    let listed = tokio::task::spawn_blocking(move || site::entries(&worker.config_path))
        .await
        .map_err(SetupError::from)
        .and_then(|r| r);
    match listed {
        Ok((_, listed)) => html.push_fragment(&machine_rows(&state, &listed)),
        Err(error) => html.push_fragment(
            &Markup::new()
                .markup("<tr><td colspan=\"4\">The program file cannot be read: ")
                .text(error)
                .markup("</td></tr>"),
        ),
    }
    html.push_markup(concat!(
        "</table><h2>Shipped machine definitions</h2><table><tr><th>Definition",
        "</th><th>Name</th><th>Evidence</th><th>Status</th></tr>"
    ));
    let worker = state.clone();
    let listed = tokio::task::spawn_blocking(move || definitions(&worker))
        .await
        .map_err(SetupError::from)
        .and_then(|r| r);
    match listed {
        Ok(list) => {
            for definition in &list {
                let name = match &definition.state {
                    Shipped::Ready { name, .. } => name.as_str(),
                    Shipped::Blocked | Shipped::Unreadable(_) => "",
                };
                let (level, status) = level_and_state(definition);
                html.push_fragment(
                    &Markup::new()
                        .markup("<tr><td>")
                        .text(&definition.key)
                        .markup("</td><td>")
                        .text(name)
                        .markup("</td><td>")
                        .text(level)
                        .markup("</td><td>")
                        .text(status)
                        .markup("</td></tr>"),
                );
            }
            html.push_markup("</table>");
        }
        Err(error) => {
            html.push_fragment(
                &Markup::new()
                    .markup("</table><p>")
                    .text(error)
                    .markup("</p>"),
            );
        }
    }
    done(html)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Pick {
    definition: Option<String>,
}

/// The raw transport kind of a definition, used only to choose which inputs to show.
fn kind_of(path: &std::path::Path) -> Option<String> {
    let text = crate::installation::config::read(path).ok()?;
    let table: toml::Table = text.parse().ok()?;
    table
        .get("transport")?
        .get("kind")?
        .as_str()
        .map(str::to_owned)
}

/// Finds the chosen shipped definition, or the installed one when none is chosen.
fn chosen(
    list: &[Definition],
    key: Option<&str>,
    installed: &site::Installed,
) -> Result<(String, PathBuf), SetupError> {
    let found = match key {
        Some(key) => list
            .iter()
            .find(|d| d.key == key)
            .ok_or(SetupError::UnknownDefinition)?,
        None => {
            let installed = installed
                .definition
                .as_ref()
                .ok_or(SetupError::NoDefinitionYet)?;
            let current = installed.canonicalize().ok();
            match list.iter().find(|d| match &d.state {
                Shipped::Ready { path, .. } => path.canonicalize().ok() == current,
                Shipped::Blocked | Shipped::Unreadable(_) => false,
            }) {
                Some(found) => found,
                None => {
                    return Ok(("installed".into(), installed.clone()));
                }
            }
        }
    };
    match &found.state {
        Shipped::Ready { path, .. } => Ok((found.key.clone(), path.clone())),
        Shipped::Blocked | Shipped::Unreadable(_) => {
            Err(SetupError::BlockedDefinition(found.key.clone()))
        }
    }
}

/// What the service runs for `id`, or `None` when it runs nothing for it yet:
/// not set up, or added or completed since the last start.
fn running<'a>(state: &'a Context, id: &str) -> Option<&'a Running> {
    state.setup.running.iter().find(|r| r.id.as_str() == id)
}

/// The running configuration digest, or why there is none.
fn running_digest(running: Option<&Running>) -> String {
    running.map_or_else(|| "none: not set up yet".into(), |r| hex(&r.digest))
}

/// The credential names to show: the running machine's, or for a machine still
/// waiting for setup, the names its site file already holds.
fn key_names(running: Option<&Running>, current_site: &toml::Table) -> Vec<Identifier> {
    if let Some(running) = running {
        return running.keys.to_vec();
    }
    ["api_key_name", "signing_key_name"]
        .iter()
        .filter_map(|key| {
            current_site
                .get("destination")
                .and_then(|d| d.get(*key))
                .and_then(toml::Value::as_str)
                .and_then(|name| Identifier::new(name).ok())
        })
        .collect()
}

fn key_status(state: &Context, name: &Identifier) -> Markup {
    let mut line = Markup::new().text(name.as_str()).markup(": ");
    if std::env::var_os(secrets::variable(name.as_str())).is_some() {
        return line
            .text("set in the environment (")
            .text(secrets::variable(name.as_str()))
            .text("), which wins over a stored key");
    }
    match Locked::new(&state.setup.data_directory).read() {
        Ok(stored) => match stored.set_at(name.as_str()) {
            Some(second) => {
                let date = jiff::Timestamp::from_second(second)
                    .map(|t| t.strftime("%Y-%m-%d").to_string())
                    .unwrap_or_else(|_| "an unknown date".into());
                line.push_text(format!("set on {date}"));
                line
            }
            None => line.text("not set"),
        },
        Err(error) => line.text(format!("cannot be checked: {error}")),
    }
}

/// GET /setup/{id}: the setup form for one installed machine.
pub(super) async fn machine(
    State(state): State<Arc<Context>>,
    session: Option<Extension<Arc<Session>>>,
    Path(id): Path<String>,
    Query(pick): Query<Pick>,
) -> Response {
    let Some(Extension(session)) = session.filter(|Extension(s)| s.authenticated) else {
        return needs_login();
    };
    // Reading files, the port list and the credentials file blocks; keep it off
    // the async workers.
    let worker = state.clone();
    let rendered = tokio::task::spawn_blocking(move || {
        render_machine(&worker, &session, &id, pick.definition.as_deref())
    })
    .await
    .map_err(SetupError::from)
    .and_then(|r| r);
    match rendered {
        Ok(html) => Html(html).into_response(),
        Err(error) => refused(&error),
    }
}

/// Builds the setup form for one installed machine; reads files and the port list.
fn render_machine(
    state: &Context,
    session: &Session,
    id: &str,
    definition: Option<&str>,
) -> Result<Markup, SetupError> {
    let (_, installed) = site::installed(&state.config_path, id)?;
    let current_site = crate::installation::config::read(&installed.site)
        .ok()
        .and_then(|text| text.parse::<toml::Table>().ok())
        .unwrap_or_default();
    let list = definitions(state)?;
    let picked = match chosen(&list, definition, &installed) {
        Ok(picked) => Some(picked),
        Err(SetupError::NoDefinitionYet) => None,
        Err(error) => return Err(error),
    };
    let running = running(state, id);
    let mut html = page("Set up a machine")
        .markup("<p>Machine <strong>")
        .text(id)
        .markup("</strong>. Running configuration digest <code>")
        .text(running_digest(running))
        .markup(concat!(
            "</code>.</p><h2>1. Machine</h2><form method=\"get\" action=\"\">",
            "<p>Choose the machine model from the shipped definitions.</p><ul>"
        ));
    html.push_fragment(&definition_picker(
        &list,
        picked.as_ref().map_or("", |(key, _)| key.as_str()),
    ));
    html.push_markup("</ul><button>Choose</button></form>");
    let Some((key, path)) = picked else {
        html.push_markup(concat!(
            "<p>This machine is not set up yet. Choose its machine definition ",
            "above and press Choose; the connection and lab app steps follow.</p></html>"
        ));
        return Ok(html);
    };
    let action = format!("/setup/{id}");
    html.push_fragment(
        &Markup::new()
            .markup("<form method=\"post\" action=\"")
            .text(&action)
            .markup("\"><input type=\"hidden\" name=\"csrf\" value=\"")
            .text(session.csrf.form_value())
            .markup("\"><input type=\"hidden\" name=\"definition\" value=\"")
            .text(&key)
            .markup("\"><h2>2. Connection</h2>"),
    );
    html.push_fragment(&connection_inputs(state, &path, &current_site));
    html.push_fragment(&test_and_lab_app(
        state,
        id,
        &key_names(running, &current_site),
        &current_site,
    ));
    Ok(html)
}

fn definition_picker(list: &[Definition], key: &str) -> Markup {
    let mut html = Markup::new();
    for definition in list {
        let (level, status) = level_and_state(definition);
        let selectable = matches!(definition.state, Shipped::Ready { .. });
        let mut item = Markup::new()
            .markup("<li><label><input type=\"radio\" name=\"definition\" value=\"")
            .text(&definition.key)
            .markup("\"");
        if definition.key == key {
            item.push_markup(" checked");
        }
        if !selectable {
            item.push_markup(" disabled");
        }
        item.push_fragment(
            &Markup::new()
                .markup("> ")
                .text(&definition.key)
                .markup(" (evidence: ")
                .text(level)
                .markup(") ")
                .text(status)
                .markup("</label></li>"),
        );
        html.push_fragment(&item);
    }
    html
}

fn connection_inputs(
    state: &Context,
    path: &std::path::Path,
    current_site: &toml::Table,
) -> Markup {
    let mut html = Markup::new();
    let current_endpoint = current_site
        .get("endpoint")
        .and_then(toml::Value::as_str)
        .unwrap_or_default();
    match kind_of(path).as_deref() {
        Some("serial") => html.push_fragment(&serial_choices(state, current_endpoint)),
        Some(kind @ ("tcp_server" | "tcp_client")) => {
            let (host, port) = current_endpoint.rsplit_once(':').unwrap_or(("", ""));
            html.push_fragment(
                &Markup::new()
                    .markup("<p>")
                    .markup(if kind == "tcp_server" {
                        "The machine connects to Analyte. Enter the address of this PC that Analyte listens on (0.0.0.0 means every network card) and the port the machine is set to."
                    } else {
                        "Analyte connects to the machine. Enter the machine's IP address and port."
                    })
                    .markup("</p><label>IP address <input name=\"host\" value=\"")
                    .text(host.trim_start_matches('[').trim_end_matches(']'))
                    .markup("\" maxlength=\"64\"></label> <label>Port <input name=\"tcp_port\" value=\"")
                    .text(port)
                    .markup("\" maxlength=\"5\" inputmode=\"numeric\"></label>"),
            );
        }
        _ => html.push_fragment(
            &Markup::new()
                .markup("<label>Folder <input name=\"folder\" value=\"")
                .text(current_endpoint)
                .markup("\" maxlength=\"1024\"></label>"),
        ),
    }
    html
}

fn test_and_lab_app(
    state: &Context,
    id: &str,
    keys: &[Identifier],
    current_site: &toml::Table,
) -> Markup {
    let mut statuses = Markup::new();
    for name in keys {
        statuses.push_fragment(
            &Markup::new()
                .markup("<li>")
                .fragment(&key_status(state, name))
                .markup("</li>"),
        );
    }
    let mut html = Markup::new();
    html.push_fragment(
        &Markup::new()
            .markup(concat!(
                "<h2>3. Test the connection</h2><p>The test only listens: it shows ",
                "what the machine sends for up to 60 seconds and sends nothing back. ",
                "What it shows may contain patient data; it appears only on this page ",
                "and is never stored or logged. It cannot use a port the running service ",
                "is using.</p><button formaction=\""
            ))
            .text(format!("/setup/{id}/listen"))
            .markup(concat!(
                "\">Test connection (listen only)</button>",
                "<h2>4. Lab app</h2><label>Lab app address <input name=\"base_url\" ",
                "required maxlength=\"4096\" value=\""
            ))
            .text(
                current_site
                    .get("destination")
                    .and_then(|d| d.get("base_url"))
                    .and_then(toml::Value::as_str)
                    .unwrap_or_default(),
            )
            .markup("\"></label> <label>Destination name <input name=\"destination\" required maxlength=\"128\" value=\"")
            .text(
                current_site
                    .get("destination")
                    .and_then(|d| d.get("name"))
                    .and_then(toml::Value::as_str)
                    .unwrap_or_default(),
            )
            .markup(concat!(
                "\"></label><p>Keys are stored so that only the Analyte service can ",
                "read them, and are never shown again. Leave a box empty to keep the ",
                "stored key.</p><ul>"
            ))
            .fragment(&statuses)
            .markup(concat!(
                "</ul><label>Replace API key <input name=\"api_key\" ",
                "type=\"password\" autocomplete=\"off\" maxlength=\"4096\"></label> ",
                "<label>Replace signing key <input name=\"signing_key\" ",
                "type=\"password\" autocomplete=\"off\" maxlength=\"4096\"></label>",
                "<h2>5. Save</h2><p>Saving checks everything first, keeps a copy of the ",
                "old site file, then writes the new one. Analyte keeps running the old ",
                "settings until you restart it.</p><button>Save</button></form></html>"
            )),
    );
    html
}

fn serial_choices(state: &Context, current: &str) -> Markup {
    let mut html = Markup::new().markup(concat!(
        "<p>Choose the port the machine's cable is plugged into. If the USB adapter ",
        "has its own serial number, Analyte saves that number, so it finds the ",
        "machine again even if the port name changes; otherwise it saves the port name.</p><ul>"
    ));
    match (state.setup.seams.ports)() {
        Ok(ports) => {
            for port in ports.iter().take(serial::LIST_LIMIT) {
                let (value, saved) = match port.factory_serial() {
                    Some(number) => (
                        format!("usb:{number}"),
                        format!("saved as serial number {number}"),
                    ),
                    None => (
                        format!("name:{}", port.name),
                        "no serial number; saved as the port name".into(),
                    ),
                };
                html.push_fragment(
                    &Markup::new()
                        .markup("<li><label><input type=\"radio\" name=\"port\" value=\"")
                        .text(value)
                        .markup("\"> ")
                        .text(&port.name)
                        .markup(" — ")
                        .text(port.description.as_deref().unwrap_or("unknown adapter"))
                        .markup(" — ")
                        .text(saved)
                        .markup("</label></li>"),
                );
            }
            if ports.is_empty() {
                html.push_markup("<li>No serial ports were found.</li>");
            }
        }
        Err(error) => html.push_fragment(
            &Markup::new()
                .markup("<li>Listing serial ports failed: ")
                .text(error)
                .markup("</li>"),
        ),
    }
    html.push_fragment(
        &Markup::new()
            .markup("</ul><label>Or type a port name <input name=\"port_name\" maxlength=\"1024\" placeholder=\"")
            .text(current)
            .markup("\"></label>"),
    );
    html
}

/// The setup form as posted. Keys are moved into [`Secret`] at once.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Choice {
    #[expect(dead_code, reason = "the CSRF middleware has already checked it")]
    csrf: String,
    definition: String,
    port: Option<String>,
    #[serde(default)]
    port_name: String,
    #[serde(default)]
    host: String,
    #[serde(default)]
    tcp_port: String,
    #[serde(default)]
    folder: String,
    #[serde(default)]
    base_url: String,
    #[serde(default)]
    destination: String,
    #[serde(default)]
    api_key: String,
    #[serde(default)]
    signing_key: String,
}

fn connection(kind: Option<&str>, choice: &Choice) -> Result<Connection, SetupError> {
    match kind {
        Some("serial") => {
            let typed = choice.port_name.trim();
            if !typed.is_empty() {
                return Ok(Connection::Endpoint(typed.into()));
            }
            match choice.port.as_deref().and_then(|v| v.split_once(':')) {
                Some(("usb", number)) => Ok(Connection::UsbSerial(
                    analyte_config::UsbSerial::new(number.into())
                        .map_err(SetupError::Connection)?,
                )),
                Some(("name", name)) if !name.is_empty() => Ok(Connection::Endpoint(name.into())),
                _ => Err(SetupError::Connection("choose a serial port")),
            }
        }
        Some("tcp_server" | "tcp_client") => {
            let host: std::net::IpAddr =
                choice.host.trim().parse().map_err(|_| {
                    SetupError::Connection("enter an IP address such as 192.168.1.20")
                })?;
            let port: u16 = choice
                .tcp_port
                .trim()
                .parse()
                .ok()
                .filter(|port| *port != 0)
                .ok_or(SetupError::Connection(
                    "enter a port number from 1 to 65535",
                ))?;
            Ok(Connection::Endpoint(
                std::net::SocketAddr::new(host, port).to_string(),
            ))
        }
        _ => {
            let folder = choice.folder.trim();
            if folder.is_empty() {
                return Err(SetupError::Connection("enter the folder path"));
            }
            Ok(Connection::Endpoint(folder.into()))
        }
    }
}

/// Everything a save or a test needs, checked and validated in memory.
struct Plan {
    program: String,
    installed: site::Installed,
    definition: PathBuf,
    connection: Connection,
    site_text: String,
    validated: analyte_config::Validated,
}

fn plan(state: &Context, id: &str, choice: &Choice) -> Result<Plan, SetupError> {
    let (program, installed) = site::installed(&state.config_path, id)?;
    let list = definitions(state)?;
    let key = (choice.definition != "installed").then_some(choice.definition.as_str());
    let (_, definition) = chosen(&list, key, &installed)?;
    let base_url = HttpsUrl::parse(choice.base_url.trim()).map_err(SetupError::InvalidUrl)?;
    let destination =
        Identifier::new(choice.destination.trim()).map_err(SetupError::InvalidDestination)?;
    let connection = connection(kind_of(&definition).as_deref(), choice)?;
    let current = crate::installation::config::read(&installed.site)?;
    let site_text = site::candidate(&current, &connection, &base_url, &destination)?;
    let validated = site::validate(&installed, &definition, &site_text)?;
    Ok(Plan {
        program,
        installed,
        definition,
        connection,
        site_text,
        validated,
    })
}

/// Turns the posted keys into checked values; errors name only the key.
fn keys(
    validated: &analyte_config::Validated,
    choice: &mut Choice,
) -> Result<Vec<(Identifier, Secret)>, SetupError> {
    let destination = &validated.effective().site().destination;
    let mut updates = Vec::new();
    let api = std::mem::take(&mut choice.api_key);
    if !api.is_empty() {
        let name = destination.api_key_name.clone();
        if !secrets::api_key_valid(&api) {
            return Err(SetupError::InvalidKey(name.as_str().into()));
        }
        let secret = Secret::new(&name, api)?;
        updates.push((name, secret));
    }
    let signing = std::mem::take(&mut choice.signing_key);
    if !signing.is_empty() {
        let name = destination.signing_key_name.clone();
        if analyte_contract::SigningSecret::parse(&signing).is_err() {
            return Err(SetupError::InvalidKey(name.as_str().into()));
        }
        let secret = Secret::new(&name, signing)?;
        updates.push((name, secret));
    }
    Ok(updates)
}

struct Saved {
    digest: [u8; 32],
    connection: String,
    base_url: String,
    replaced: Vec<String>,
    backups: Vec<PathBuf>,
}

fn save_blocking(state: &Context, id: &str, mut choice: Choice) -> Result<Saved, SetupError> {
    let _writing = state.setup.writing()?;
    let plan = plan(state, id, &choice)?;
    shares_connection(state, id, &plan)?;
    // Keys are checked before any file is written, and stored only after the
    // files pass, so a refused or rolled-back save never changes a key.
    let updates = keys(&plan.validated, &mut choice)?;
    let replaced = updates
        .iter()
        .map(|(name, _)| name.as_str().to_owned())
        .collect();
    let written = site::apply(
        &state.config_path,
        &plan.program,
        &plan.installed,
        &plan.definition,
        &plan.site_text,
    )?;
    if !updates.is_empty() {
        Locked::new(&state.setup.data_directory)
            .replace(updates, jiff::Timestamp::now().as_second())
            .map_err(SetupError::KeysNotStored)?;
    }
    Ok(Saved {
        digest: plan.validated.digest(),
        connection: plan.connection.describe(),
        base_url: plan
            .validated
            .effective()
            .site()
            .destination
            .base_url
            .as_str()
            .to_owned(),
        replaced,
        backups: written.backups,
    })
}

async fn audit(
    state: &Context,
    session: &Session,
    action: &str,
    id: &str,
    reason: String,
) -> Result<(), SetupError> {
    let failed = |error: &dyn std::fmt::Display| SetupError::Audit(error.to_string());
    let audit = analyte_store::Audit {
        actor: session.actor.clone(),
        action: Identifier::new(action).map_err(|e| failed(&e))?,
        targets: vec![Identifier::new(id).map_err(|e| failed(&e))?],
        time: jiff::Timestamp::now().as_second(),
        reason: Reason::new(&reason.chars().take(1000).collect::<String>())
            .map_err(|e| failed(&e))?,
    };
    state
        .store
        .request::<_, analyte_store::StorageError>(
            crate::deadline().map_err(|e| failed(&e))?,
            move |store| store.audit(&audit),
        )
        .await
        .map_err(|e| failed(&e))
}

/// POST /setup/{id}: validate, store keys, write the site file, keep backups.
pub(super) async fn save(
    State(state): State<Arc<Context>>,
    Extension(session): Extension<Arc<Session>>,
    Path(id): Path<String>,
    form: Result<Form<Choice>, axum::extract::rejection::FormRejection>,
) -> Response {
    let Ok(Form(choice)) = form else {
        return refused(&SetupError::Connection(
            "the form was incomplete; reload the page",
        ));
    };
    let worker = state.clone();
    let machine = id.clone();
    let saved = match tokio::task::spawn_blocking(move || save_blocking(&worker, &machine, choice))
        .await
        .map_err(SetupError::from)
        .and_then(|r| r)
    {
        Ok(saved) => saved,
        Err(error) => return refused(&error),
    };
    let reason = format!(
        "setup page saved: {}, lab app {}, keys replaced: {}",
        saved.connection,
        saved.base_url,
        if saved.replaced.is_empty() {
            "none".into()
        } else {
            saved.replaced.join(", ")
        }
    );
    let audited = audit(&state, &session, "setup-save", &id, reason).await;
    tracing::info!(machine = id.as_str(), "setup page saved a site file");
    let running = running_digest(running(&state, &id));
    let mut html = page("Settings saved")
        .markup("<p>Machine <strong>")
        .text(&id)
        .markup("</strong>: ")
        .text(&saved.connection)
        .markup(", lab app ")
        .text(&saved.base_url)
        .markup(".</p><p>New configuration digest <code>")
        .text(hex(&saved.digest))
        .markup("</code>. Running now <code>")
        .text(running)
        .markup("</code>.</p><ul>");
    for name in &saved.replaced {
        html.push_fragment(
            &Markup::new()
                .markup("<li>Stored key ")
                .text(name)
                .markup("</li>"),
        );
    }
    for backup in &saved.backups {
        html.push_fragment(
            &Markup::new()
                .markup("<li>Previous file kept as ")
                .text(backup.display())
                .markup("</li>"),
        );
    }
    html.push_markup("</ul>");
    if let Err(error) = audited {
        html.push_fragment(&Markup::new().markup("<p>").text(error).markup("</p>"));
    }
    html.push_fragment(&restart_form(&session));
    done(html)
}

fn restart_form(session: &Session) -> Markup {
    Markup::new()
        .markup(concat!(
            "<h2>Restart to apply</h2><p>Restarting stops every machine connection for ",
            "a few seconds; machines resend what they could not deliver. After the ",
            "restart, log in again: the setup page shows the running configuration digest.</p>",
            "<form method=\"post\" action=\"/setup/restart\"><input type=\"hidden\" ",
            "name=\"csrf\" value=\""
        ))
        .text(session.csrf.form_value())
        .markup("\"><button>Restart now</button></form>")
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
    let mut response = Html(Markup::new().markup(concat!(
        "<!doctype html><html lang=\"en\"><meta charset=\"utf-8\">",
        "<meta http-equiv=\"refresh\" content=\"15;url=/setup\">",
        "<title>Restarting</title><h1>Restarting</h1><p>Analyte is stopping ",
        "and the service manager starts it again within about 15 seconds. ",
        "If Analyte was started by hand in a terminal, start it again there. ",
        "This page then asks you to log in.</p></html>"
    )))
    .into_response();
    response.headers_mut().insert(
        header::CONNECTION,
        header::HeaderValue::from_static("close"),
    );
    response
}

/// Refuses a test on a serial port or network address a running machine uses.
/// A running machine named by `usb_serial` is looked up in the current port list,
/// so choosing its port by name is refused too.
fn in_use(state: &Context, chosen: &Endpoint, port: Option<&str>) -> Result<(), SetupError> {
    for running in &state.setup.running {
        let same = if let Some(port) = port {
            running.link == Link::Serial && {
                let running_port = match &running.endpoint {
                    Endpoint::Fixed(name) => Some(name.clone()),
                    Endpoint::UsbSerial(number) => {
                        serial::locate(&serial::Device::UsbSerial(number.as_str().into()), || {
                            (state.setup.seams.ports)()
                        })
                        .ok()
                    }
                };
                let same_number = matches!(
                    (&running.endpoint, chosen),
                    (Endpoint::UsbSerial(left), Endpoint::UsbSerial(right))
                        if left.as_str().eq_ignore_ascii_case(right.as_str())
                );
                same_number || running_port.is_some_and(|name| name.eq_ignore_ascii_case(port))
            }
        } else {
            matches!(running.link, Link::TcpServer | Link::TcpClient)
                && connections::listeners_overlap(&running.endpoint, chosen)
        };
        if same {
            return Err(SetupError::InUse(running.id.as_str().into()));
        }
    }
    Ok(())
}

/// Refuses, before writing, a connection another machine in the program file
/// already uses. When the current program does not load, the reload after
/// writing refuses instead, so a broken file can still be fixed here.
fn shares_connection(state: &Context, id: &str, plan: &Plan) -> Result<(), SetupError> {
    let Ok(program) = Program::load(&state.config_path) else {
        return Ok(());
    };
    let endpoint = plan.connection.endpoint();
    let chosen = (Link::of(plan.validated.effective().stack()), &endpoint);
    for other in program.machines().iter().filter(|m| m.id().as_str() != id) {
        if let Some(what) = connections::shared(connections::of(other), chosen) {
            return Err(crate::installation::config::Error::SharedConnection {
                first: other.id().as_str().into(),
                second: id.into(),
                what,
            }
            .into());
        }
    }
    Ok(())
}

fn listen_target(state: &Context, plan: &Plan) -> Result<(String, listen::Target), SetupError> {
    let stack = plan.validated.effective().stack();
    let endpoint = plan.connection.endpoint();
    match Link::of(stack) {
        Link::Serial => {
            let device = match &endpoint {
                Endpoint::Fixed(name) => serial::Device::Named(name.clone()),
                Endpoint::UsbSerial(number) => serial::Device::UsbSerial(number.as_str().into()),
            };
            let ports = state.setup.seams.ports.clone();
            let name = serial::locate(&device, || ports())?;
            in_use(state, &endpoint, Some(&name))?;
            let settings = crate::machine::link::serial::line_settings(
                stack,
                serial::Device::Named(name.clone()),
                listen::SERIAL_READ,
            )
            .ok_or(SetupError::NothingToListen)?
            .map_err(|error| SetupError::SerialSettings(error.to_string()))?;
            Ok((
                format!("serial port {name}"),
                listen::Target::Serial {
                    name,
                    settings,
                    open: state.setup.seams.open.clone(),
                },
            ))
        }
        link @ (Link::TcpServer | Link::TcpClient) => {
            in_use(state, &endpoint, None)?;
            let Endpoint::Fixed(text) = &endpoint else {
                return Err(SetupError::Connection(
                    "a network link needs an IP address and port",
                ));
            };
            let address = text
                .parse()
                .map_err(|_| SetupError::Connection("enter an IP address and port"))?;
            Ok(if link == Link::TcpServer {
                (
                    format!("listening on {address}"),
                    listen::Target::TcpServer(address),
                )
            } else {
                (
                    format!("connecting to {address}"),
                    listen::Target::TcpClient(address),
                )
            })
        }
        Link::Folder => Err(SetupError::NothingToListen),
    }
}

/// POST /setup/{id}/listen: start the listen-only test with the form's choices.
pub(super) async fn listen_start(
    State(state): State<Arc<Context>>,
    Path(id): Path<String>,
    form: Result<Form<Choice>, axum::extract::rejection::FormRejection>,
) -> Response {
    let Ok(Form(mut choice)) = form else {
        return refused(&SetupError::Connection(
            "the form was incomplete; reload the page",
        ));
    };
    // The test never uses keys; drop them at once.
    drop(std::mem::take(&mut choice.api_key));
    drop(std::mem::take(&mut choice.signing_key));
    let worker = state.clone();
    let target = tokio::task::spawn_blocking(move || {
        let plan = plan(&worker, &id, &choice)?;
        listen_target(&worker, &plan)
    })
    .await
    .map_err(SetupError::from)
    .and_then(|r| r);
    let (label, target) = match target {
        Ok(found) => found,
        Err(error) => return refused(&error),
    };
    let session = listen::Session::start(label, target, state.setup.seams.limits);
    match state.setup.listen.lock() {
        Ok(mut slot) => {
            // Dropping the previous test stops it and discards its bytes.
            *slot = Some(session);
        }
        Err(_) => {
            return refused(&SetupError::Connection(
                "the listen test state is unavailable",
            ));
        }
    }
    let mut response = StatusCode::SEE_OTHER.into_response();
    response.headers_mut().insert(
        header::LOCATION,
        header::HeaderValue::from_static("/setup/listen"),
    );
    response
}

/// One line of the live view: offset, hex bytes and printable text.
fn dump(bytes: &[u8]) -> String {
    let mut text = String::new();
    for (index, line) in bytes.chunks(16).enumerate() {
        let hex: Vec<String> = line.iter().map(|b| format!("{b:02x}")).collect();
        let printable: String = line
            .iter()
            .map(|&b| {
                if (32..127).contains(&b) {
                    char::from(b)
                } else {
                    '.'
                }
            })
            .collect();
        text.push_str(&format!(
            "{:06x}  {:<47}  {printable}\n",
            index.saturating_mul(16),
            hex.join(" ")
        ));
    }
    text
}

/// GET /setup/listen: the live view, refreshed every two seconds while running.
pub(super) async fn listen_view(
    State(state): State<Arc<Context>>,
    session: Option<Extension<Arc<Session>>>,
) -> Response {
    let Some(Extension(session)) = session.filter(|Extension(s)| s.authenticated) else {
        return needs_login();
    };
    let view = match state.setup.listen.lock() {
        Ok(slot) => slot.as_ref().map(listen::Session::view),
        Err(_) => None,
    };
    let Some(view) = view else {
        return done(
            page("Connection test")
                .markup("<p>No test has been started. Start one from a machine's setup page.</p>"),
        );
    };
    let running = view.end.is_none();
    let mut html =
        Markup::new().markup("<!doctype html><html lang=\"en\"><meta charset=\"utf-8\">");
    if running {
        html.push_markup("<meta http-equiv=\"refresh\" content=\"2\">");
    }
    html.push_fragment(
        &Markup::new()
            .markup("<title>Connection test</title>")
            .markup(NAV)
            .markup(concat!(
                "<h1>Connection test (listen only)</h1><p>Analyte sends nothing to ",
                "the machine. What is shown may contain patient data: it stays on this ",
                "page only, is never stored or logged, and is discarded when the test ends.</p><p>"
            ))
            .text(&view.label)
            .markup(". "),
    );
    let status = match &view.end {
        None if view.closed => format!(
            "The machine closed the connection. Showing for {} more seconds.",
            view.remaining.as_secs()
        ),
        None if view.connected => format!("Connected. {} seconds left.", view.remaining.as_secs()),
        None => format!(
            "Waiting for the connection. {} seconds left.",
            view.remaining.as_secs()
        ),
        Some(listen::End::TimeLimit) => {
            "The test ended after its time limit; the bytes were discarded.".into()
        }
        Some(listen::End::Stopped) => "The test was stopped; the bytes were discarded.".into(),
        Some(listen::End::Closed) => {
            "The machine closed the connection; the bytes were discarded.".into()
        }
        Some(listen::End::Failed(why)) => format!("The test could not run: {why}."),
    };
    html.push_fragment(
        &Markup::new()
            .text(status)
            .markup(" Bytes received: ")
            .text(view.seen)
            .markup("."),
    );
    if view.size_limit_reached {
        html.push_markup(" The 64 KiB display limit was reached, so reading stopped.");
    }
    html.push_markup("</p>");
    if running {
        html.push_fragment(
            &Markup::new()
                .markup("<pre>")
                .text(dump(&view.bytes))
                .markup(concat!(
                    "</pre><form method=\"post\" action=\"/setup/listen/stop\">",
                    "<input type=\"hidden\" name=\"csrf\" value=\""
                ))
                .text(session.csrf.form_value())
                .markup("\"><button>Stop the test</button></form>"),
        );
    }
    done(html)
}

/// POST /setup/listen/stop: end the test now and discard its bytes.
pub(super) async fn listen_stop(
    State(state): State<Arc<Context>>,
    Form(_): Form<Csrf>,
) -> Response {
    if let Ok(slot) = state.setup.listen.lock()
        && let Some(session) = slot.as_ref()
    {
        session.stop();
    }
    let mut response = StatusCode::SEE_OTHER.into_response();
    response.headers_mut().insert(
        header::LOCATION,
        header::HeaderValue::from_static("/setup/listen"),
    );
    response
}
