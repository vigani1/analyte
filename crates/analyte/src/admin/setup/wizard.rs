//! Walks a lab manager through setting up one machine, one step per screen: 1 Machine, 2 Connection, 3 Test, 4 Save.
//!
//! No script runs, so each screen carries the earlier choices forward as hidden
//! form fields ([`Draft`]). They are never trusted: every step checks all the
//! earlier choices again against the files, and Save runs the whole check once
//! more under the write lock ([`super::plan`]). The fields hold no secret: keys
//! are set on the Lab app page. Steps 1 to 4 only read; the connection test and
//! Save are posts with the session's CSRF value, so that value never reaches a
//! URL. Hidden fields beat a server-side draft here: there is nothing to store,
//! bound, expire or lose at a restart, and Back works in every tab.
use super::{
    Choice, Csrf, SetupError, audit, chosen, current_lab_app, definition_name, definitions, done,
    kind_of, level_and_state, listen, listen_target, needs_login, page, plan, refused,
    restart_form, running, save_blocking, serial, settings_version, site,
    verdict::{self, Family},
};
use crate::admin::{
    markup::Markup,
    server::Context,
    sessions::Session,
    view::{self, Refresh, Tab, Tone, pill},
};
use crate::machine::link::status::Link;
use axum::{
    Extension, Form,
    extract::{Path, Query, State, rejection::QueryRejection},
    http::{StatusCode, header},
    response::{Html, IntoResponse, Response},
};
use serde::Deserialize;
use std::sync::Arc;

/// The choices made so far, as carried in a step's form. Every value is
/// re-checked before it is used; none is a secret.
#[derive(Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::admin) struct Draft {
    /// A shipped model's `vendor/model` key, or `installed` to keep the current one.
    pub(super) definition: Option<String>,
    /// A detected serial port: `usb:<serial number>` or `name:<port name>`.
    pub(super) port: Option<String>,
    /// A typed serial port name; it wins over `port`.
    pub(super) port_name: Option<String>,
    pub(super) host: Option<String>,
    pub(super) tcp_port: Option<String>,
    pub(super) folder: Option<String>,
    pub(super) lab_app: Option<String>,
}

/// The fields step 2 shows itself; every other step carries them hidden.
const CONNECTION_FIELDS: [&str; 6] = ["port", "port_name", "host", "tcp_port", "folder", "lab_app"];

impl Draft {
    fn fields(&self) -> [(&'static str, Option<&str>); 7] {
        [
            ("definition", self.definition.as_deref()),
            ("port", self.port.as_deref()),
            ("port_name", self.port_name.as_deref()),
            ("host", self.host.as_deref()),
            ("tcp_port", self.tcp_port.as_deref()),
            ("folder", self.folder.as_deref()),
            ("lab_app", self.lab_app.as_deref()),
        ]
    }

    /// Hidden inputs for every choice made, except the ones the step shows.
    fn hidden(&self, shown: &[&str]) -> Markup {
        let mut html = Markup::new();
        for (name, value) in self.fields() {
            if let Some(value) = value
                && !shown.contains(&name)
            {
                html.push_fragment(
                    &Markup::new()
                        .markup("<input type=\"hidden\" name=\"")
                        .markup(name)
                        .markup("\" value=\"")
                        .text(value)
                        .markup("\">"),
                );
            }
        }
        html
    }
}

impl From<Choice> for Draft {
    fn from(choice: Choice) -> Self {
        Self {
            definition: choice.definition,
            port: choice.port,
            port_name: choice.port_name,
            host: choice.host,
            tcp_port: choice.tcp_port,
            folder: choice.folder,
            lab_app: choice.lab_app,
        }
    }
}

/// One screen of the setup steps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Machine,
    Connection,
    Test,
    Save,
}

impl Step {
    const ALL: [Self; 4] = [Self::Machine, Self::Connection, Self::Test, Self::Save];

    const fn index(self) -> u8 {
        match self {
            Self::Machine => 1,
            Self::Connection => 2,
            Self::Test => 3,
            Self::Save => 4,
        }
    }

    const fn number(self) -> &'static str {
        match self {
            Self::Machine => "1",
            Self::Connection => "2",
            Self::Test => "3",
            Self::Save => "4",
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Machine => "Machine",
            Self::Connection => "Connection",
            Self::Test => "Test",
            Self::Save => "Save",
        }
    }

    /// The step's page; GET, with the draft as the query.
    fn path(self, id: &str) -> String {
        match self {
            Self::Machine => format!("/setup/{id}"),
            Self::Connection => format!("/setup/{id}/connection"),
            Self::Test => format!("/setup/{id}/test"),
            Self::Save => format!("/setup/{id}/save"),
        }
    }

    /// The step whose choice caused `error`, or `None` when no step can fix it.
    fn to_fix(error: &SetupError) -> Option<Self> {
        use crate::installation::config::Error as Installation;
        match error {
            SetupError::UnknownDefinition
            | SetupError::NoDefinitionYet
            | SetupError::BlockedDefinition(_)
            | SetupError::Definitions(_) => Some(Self::Machine),
            SetupError::Connection(_)
            | SetupError::NoLabAppChosen
            | SetupError::UnknownLabApp(_)
            | SetupError::LabAppDiffers(..)
            | SetupError::Config(_)
            | SetupError::Installation(Installation::SharedConnection { .. })
            | SetupError::InUse(_)
            | SetupError::SerialSettings(_)
            | SetupError::Lookup(_) => Some(Self::Connection),
            SetupError::NothingToListen | SetupError::RolledBack(_) => Some(Self::Save),
            SetupError::UnknownMachine(_)
            | SetupError::InvalidUrl(_)
            | SetupError::LabAppName
            | SetupError::LabAppTaken(_)
            | SetupError::KeysRequired
            | SetupError::InvalidNumber(_)
            | SetupError::LabAppInSiteFile(..)
            | SetupError::LabAppInUse(..)
            | SetupError::InvalidKey(_)
            | SetupError::ProgramFile(_)
            | SetupError::SiteFile(_)
            | SetupError::Installation(_)
            | SetupError::Io { .. }
            | SetupError::Credentials(_)
            | SetupError::KeysNotStored(_)
            | SetupError::Join(_)
            | SetupError::Audit(_)
            | SetupError::InvalidName(_)
            | SetupError::NameCharacters
            | SetupError::ReservedName(_)
            | SetupError::NameInUse(_)
            | SetupError::SiteInUse(..)
            | SetupError::HistoryNotConfirmed(_)
            | SetupError::UnfinishedWork(..)
            | SetupError::Store(_)
            | SetupError::Busy => None,
        }
    }
}

/// The step indicator: every step by number and name, the current one marked,
/// earlier ones done. `None` marks all four done, after a save.
fn indicator(current: Option<Step>) -> Markup {
    let mut html = Markup::new().markup("<ol class=\"steps\" aria-label=\"Setup steps\">");
    for step in Step::ALL {
        let done = current.is_none_or(|current| step.index() < current.index());
        html.push_markup(if current == Some(step) {
            "<li aria-current=\"step\">"
        } else if done {
            "<li class=\"done\">"
        } else {
            "<li>"
        });
        html.push_markup("<span class=\"number\">");
        html.push_markup(step.number());
        html.push_markup("</span>");
        html.push_markup(step.label());
        html.push_markup("</li>");
    }
    html.markup("</ol>")
}

/// A refusal that names the step to fix and goes back to it with every choice kept.
pub(super) fn refused_at(error: &SetupError, id: &str, draft: &Draft) -> Response {
    let Some(step) = Step::to_fix(error) else {
        return refused(error);
    };
    let html = page("Setup could not continue")
        .markup("<p class=\"notice bad\">")
        .text(error)
        .markup("</p><p>")
        .text(format!(
            "Fix this in step {}: {}.",
            step.number(),
            step.label()
        ))
        .markup("</p><form method=\"get\" action=\"")
        .text(step.path(id))
        .markup("\">")
        .fragment(&draft.hidden(&[]))
        .markup("<div class=\"actions-row\"><button>")
        .text(format!("Go to step {}: {}", step.number(), step.label()))
        .markup(concat!(
            "</button><a class=\"button secondary\" href=\"/setup\">Back to setup</a>",
            "</div></form>"
        ))
        .markup(view::END);
    (StatusCode::CONFLICT, Html(html)).into_response()
}

/// Opens a step's page: the machine's name in the title, then the indicator.
fn step_page(id: &str, step: Step) -> Markup {
    page(&format!("Set up {id}")).fragment(&indicator(Some(step)))
}

type Render = fn(&Context, &Session, &str, &Draft) -> Result<Markup, SetupError>;

/// Serves one step: signed in only; files are read off the async workers; a
/// choice that does not pass is refused with the step to fix.
async fn serve(
    state: Arc<Context>,
    session: Option<Extension<Arc<Session>>>,
    id: String,
    query: Result<Query<Draft>, QueryRejection>,
    render: Render,
) -> Response {
    let Some(Extension(session)) = session.filter(|Extension(s)| s.authenticated) else {
        return needs_login();
    };
    let Ok(Query(draft)) = query else {
        return refused(&SetupError::Connection(
            "the form was incomplete; start again from the setup page",
        ));
    };
    let worker = state.clone();
    let (machine, kept) = (id.clone(), draft.clone());
    let rendered = tokio::task::spawn_blocking(move || render(&worker, &session, &machine, &kept))
        .await
        .map_err(SetupError::from)
        .and_then(|r| r);
    match rendered {
        Ok(html) => done(html),
        Err(error) => refused_at(&error, &id, &draft),
    }
}

/// GET /setup/{id}: step 1, the machine model.
pub(in crate::admin) async fn machine(
    State(state): State<Arc<Context>>,
    session: Option<Extension<Arc<Session>>>,
    Path(id): Path<String>,
    query: Result<Query<Draft>, QueryRejection>,
) -> Response {
    serve(state, session, id, query, render_machine).await
}

/// GET /setup/{id}/connection: step 2, the port or address and the lab app.
pub(in crate::admin) async fn connection(
    State(state): State<Arc<Context>>,
    session: Option<Extension<Arc<Session>>>,
    Path(id): Path<String>,
    query: Result<Query<Draft>, QueryRejection>,
) -> Response {
    serve(state, session, id, query, render_connection).await
}

/// GET /setup/{id}/test: step 3, the listen-only connection test.
pub(in crate::admin) async fn test(
    State(state): State<Arc<Context>>,
    session: Option<Extension<Arc<Session>>>,
    Path(id): Path<String>,
    query: Result<Query<Draft>, QueryRejection>,
) -> Response {
    serve(state, session, id, query, render_test).await
}

/// GET /setup/{id}/save: step 4, the summary before saving.
pub(in crate::admin) async fn review(
    State(state): State<Arc<Context>>,
    session: Option<Extension<Arc<Session>>>,
    Path(id): Path<String>,
    query: Result<Query<Draft>, QueryRejection>,
) -> Response {
    serve(state, session, id, query, render_review).await
}

/// The chosen model's key and file; `installed` keeps the current one.
pub(super) fn model_of(
    list: &[super::Definition],
    draft: &Draft,
    installed: &site::Installed,
) -> Result<(String, std::path::PathBuf), SetupError> {
    match draft.definition.as_deref().map(str::trim) {
        None | Some("") => Err(SetupError::UnknownDefinition),
        Some("installed") => chosen(list, None, installed),
        Some(key) => chosen(list, Some(key), installed),
    }
}

/// The model's name for people; a model kept from outside the shipped list shows its file.
pub(super) fn model_name(list: &[super::Definition], key: &str, path: &std::path::Path) -> String {
    list.iter()
        .find(|definition| definition.key == key)
        .map_or_else(|| path.display().to_string(), |d| d.display.clone())
}

fn render_machine(
    state: &Context,
    _: &Session,
    id: &str,
    draft: &Draft,
) -> Result<Markup, SetupError> {
    let (_, installed) = site::installed(&state.config_path, id)?;
    let list = definitions(state)?;
    let current = match chosen(&list, None, &installed) {
        Ok(current) => Some(current),
        Err(SetupError::NoDefinitionYet) => None,
        Err(error) => return Err(error),
    };
    let selected = draft
        .definition
        .clone()
        .or_else(|| current.as_ref().map(|(key, _)| key.clone()))
        .unwrap_or_default();
    let mut html = step_page(id, Step::Machine)
        .markup("<section><dl><dt>Machine</dt><dd><strong>")
        .text(id)
        .markup(
            "</strong></dd><dt title=\"Running configuration digest\">Settings version</dt><dd>",
        )
        .fragment(&settings_version(running(state, id)))
        .markup("</dd></dl></section><form method=\"get\" action=\"")
        .text(Step::Connection.path(id))
        .markup(concat!(
            "\"><section><h2>1. Machine model</h2><p>Choose this machine's model. A model ",
            "that is not ready yet cannot be chosen.</p><ul class=\"choices\">"
        ));
    if let Some((key, path)) = &current
        && key == "installed"
    {
        html.push_fragment(
            &Markup::new()
                .markup("<li><label><input type=\"radio\" name=\"definition\" value=\"installed\" required")
                .markup(if selected == "installed" { " checked" } else { "" })
                .markup("> <strong>Keep the current model</strong> <span class=\"muted\">")
                .text(path.display())
                .markup("</span></label></li>"),
        );
    }
    html.push_fragment(&definition_picker(&list, &selected));
    Ok(html
        .markup("</ul></section>")
        .fragment(&draft.hidden(&["definition"]))
        .markup(concat!(
            "<div class=\"actions-row\"><a class=\"button secondary\" href=\"/setup\">Cancel</a>",
            "<button>Next: Connection</button></div></form>"
        )))
}

fn definition_picker(list: &[super::Definition], key: &str) -> Markup {
    let mut html = Markup::new();
    for definition in list {
        let (level, status) = level_and_state(definition);
        let selectable = matches!(definition.state, super::Shipped::Ready { .. });
        html.push_fragment(
            &Markup::new()
                .markup("<li><label><input type=\"radio\" name=\"definition\" value=\"")
                .text(&definition.key)
                .markup("\" required")
                .markup(if definition.key == key {
                    " checked"
                } else {
                    ""
                })
                .markup(if selectable { "" } else { " disabled" })
                .markup("> <strong>")
                .fragment(&definition_name(definition))
                .markup("</strong> <span class=\"muted\">")
                .fragment(&level)
                .markup("</span> ")
                .fragment(&status)
                .markup("</label></li>"),
        );
    }
    html
}

fn render_connection(
    state: &Context,
    _: &Session,
    id: &str,
    draft: &Draft,
) -> Result<Markup, SetupError> {
    let (_, installed) = site::installed(&state.config_path, id)?;
    let list = definitions(state)?;
    let (key, path) = model_of(&list, draft, &installed)?;
    let current_site = crate::installation::config::read(&installed.site)
        .ok()
        .and_then(|text| text.parse::<toml::Table>().ok())
        .unwrap_or_default();
    let lab_app = draft
        .lab_app
        .clone()
        .or_else(|| current_lab_app(running(state, id), &current_site));
    let picker = match super::lab_apps::scan(&state.config_path) {
        Ok(scan) => super::lab_apps::picker(&scan, lab_app.as_deref()),
        Err(error) => Markup::new()
            .markup("<p class=\"notice bad\">The lab apps cannot be read: ")
            .text(error)
            .markup("</p>"),
    };
    Ok(step_page(id, Step::Connection)
        .markup("<form method=\"get\" action=\"")
        .text(Step::Test.path(id))
        .markup("\"><section><h2>2. Connection</h2><p class=\"muted\">Model: <strong>")
        .text(model_name(&list, &key, &path))
        .markup("</strong></p>")
        .fragment(&connection_inputs(state, &path, &current_site, draft))
        .markup("</section><section><h2>Lab app</h2>")
        .fragment(&picker)
        .markup("</section>")
        .fragment(&draft.hidden(&CONNECTION_FIELDS))
        .markup("<div class=\"actions-row\"><button class=\"secondary\" formaction=\"")
        .text(Step::Machine.path(id))
        .markup("\" formnovalidate>Back</button><button>Next: Test</button></div></form>"))
}

fn connection_inputs(
    state: &Context,
    path: &std::path::Path,
    current_site: &toml::Table,
    draft: &Draft,
) -> Markup {
    let current_endpoint = current_site
        .get("endpoint")
        .and_then(toml::Value::as_str)
        .unwrap_or_default();
    match kind_of(path).as_deref() {
        Some("serial") => serial_choices(state, current_site, draft),
        Some(kind @ ("tcp_server" | "tcp_client")) => {
            let (host, port) = current_endpoint.rsplit_once(':').unwrap_or(("", ""));
            let host = host.trim_start_matches('[').trim_end_matches(']');
            Markup::new()
                .markup("<p>")
                .markup(if kind == "tcp_server" {
                    "The machine connects to Analyte. Enter the address of this PC that Analyte listens on (0.0.0.0 means every network card) and the port the machine is set to."
                } else {
                    "Analyte connects to the machine. Enter the machine's IP address and port."
                })
                .markup("</p><label class=\"field\"><span>IP address</span><input name=\"host\" required value=\"")
                .text(draft.host.as_deref().unwrap_or(host))
                .markup("\" maxlength=\"64\"></label><label class=\"field\"><span>Port</span><input name=\"tcp_port\" required value=\"")
                .text(draft.tcp_port.as_deref().unwrap_or(port))
                .markup("\" maxlength=\"5\" inputmode=\"numeric\"></label>")
        }
        _ => Markup::new()
            .markup("<label class=\"field\"><span>Folder</span><input name=\"folder\" required value=\"")
            .text(draft.folder.as_deref().unwrap_or(current_endpoint))
            .markup("\" maxlength=\"1024\"></label>"),
    }
}

/// The detected ports with adapter name and serial number, then a typed name.
/// Without a choice in the draft, the port the site file names is chosen.
fn serial_choices(state: &Context, current_site: &toml::Table, draft: &Draft) -> Markup {
    let current = |key| current_site.get(key).and_then(toml::Value::as_str);
    let mut html = Markup::new().markup(concat!(
        "<p>Choose the port the machine's cable is plugged into. If the USB adapter ",
        "has its own serial number, Analyte saves that number, so it finds the ",
        "machine again even if the port name changes; otherwise it saves the port name.</p><ul class=\"choices\">"
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
                let checked = match draft.port.as_deref() {
                    Some(chosen) => chosen == value,
                    None => match port.factory_serial() {
                        Some(number) => current("usb_serial") == Some(number),
                        None => current("endpoint") == Some(port.name.as_str()),
                    },
                };
                html.push_fragment(
                    &Markup::new()
                        .markup("<li><label><input type=\"radio\" name=\"port\" value=\"")
                        .text(value)
                        .markup(if checked { "\" checked> " } else { "\"> " })
                        .text(&port.name)
                        .markup(" — ")
                        .text(port.description.as_deref().unwrap_or("unknown adapter"))
                        .markup(" — ")
                        .text(saved)
                        .markup("</label></li>"),
                );
            }
            if ports.is_empty() {
                html.push_markup("<li class=\"notice\">No serial ports were found.</li>");
            }
        }
        Err(error) => html.push_fragment(
            &Markup::new()
                .markup("<li class=\"notice bad\">Listing serial ports failed: ")
                .text(error)
                .markup("</li>"),
        ),
    }
    html.markup("</ul><label class=\"field\"><span>Or type a port name</span><input name=\"port_name\" maxlength=\"1024\" value=\"")
        .text(draft.port_name.as_deref().unwrap_or_default())
        .markup("\" placeholder=\"")
        .text(current("endpoint").unwrap_or_default())
        .markup("\"></label>")
}

/// The chosen connection in plain words, with the exact setting on hover.
pub(super) fn connection_words(link: Link, connection: &site::Connection) -> Markup {
    let words = match (link, connection) {
        (_, site::Connection::UsbSerial(number)) => {
            format!("USB serial adapter with serial number {}", number.as_str())
        }
        (Link::Serial, site::Connection::Endpoint(name)) => format!("Serial port {name}"),
        (Link::TcpServer, site::Connection::Endpoint(address)) => {
            format!("Analyte listens on {address}; the machine connects to it")
        }
        (Link::TcpClient, site::Connection::Endpoint(address)) => {
            format!("Analyte connects to the machine at {address}")
        }
        (Link::Folder, site::Connection::Endpoint(folder)) => format!("Watched folder {folder}"),
    };
    view::exact(&words, &connection.describe())
}

fn render_test(
    state: &Context,
    session: &Session,
    id: &str,
    draft: &Draft,
) -> Result<Markup, SetupError> {
    let plan = plan(state, id, draft)?;
    let link = Link::of(plan.validated.effective().stack());
    let mut html = step_page(id, Step::Test).markup("<section><h2>3. Test the connection</h2>");
    if link == Link::Folder {
        html.push_markup(concat!(
            "<p class=\"notice info\">A watched folder has no connection to listen on, so ",
            "there is nothing to test here. Go on to save.</p></section>"
        ));
    } else {
        html.push_fragment(
            &Markup::new()
                .markup(concat!(
                    "<p>The test only listens: for up to 60 seconds it shows what the machine ",
                    "sends, says what it looks like, and sends nothing back. It cannot use a ",
                    "port the running service is using.</p><p class=\"notice\">What it shows may ",
                    "contain patient data; it appears only on this page and is never stored or ",
                    "logged.</p>"
                ))
                .fragment(&start_test(
                    session,
                    id,
                    draft,
                    "Start the test (listen only)",
                ))
                .markup("</section>"),
        );
    }
    Ok(html.fragment(&nav(
        id,
        draft,
        Step::Connection,
        if link == Link::Folder {
            "<button>Next: Save</button>"
        } else {
            "<button class=\"secondary\">Next: Save</button>"
        },
    )))
}

/// The form that starts (or repeats) the listen-only test with the draft's choices.
fn start_test(session: &Session, id: &str, draft: &Draft, label: &'static str) -> Markup {
    Markup::new()
        .markup("<form method=\"post\" action=\"")
        .text(format!("/setup/{id}/listen"))
        .markup("\"><input type=\"hidden\" name=\"csrf\" value=\"")
        .text(session.csrf.form_value())
        .markup("\">")
        .fragment(&draft.hidden(&[]))
        .markup("<button>")
        .markup(label)
        .markup("</button></form>")
}

/// Back to `back` and Next to the Save step, both carrying the draft.
fn nav(id: &str, draft: &Draft, back: Step, next: &'static str) -> Markup {
    Markup::new()
        .markup("<form method=\"get\" action=\"")
        .text(Step::Save.path(id))
        .markup("\">")
        .fragment(&draft.hidden(&[]))
        .markup("<div class=\"actions-row\"><button class=\"secondary\" formaction=\"")
        .text(back.path(id))
        .markup("\">Back</button>")
        .markup(next)
        .markup("</div></form>")
}

fn render_review(
    state: &Context,
    session: &Session,
    id: &str,
    draft: &Draft,
) -> Result<Markup, SetupError> {
    let plan = plan(state, id, draft)?;
    let link = Link::of(plan.validated.effective().stack());
    let destination = &plan.validated.effective().site().destination;
    Ok(step_page(id, Step::Save)
        .markup("<section><h2>4. Check and save</h2><dl>")
        .fragment(&view::row(
            "Machine",
            None,
            &Markup::new()
                .markup("<strong>")
                .text(id)
                .markup("</strong>"),
        ))
        .fragment(&view::row("Model", None, &Markup::new().text(&plan.model)))
        .fragment(&view::row(
            "Connection",
            None,
            &connection_words(link, &plan.connection),
        ))
        .fragment(&view::row(
            "Send results to",
            None,
            &Markup::new().text(format!(
                "{} ({})",
                destination.name.as_str(),
                destination.base_url.as_str()
            )),
        ))
        .markup(concat!(
            "</dl><p>Saving checks everything again and keeps a copy of the old settings. ",
            "Analyte keeps running the old settings until you restart it.</p></section>",
            "<div class=\"actions-row\"><form method=\"get\" action=\""
        ))
        .text(Step::Test.path(id))
        .markup("\">")
        .fragment(&draft.hidden(&[]))
        .markup("<button class=\"secondary\">Back</button></form><form method=\"post\" action=\"")
        .text(format!("/setup/{id}"))
        .markup("\"><input type=\"hidden\" name=\"csrf\" value=\"")
        .text(session.csrf.form_value())
        .markup("\">")
        .fragment(&draft.hidden(&[]))
        .markup("<button>Save</button></form></div>"))
}

/// POST /setup/{id}: check every choice again, write the files with backups, show what was saved.
pub(in crate::admin) async fn save(
    State(state): State<Arc<Context>>,
    Extension(session): Extension<Arc<Session>>,
    Path(id): Path<String>,
    form: Result<Form<Choice>, axum::extract::rejection::FormRejection>,
) -> Response {
    let Ok(Form(choice)) = form else {
        return refused(&SetupError::Connection(
            "the form was incomplete; start again from the setup page",
        ));
    };
    let draft = Draft::from(choice);
    let worker = state.clone();
    let (machine, kept) = (id.clone(), draft.clone());
    let saved = match tokio::task::spawn_blocking(move || save_blocking(&worker, &machine, &kept))
        .await
        .map_err(SetupError::from)
        .and_then(|r| r)
    {
        Ok(saved) => saved,
        Err(error) => return refused_at(&error, &id, &draft),
    };
    let reason = format!(
        "setup page saved: {}, lab app {} ({}){}",
        saved.connection.describe(),
        saved.lab_app,
        saved.base_url,
        if saved.moved {
            "; its lab app settings moved to lab-apps.toml"
        } else {
            ""
        }
    );
    let audited = audit(&state, &session, "setup-save", &id, reason).await;
    tracing::info!(machine = id.as_str(), "setup page saved a site file");
    let mut html = page("Settings saved")
        .fragment(&indicator(None))
        .markup("<section><dl>")
        .fragment(&view::row(
            "Machine",
            None,
            &Markup::new()
                .markup("<strong>")
                .text(&id)
                .markup("</strong>"),
        ))
        .fragment(&view::row("Model", None, &Markup::new().text(&saved.model)))
        .fragment(&view::row(
            "Connection",
            None,
            &connection_words(saved.link, &saved.connection),
        ))
        .fragment(&view::row(
            "Lab app",
            None,
            &Markup::new().text(format!("{} ({})", saved.lab_app, saved.base_url)),
        ))
        .fragment(&view::row(
            "New settings version",
            Some("New configuration digest"),
            &view::version(&saved.digest),
        ))
        .fragment(&view::row(
            "Running now",
            Some("Running configuration digest"),
            &settings_version(running(&state, &id)),
        ))
        .markup("</dl><ul>");
    if saved.moved {
        html.push_markup(
            "<li>This lab app is now kept on the <a href=\"/setup/lab-apps\">Lab app page</a>; nothing else changed.</li>",
        );
    }
    for backup in &saved.backups {
        html.push_fragment(
            &Markup::new()
                .markup("<li class=\"muted\">Copy of the old settings: ")
                .text(backup.display())
                .markup("</li>"),
        );
    }
    html.push_markup("</ul></section>");
    if let Err(error) = audited {
        html.push_fragment(
            &Markup::new()
                .markup("<p class=\"notice bad\">")
                .text(error)
                .markup("</p>"),
        );
    }
    html.push_fragment(&restart_form(&session));
    done(html)
}

/// The one connection test and the setup choices it was started from.
pub(super) struct Test {
    session: listen::Session,
    machine: String,
    draft: Draft,
    model: Family,
}

/// POST /setup/{id}/listen: start the listen-only test with the draft's choices.
pub(in crate::admin) async fn listen_start(
    State(state): State<Arc<Context>>,
    Path(id): Path<String>,
    form: Result<Form<Choice>, axum::extract::rejection::FormRejection>,
) -> Response {
    let Ok(Form(choice)) = form else {
        return refused(&SetupError::Connection(
            "the form was incomplete; start again from the setup page",
        ));
    };
    let draft = Draft::from(choice);
    let worker = state.clone();
    let (machine, kept) = (id.clone(), draft.clone());
    let target = tokio::task::spawn_blocking(move || {
        let plan = plan(&worker, &machine, &kept)?;
        let model = Family::of(plan.validated.effective().stack());
        listen_target(&worker, &plan).map(|(label, target)| (label, target, model))
    })
    .await
    .map_err(SetupError::from)
    .and_then(|r| r);
    let (label, target, model) = match target {
        Ok(found) => found,
        Err(error) => return refused_at(&error, &id, &draft),
    };
    let session = listen::Session::start(label, target, state.setup.seams.limits);
    match state.setup.listen.lock() {
        Ok(mut slot) => {
            // Dropping the previous test stops it and discards its bytes.
            *slot = Some(Test {
                session,
                machine: id,
                draft,
                model,
            });
        }
        Err(_) => {
            return refused(&SetupError::Connection(
                "the listen test state is unavailable",
            ));
        }
    }
    see_other("/setup/listen")
}

fn see_other(location: &'static str) -> Response {
    let mut response = StatusCode::SEE_OTHER.into_response();
    response
        .headers_mut()
        .insert(header::LOCATION, header::HeaderValue::from_static(location));
    response
}

/// One line of the details: offset, hex bytes and printable text.
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

/// How the test page shows the bytes.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::admin) struct Shown {
    /// `open` keeps the details open across the page's reloads.
    details: Option<String>,
}

/// GET /setup/listen: the verdict first, then the test's state, the bytes under
/// Details, and the way back into the steps; reloads every two seconds while running.
pub(in crate::admin) async fn listen_view(
    State(state): State<Arc<Context>>,
    session: Option<Extension<Arc<Session>>>,
    shown: Result<Query<Shown>, QueryRejection>,
) -> Response {
    let Some(Extension(session)) = session.filter(|Extension(s)| s.authenticated) else {
        return needs_login();
    };
    let open = shown.is_ok_and(|Query(shown)| shown.details.as_deref() == Some("open"));
    let test = match state.setup.listen.lock() {
        Ok(slot) => slot.as_ref().map(|t| {
            (
                t.session.view(),
                t.machine.clone(),
                t.draft.clone(),
                t.model,
            )
        }),
        Err(_) => None,
    };
    let Some((view, machine, draft, model)) = test else {
        return done(
            page("Connection test").markup(
                "<p class=\"notice info\">No test has been started. Start one from a machine's setup steps.</p>",
            ),
        );
    };
    let running = view.end.is_none();
    let title = "Connection test (listen only)";
    let mut html = if running {
        view::refreshing(Tab::Setup, title, Refresh::EveryTwoSeconds)
    } else {
        page(title)
    };
    let (tone, sentence) = match &view.end {
        Some(listen::End::Failed(why)) => {
            (Tone::Problem, format!("The test could not run: {why}."))
        }
        _ => verdict::verdict(view.guess, model),
    };
    let (light, word, status) = listen_status(&view);
    html.push_fragment(
        &indicator(Some(Step::Test))
            .markup("<section><h2>")
            .text(&machine)
            .markup("</h2><p class=\"verdict ")
            .markup(notice(tone))
            .markup("\">")
            .text(sentence)
            .markup("</p><p>")
            .fragment(&pill(light, word))
            .markup(" ")
            .text(&view.label)
            .markup(". ")
            .text(status)
            .markup(" Bytes received: ")
            .text(view.seen)
            .markup("."),
    );
    if view.size_limit_reached {
        html.push_markup(" The 64 KiB display limit was reached, so reading stopped.");
    }
    html.push_markup(concat!(
        "</p><p class=\"muted\">Analyte sends nothing to the machine. What is shown may ",
        "contain patient data: it stays on this page only, is never stored or logged, and ",
        "is discarded when the test ends.</p>"
    ));
    if running && !view.bytes.is_empty() {
        html.push_fragment(
            &Markup::new()
                .markup(if open {
                    "<details class=\"support raw\" open><summary><a href=\"/setup/listen\">"
                } else {
                    "<details class=\"support raw\"><summary><a href=\"/setup/listen?details=open\">"
                })
                .markup("Details: the bytes received, as hex and text</a></summary><pre>")
                .text(dump(&view.bytes))
                .markup("</pre></details>"),
        );
    }
    if running {
        html.push_fragment(
            &Markup::new()
                .markup(concat!(
                    "<form method=\"post\" action=\"/setup/listen/stop\">",
                    "<input type=\"hidden\" name=\"csrf\" value=\""
                ))
                .text(session.csrf.form_value())
                .markup("\"><button class=\"quiet-danger\">Stop the test</button></form>"),
        );
    } else {
        html.push_fragment(&start_test(&session, &machine, &draft, "Test again"));
    }
    html.push_markup("</section>");
    html.push_fragment(&nav(
        &machine,
        &draft,
        Step::Connection,
        "<button>Next: Save</button>",
    ));
    done(html)
}

/// The notice class for a verdict's tone.
const fn notice(tone: Tone) -> &'static str {
    match tone {
        Tone::Good => "notice ok",
        Tone::Attention => "notice",
        Tone::Problem => "notice bad",
        Tone::Neutral => "notice info",
    }
}

/// The listen test's state as a pill colour, a short word and a sentence.
fn listen_status(view: &listen::View) -> (Tone, &'static str, String) {
    match &view.end {
        None if view.closed => (
            Tone::Neutral,
            "Closed",
            format!(
                "The machine closed the connection. Showing for {} more seconds.",
                view.remaining.as_secs()
            ),
        ),
        None if view.connected => (
            Tone::Good,
            "Connected",
            format!("Connected. {} seconds left.", view.remaining.as_secs()),
        ),
        None => (
            Tone::Attention,
            "Waiting",
            format!(
                "Waiting for the connection. {} seconds left.",
                view.remaining.as_secs()
            ),
        ),
        Some(listen::End::TimeLimit) => (
            Tone::Neutral,
            "Ended",
            "The test ended after its time limit; the bytes were discarded.".into(),
        ),
        Some(listen::End::Stopped) => (
            Tone::Neutral,
            "Stopped",
            "The test was stopped; the bytes were discarded.".into(),
        ),
        Some(listen::End::Closed) => (
            Tone::Neutral,
            "Closed",
            "The machine closed the connection; the bytes were discarded.".into(),
        ),
        Some(listen::End::Failed(why)) => (
            Tone::Problem,
            "Failed",
            format!("The test could not run: {why}."),
        ),
    }
}

/// POST /setup/listen/stop: end the test now and discard its bytes.
pub(in crate::admin) async fn listen_stop(
    State(state): State<Arc<Context>>,
    Form(_): Form<Csrf>,
) -> Response {
    if let Ok(slot) = state.setup.listen.lock()
        && let Some(test) = slot.as_ref()
    {
        test.session.stop();
    }
    see_other("/setup/listen")
}

#[cfg(test)]
mod tests {
    use super::{Draft, Step, indicator};

    // Owner request 2026-11-19 (item 3): the indicator lists the four steps,
    // marks the current one and the earlier ones done.
    #[test]
    fn indicator_marks_the_current_step_and_the_done_ones() {
        let html = indicator(Some(Step::Test));
        assert_eq!(
            &*html,
            concat!(
                "<ol class=\"steps\" aria-label=\"Setup steps\">",
                "<li class=\"done\"><span class=\"number\">1</span>Machine</li>",
                "<li class=\"done\"><span class=\"number\">2</span>Connection</li>",
                "<li aria-current=\"step\"><span class=\"number\">3</span>Test</li>",
                "<li><span class=\"number\">4</span>Save</li></ol>"
            )
        );
        let saved = indicator(None);
        assert_eq!(saved.matches("class=\"done\"").count(), 4);
        assert!(!saved.contains("aria-current"));
    }

    // Owner request 2026-11-19 (item 3): hidden fields carry the choices as
    // escaped values, and a step leaves out the ones it shows itself.
    #[test]
    fn hidden_fields_escape_values_and_skip_shown_ones() {
        let draft = Draft {
            definition: Some("example/serial-astm".into()),
            port: Some("usb:\"><script>".into()),
            lab_app: Some("demo".into()),
            ..Draft::default()
        };
        let html = draft.hidden(&["port"]);
        assert_eq!(
            &*html,
            concat!(
                "<input type=\"hidden\" name=\"definition\" value=\"example/serial-astm\">",
                "<input type=\"hidden\" name=\"lab_app\" value=\"demo\">"
            )
        );
        assert!(
            draft
                .hidden(&[])
                .contains("value=\"usb:&quot;&gt;&lt;script&gt;\"")
        );
    }
}
