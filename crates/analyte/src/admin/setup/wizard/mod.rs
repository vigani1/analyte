//! Walks a lab manager through setting up one machine, one step per screen: 1 Machine, 2 Connection, 3 Test, 4 Save.
//!
//! No script runs, so each screen carries the earlier choices forward as hidden
//! form fields ([`Draft`]). They are never trusted: every step checks all the
//! earlier choices again against the files, and Save runs the whole check once
//! more under the write lock ([`super::plan`]). The fields hold no secret: keys
//! are set on the Lab apps page. Steps 1 to 4 only read; the connection test and
//! Save are posts with the session's CSRF value, so that value never reaches a
//! URL. Hidden fields beat a server-side draft here: there is nothing to store,
//! bound, expire or lose at a restart, and Back works in every tab.
use super::{
    Csrf, SetupError, Shipped, audit, copies, definition_name, definitions, lab_apps,
    level_and_state, needs_login, off_thread,
    plan::{
        Choice as Posted, KEEP, chosen, current_lab_app, holder, kind_of, listen_target, model_of,
        plan, save_blocking,
    },
    refused, running, see_other, serial, settings_version, site,
};
use crate::admin::{
    server::{Connection, Context, sessions::Session},
    view::{
        Tab, Tone, format,
        frame::Frame,
        parts::Hover,
        report::{Button, Report, Section, Status, respond},
    },
};
use crate::machine::link::status::{Light, Link};
use analyte_config::{Endpoint, TransportKind};
use askama::Template as _;
use axum::{
    Extension, Form,
    extract::{Path, Query, State, rejection::QueryRejection},
    http::StatusCode,
    response::Response,
    routing::{MethodRouter, get},
};
use jiff::Timestamp;
use page::{
    Accepts, Choice, ConnectionStep, Foot, Hidden, LabApp, Listen, Live, Mark, ModelStep, Outcome,
    Picker, Port, SaveStep, Sends, Start, Summary, TestStep, Tested, Wizard,
};
use serde::Deserialize;
use std::sync::Arc;
use verdict::{Family, Route};

pub(super) mod listen;
mod page;
pub(super) mod verdict;

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
    /// The only devices a machine that connects to this PC may connect
    /// from, typed in step 2 or ticked in step 4 (ADR 0026); empty accepts any.
    pub(super) allowed_peers: Option<String>,
}

/// The fields step 2 shows itself; every other step carries them hidden.
const CONNECTION_FIELDS: [&str; 7] = [
    "port",
    "port_name",
    "host",
    "tcp_port",
    "folder",
    "lab_app",
    PEERS,
];

/// The field naming the devices a listening machine accepts (ADR 0026).
const PEERS: &str = "allowed_peers";

impl Draft {
    fn fields(&self) -> [(&'static str, Option<&str>); 8] {
        [
            ("definition", self.definition.as_deref()),
            ("port", self.port.as_deref()),
            ("port_name", self.port_name.as_deref()),
            ("host", self.host.as_deref()),
            ("tcp_port", self.tcp_port.as_deref()),
            ("folder", self.folder.as_deref()),
            ("lab_app", self.lab_app.as_deref()),
            (PEERS, self.allowed_peers.as_deref()),
        ]
    }

    /// Every choice made, except the ones the step shows, as hidden fields.
    fn hidden(&self, shown: &[&str]) -> Hidden {
        self.fields()
            .into_iter()
            .filter(|(name, _)| !shown.contains(name))
            .filter_map(|(name, value)| value.map(|value| (name, value.to_owned())))
            .collect()
    }

    /// Whether `other` holds the same choices for the connection test; an
    /// empty field counts as not chosen, as every step reads it. The allowed
    /// devices do not change what the test listens on, so step 4 still shows
    /// the test after they are ticked or typed.
    fn same(&self, other: &Self) -> bool {
        let chosen = |draft: &Self| -> Vec<(&'static str, String)> {
            draft
                .hidden(&[PEERS])
                .into_iter()
                .filter(|(_, value)| !value.trim().is_empty())
                .collect()
        };
        chosen(self) == chosen(other)
    }

    /// Every choice made, as a URL query that opens a step with them.
    fn query(&self) -> String {
        self.hidden(&[])
            .iter()
            .map(|(name, value)| format!("{name}={}", format::path_segment(value)))
            .collect::<Vec<_>>()
            .join("&")
    }
}

impl From<Posted> for Draft {
    fn from(choice: Posted) -> Self {
        Self {
            definition: choice.definition,
            port: choice.port,
            port_name: choice.port_name,
            host: choice.host,
            tcp_port: choice.tcp_port,
            folder: choice.folder,
            lab_app: choice.lab_app,
            allowed_peers: choice.allowed_peers,
        }
    }
}

/// One screen of the setup steps, in step order; each is a GET with the draft as the query.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::admin) enum Step {
    /// GET /setup/{id}: step 1, the machine model.
    Machine,
    /// GET /setup/{id}/connection: step 2, the port or address and the lab app.
    Connection,
    /// GET /setup/{id}/test: step 3, the listen-only connection test.
    Test,
    /// GET /setup/{id}/save: step 4, the summary before saving.
    Save,
}

impl Step {
    const ALL: [Self; 4] = [Self::Machine, Self::Connection, Self::Test, Self::Save];

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

    /// The button that goes back to this step to fix a refused choice.
    const fn fix(self) -> &'static str {
        match self {
            Self::Machine => "Go to step 1: Machine",
            Self::Connection => "Go to step 2: Connection",
            Self::Test => "Go to step 3: Test",
            Self::Save => "Go to step 4: Save",
        }
    }

    /// The step's page; GET, with the draft as the query.
    fn path(self, id: &str) -> String {
        format::setup_href(
            id,
            match self {
                Self::Machine => "",
                Self::Connection => "connection",
                Self::Test => "test",
                Self::Save => "save",
            },
        )
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
            | SetupError::IncompleteForm(_)
            | SetupError::ProgramFile(_)
            | SetupError::ProgramLayout(_)
            | SetupError::SiteFile(_)
            | SetupError::SiteDestination(_)
            | SetupError::SiteText(_)
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
            | SetupError::ListenState
            | SetupError::Busy => None,
        }
    }
}

/// The stepper: every step by number and name, `current` marked and the
/// earlier ones done. `None` marks all four done, after a save.
fn marks(current: Option<Step>) -> Vec<Mark> {
    Step::ALL
        .into_iter()
        .map(|step| Mark {
            number: step.number(),
            label: step.label(),
            done: current.is_none_or(|current| step < current),
            current: current == Some(step),
        })
        .collect()
}

/// What every step shows around its form; `current` is none after a save.
fn wizard(id: &str, model: Option<String>, current: Option<Step>) -> Wizard {
    Wizard {
        id: id.to_owned(),
        model,
        steps: marks(current),
        count: format!("Step {} of 4", current.unwrap_or(Step::Save).number()),
        cancel: current.is_some(),
    }
}

/// The frame of a step about machine `id`.
fn frame(state: &Context, session: &Session, id: &str) -> Frame {
    Section::Setup(Some(id)).frame(state, format!("Set up {id}"), Some(session))
}

/// A refusal that names the step to fix and links back to it with every choice kept.
pub(super) fn refused_at(
    state: &Context,
    session: &Session,
    error: &SetupError,
    id: &str,
    draft: &Draft,
) -> Response {
    let Some(step) = Step::to_fix(error) else {
        return refused(state, Some(session), Some(id), error);
    };
    let section = Section::Setup(Some(id));
    let title = "Nothing was changed";
    let page = Report {
        lines: vec![format!(
            "Fix this in step {}: {}.",
            step.number(),
            step.label()
        )],
        links: vec![
            Button {
                href: format!("{}?{}", step.path(id), draft.query()),
                label: step.fix(),
                class: "btn--primary",
            },
            Button {
                href: Tab::Setup.href().to_owned(),
                label: "Back to Setup",
                class: "btn--outline",
            },
        ],
        ..Report::new(
            section.frame(state, title, Some(session)),
            section,
            Tone::Problem,
            "Setup could not continue",
            format::sentence(&error.to_string()),
        )
    };
    respond(
        state,
        Some(session),
        section,
        StatusCode::CONFLICT,
        page.render(),
    )
}

/// A step's page, or why its template could not be written.
type Rendered = Result<String, askama::Error>;

type Render = fn(&Context, &Session, &str, &Draft) -> Result<Rendered, SetupError>;

/// Serves one step: signed in only; files are read off the async workers; a
/// choice that does not pass is refused with the step to fix.
async fn serve(
    state: Arc<Context>,
    session: Option<Extension<Arc<Session>>>,
    id: String,
    query: Result<Query<Draft>, QueryRejection>,
    render: Render,
) -> Response {
    let Some(Extension(session)) = session.filter(|Extension(s)| s.is_signed_in()) else {
        return needs_login(&state);
    };
    let Ok(Query(draft)) = query else {
        return refused(
            &state,
            Some(&session),
            Some(&id),
            &SetupError::IncompleteForm(START_AGAIN),
        );
    };
    let (machine, kept, owner) = (id.clone(), draft.clone(), session.clone());
    let rendered = off_thread(&state, move |state| render(state, &owner, &machine, &kept)).await;
    match rendered {
        Ok(html) => respond(
            &state,
            Some(&session),
            Section::Setup(Some(&id)),
            StatusCode::OK,
            html,
        ),
        Err(error) => refused_at(&state, &session, &error, &id, &draft),
    }
}

/// The GET handler of `step`, for the router.
pub(in crate::admin) fn step(step: Step) -> MethodRouter<Arc<Context>> {
    let render: Render = match step {
        Step::Machine => render_machine,
        Step::Connection => render_connection,
        Step::Test => render_test,
        Step::Save => render_review,
    };
    get(
        move |State(state): State<Arc<Context>>,
              session: Option<Extension<Arc<Session>>>,
              Path(id): Path<String>,
              query: Result<Query<Draft>, QueryRejection>| {
            serve(state, session, id, query, render)
        },
    )
}

fn render_machine(
    state: &Context,
    session: &Session,
    id: &str,
    draft: &Draft,
) -> Result<Rendered, SetupError> {
    let (_, installed) = site::installed(state.setup.config(), id)?;
    let list = definitions(&state.setup.seams)?;
    let current = match chosen(&list, None, &installed) {
        Ok(current) => Some(current),
        Err(SetupError::NoDefinitionYet) => None,
        Err(error) => return Err(error),
    };
    let selected = draft
        .definition
        .clone()
        .or_else(|| current.as_ref().map(|model| model.form_value().to_owned()))
        .unwrap_or_default();
    let plain = |shown: String| Hover {
        shown,
        exact: String::new(),
    };
    let mut choices: Vec<Choice> = current
        .as_ref()
        .filter(|model| model.key.is_none())
        .map(|kept| Choice {
            value: KEEP.to_owned(),
            title: plain("Keep the current model".into()),
            meta: plain(kept.path.display().to_string()),
            status: None,
            checked: selected == KEEP,
            disabled: false,
        })
        .into_iter()
        .collect();
    choices.extend(list.iter().map(|definition| {
        let (checked, status, icon) = level_and_state(definition);
        Choice {
            value: definition.key.clone(),
            title: definition_name(definition),
            meta: checked,
            status: Some((status, icon)),
            checked: definition.key == selected,
            disabled: !matches!(definition.state, Shipped::Ready { .. }),
        }
    }));
    Ok(ModelStep {
        frame: frame(state, session, id),
        wizard: wizard(id, current.map(|model| model.name), Some(Step::Machine)),
        action: Step::Connection.path(id),
        hidden: draft.hidden(&["definition"]),
        choices,
    }
    .render())
}

fn render_connection(
    state: &Context,
    session: &Session,
    id: &str,
    draft: &Draft,
) -> Result<Rendered, SetupError> {
    let (_, installed) = site::installed(state.setup.config(), id)?;
    let list = definitions(&state.setup.seams)?;
    let model = model_of(&list, draft, &installed)?;
    let current_site = crate::installation::config::read(&installed.site)
        .ok()
        .and_then(|text| text.parse::<toml::Table>().ok())
        .unwrap_or_default();
    let lab_app = draft
        .lab_app
        .clone()
        .or_else(|| current_lab_app(running(state, id), &current_site));
    let lab_apps = match lab_apps::scan(state.setup.config()) {
        Ok(scan) => picker(&scan, lab_app.as_deref()),
        Err(error) => Picker::Unreadable(format::sentence(&error.to_string())),
    };
    let (how, inputs) = connection_inputs(state, id, &model.path, &current_site, draft);
    Ok(ConnectionStep {
        frame: frame(state, session, id),
        wizard: wizard(id, Some(model.name), Some(Step::Connection)),
        action: Step::Test.path(id),
        back: Step::Machine.path(id),
        hidden: draft.hidden(&CONNECTION_FIELDS),
        how,
        inputs,
        lab_apps,
    }
    .render())
}

/// The lab app picker: the known lab apps with `current` (or the only one) chosen.
fn picker(scan: &lab_apps::Scan, current: Option<&str>) -> Picker {
    if scan.known.is_empty() {
        return Picker::None;
    }
    let chosen = current
        .filter(|name| scan.get(name).is_some())
        .map(str::to_owned)
        .or_else(|| match scan.known.as_slice() {
            [only] => Some(only.settings.name.as_str().to_owned()),
            _ => None,
        });
    Picker::Choose {
        apps: scan
            .known
            .iter()
            .map(|known| LabApp {
                name: known.settings.name.as_str().to_owned(),
                address: known.settings.base_url.as_str().to_owned(),
                selected: chosen.as_deref() == Some(known.settings.name.as_str()),
            })
            .collect(),
        chosen: chosen.is_some(),
    }
}

/// How the model connects in one sentence, and the fields its transport asks for.
fn connection_inputs(
    state: &Context,
    id: &str,
    path: &std::path::Path,
    current_site: &toml::Table,
    draft: &Draft,
) -> (&'static str, page::Inputs) {
    let current_endpoint = current_site
        .get("endpoint")
        .and_then(toml::Value::as_str)
        .unwrap_or_default();
    match kind_of(path) {
        Some(TransportKind::Serial) => (
            "It connects by a serial cable or a USB serial adapter.",
            serial_choices(state, id, current_site, draft),
        ),
        Some(kind @ (TransportKind::TcpServer | TransportKind::TcpClient)) => {
            let (host, port) = current_endpoint.rsplit_once(':').unwrap_or(("", ""));
            let host = draft
                .host
                .as_deref()
                .unwrap_or_else(|| host.trim_start_matches('[').trim_end_matches(']'))
                .to_owned();
            let port = draft.tcp_port.as_deref().unwrap_or(port).to_owned();
            if kind == TransportKind::TcpServer {
                (
                    concat!(
                        "The machine connects to this PC over the lab network. Choose the ",
                        "address Analyte listens on, then type that address and the port into ",
                        "the machine’s host (LIS) settings."
                    ),
                    page::Inputs::Listens {
                        hosts: (state.setup.seams.addresses)()
                            .map(|found| host_choices(&found, &host))
                            .map_err(|error| format::sentence(&error.to_string())),
                        typed: host,
                        port,
                        peers: draft
                            .allowed_peers
                            .clone()
                            .unwrap_or_else(|| site_peers(current_site)),
                    },
                )
            } else {
                (
                    concat!(
                        "Analyte connects to the machine over the lab network. Type the address ",
                        "and port shown in the machine’s own network or host (LIS) settings."
                    ),
                    page::Inputs::Connects { host, port },
                )
            }
        }
        Some(TransportKind::WatchedFolder) | None => (
            "Analyte reads its results from a folder.",
            page::Inputs::Folder(
                draft
                    .folder
                    .as_deref()
                    .unwrap_or(current_endpoint)
                    .to_owned(),
            ),
        ),
    }
}

/// The devices the current site file lets connect, as step 2 shows them:
/// "192.168.1.50, 192.168.1.51"; empty when it names none.
fn site_peers(current_site: &toml::Table) -> String {
    current_site
        .get(PEERS)
        .and_then(toml::Value::as_array)
        .map(|addresses| {
            addresses
                .iter()
                .filter_map(toml::Value::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default()
}

/// The addresses Analyte can listen on: this PC's own, then every network
/// card, with `chosen` checked. A chosen address the list lacks (saved before,
/// or this PC only) comes first, so it stays visible and can be kept.
pub(super) fn host_choices(found: &[super::Address], chosen: &str) -> Vec<page::Host> {
    let chosen = chosen.trim();
    let every = std::net::Ipv4Addr::UNSPECIFIED.to_string();
    let mut hosts = Vec::with_capacity(found.len().saturating_add(2));
    let listed = chosen == every || found.iter().any(|a| a.ip.to_string() == chosen);
    if !chosen.is_empty() && !listed {
        let loopback = chosen
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback());
        hosts.push(page::Host {
            value: chosen.to_owned(),
            address: chosen.to_owned(),
            meta: if loopback {
                "this PC only: no machine on the network can reach it".into()
            } else {
                "saved before; not an address of this PC now".into()
            },
            checked: true,
        });
    }
    hosts.extend(found.iter().map(|address| {
        let ip = address.ip.to_string();
        page::Host {
            checked: ip == chosen,
            value: ip.clone(),
            address: ip,
            meta: address.adapter.clone(),
        }
    }));
    hosts.push(page::Host {
        checked: chosen == every,
        value: every.clone(),
        address: every,
        meta: "every network card of this PC".into(),
    });
    hosts
}

/// The detected ports with adapter name and serial number, then a typed name.
/// Without a choice in the draft, the port the site file names is chosen. A
/// port another running machine uses cannot be chosen.
fn serial_choices(
    state: &Context,
    id: &str,
    current_site: &toml::Table,
    draft: &Draft,
) -> page::Inputs {
    let current = |key| current_site.get(key).and_then(toml::Value::as_str);
    let ports = (state.setup.seams.ports)()
        .map(|ports| {
            let listed: Vec<serial::Listed> =
                ports.iter().take(serial::LIST_LIMIT).cloned().collect();
            listed
                .iter()
                .map(|port| {
                    let (value, endpoint, saved) = match port.factory_serial() {
                        Some(number) => (
                            format!("usb:{number}"),
                            analyte_config::UsbSerial::new(number.into()).map_or_else(
                                |_| Endpoint::Fixed(port.name.clone()),
                                Endpoint::UsbSerial,
                            ),
                            format!("remembered by its serial number {number}"),
                        ),
                        None => (
                            format!("name:{}", port.name),
                            Endpoint::Fixed(port.name.clone()),
                            "no serial number; remembered by its port name".into(),
                        ),
                    };
                    let checked = match draft.port.as_deref() {
                        Some(chosen) => chosen == value,
                        None => match port.factory_serial() {
                            Some(number) => current("usb_serial") == Some(number),
                            None => current("endpoint") == Some(port.name.as_str()),
                        },
                    };
                    let used_by = holder(&state.setup, &endpoint, Some(&port.name), &|| {
                        Ok(listed.clone())
                    })
                    .filter(|holder| holder.as_str() != id)
                    .map(|holder| holder.as_str().to_owned());
                    Port {
                        meta: format!(
                            "{} – {}",
                            port.description.as_deref().unwrap_or("unknown adapter"),
                            used_by
                                .as_ref()
                                .map_or(saved, |holder| format!("used by {holder}"))
                        ),
                        value,
                        name: port.name.clone(),
                        checked: checked && used_by.is_none(),
                        used_by,
                    }
                })
                .collect()
        })
        .map_err(|error| format::sentence(&format!("listing serial ports failed: {error}")));
    page::Inputs::Serial {
        ports,
        typed: draft.port_name.clone().unwrap_or_default(),
        placeholder: current("endpoint").unwrap_or_default().to_owned(),
    }
}

/// The chosen connection in plain words, with the exact setting on hover.
pub(super) fn connection_words(link: Link, connection: &Endpoint) -> Hover {
    let shown = match (link, connection) {
        (_, Endpoint::UsbSerial(number)) => {
            format!("USB serial adapter with serial number {}", number.as_str())
        }
        (Link::Serial, Endpoint::Fixed(name)) => format!("Serial port {name}"),
        (Link::TcpServer, Endpoint::Fixed(address)) => {
            format!("Analyte listens on {address}; the machine connects to it")
        }
        (Link::TcpClient, Endpoint::Fixed(address)) => {
            format!("Analyte connects to the machine at {address}")
        }
        (Link::Folder, Endpoint::Fixed(folder)) => format!("Watched folder {folder}"),
    };
    Hover {
        shown,
        exact: site::describe(connection),
    }
}

fn render_test(
    state: &Context,
    session: &Session,
    id: &str,
    draft: &Draft,
) -> Result<Rendered, SetupError> {
    let plan = plan(&state.setup, id, draft)?;
    let link = Link::of(plan.validated.effective().stack());
    let folder = link == Link::Folder;
    let start = (!folder).then(|| {
        start(
            state,
            session,
            id,
            draft,
            (
                connection_words(link, &plan.connection),
                link == Link::TcpClient,
            ),
            "Start the test",
        )
    });
    Ok(TestStep {
        frame: frame(state, session, id),
        wizard: wizard(id, Some(plan.model), Some(Step::Test)),
        foot: foot(id, draft, folder),
        start,
    }
    .render())
}

/// The form that starts (or repeats) the listen-only test with the draft's
/// choices; `target` is what it listens on and whether Analyte connects instead.
fn start(
    state: &Context,
    session: &Session,
    id: &str,
    draft: &Draft,
    (target, connects): (Hover, bool),
    label: &'static str,
) -> Start {
    Start {
        action: format::setup_href(id, "listen"),
        csrf: session.csrf.form_value().to_owned(),
        hidden: draft.hidden(&[]),
        target,
        connects,
        limit: state.setup.seams.limits.duration.as_secs(),
        label,
    }
}

/// Back to step 2 and Next to step 4, both carrying the draft.
fn foot(id: &str, draft: &Draft, primary: bool) -> Foot {
    Foot {
        back: Step::Connection.path(id),
        next: Step::Save.path(id),
        hidden: draft.hidden(&[]),
        primary,
    }
}

fn render_review(
    state: &Context,
    session: &Session,
    id: &str,
    draft: &Draft,
) -> Result<Rendered, SetupError> {
    let plan = plan(&state.setup, id, draft)?;
    let link = Link::of(plan.validated.effective().stack());
    let destination = &plan.validated.effective().site().destination;
    let summary = Summary {
        connection: connection_words(link, &plan.connection),
        accepts: (link == Link::TcpServer)
            .then(|| offer(plan.peers.as_ref(), seen(state, id, draft))),
        lab_app: sends(
            destination,
            lab_apps::missing_keys(&state.setup, destination),
        ),
        test: tested(state, id, draft, link),
        versions: None,
        model: plan.model.clone(),
    };
    Ok(SaveStep {
        frame: frame(state, session, id),
        wizard: wizard(id, Some(plan.model), Some(Step::Save)),
        summary,
        outcome: Outcome::Review {
            back: Step::Test.path(id),
            back_hidden: draft.hidden(&[]),
            action: format::setup_href(id, ""),
            csrf: session.csrf.form_value().to_owned(),
            // The allowed devices come from the step's tick box instead.
            hidden: draft.hidden(&[PEERS]),
        },
    }
    .render())
}

/// What step 4 offers about the devices a listening machine accepts (ADR
/// 0026): the typed addresses, or else the device the connection test saw,
/// ticked by default; with neither, a warning that any device is accepted.
fn offer(typed: Option<&analyte_config::AllowedPeers>, seen: Option<std::net::IpAddr>) -> Accepts {
    match (typed, seen) {
        (Some(typed), seen) => Accepts::Offer {
            value: typed.to_string(),
            label: if typed.addresses().len() == 1 {
                format!("Only accept this device: {typed}")
            } else {
                format!("Only accept these devices: {typed}")
            },
            note: seen.filter(|seen| !typed.allows(*seen)).map(|seen| {
                format!("The connection test saw {seen}, which is not on this list.")
            }),
        },
        (None, Some(seen)) => Accepts::Offer {
            value: seen.to_string(),
            label: format!("Only accept this device: {seen}"),
            note: Some(
                "The device that connected during the test. Check that this is the machine’s own address in its network settings."
                    .into(),
            ),
        },
        (None, None) => Accepts::Any,
    }
}

/// The device the last connection test for `id` saw connect, when it ran
/// with exactly these choices and Analyte listened.
fn seen(state: &Context, id: &str, draft: &Draft) -> Option<std::net::IpAddr> {
    let slot = state.setup.listen.lock().ok()?;
    slot.as_ref()
        .filter(|test| test.machine == id && test.draft.same(draft))
        .and_then(|test| test.session.view().peer)
}

/// The lab app the machine sends to, with the keys it still lacks.
fn sends(destination: &analyte_config::Destination, missing: Option<String>) -> Sends {
    Sends {
        name: destination.name.as_str().to_owned(),
        href: format::lab_app_href(destination.name.as_str()),
        address: destination.base_url.as_str().to_owned(),
        missing,
    }
}

/// The verdict of the last connection test for `id`, when it ran with exactly
/// these choices; a watched folder has nothing to test.
fn tested(state: &Context, id: &str, draft: &Draft, link: Link) -> Tested {
    if link == Link::Folder {
        return Tested::Folder;
    }
    let Ok(slot) = state.setup.listen.lock() else {
        return Tested::NotRun;
    };
    match slot
        .as_ref()
        .filter(|test| test.machine == id && test.draft.same(draft))
    {
        Some(test) => {
            let view = test.session.view();
            let verdict = verdict::of_view(&view, test.family, &test.route);
            Tested::Seen {
                tone: verdict.tone,
                title: verdict.title,
                when: format::moment(view.started, Timestamp::now()),
            }
        }
        None => Tested::NotRun,
    }
}

/// A posted step (the test or Save), or why its form could not be read.
type PostedForm = Result<Form<Posted>, axum::extract::rejection::FormRejection>;

/// What to do after a step's form could not be read.
const START_AGAIN: &str = "start again from the setup page";

/// The refusal of a step's form that could not be read.
fn unread(state: &Context, session: &Session, id: &str) -> Response {
    refused(
        state,
        Some(session),
        Some(id),
        &SetupError::IncompleteForm(START_AGAIN),
    )
}

/// POST /setup/{id}: check every choice again, write the files with backups, show what was saved.
pub(in crate::admin) async fn save(
    State(state): State<Arc<Context>>,
    Extension(session): Extension<Arc<Session>>,
    Path(id): Path<String>,
    form: PostedForm,
) -> Response {
    let Ok(Form(choice)) = form else {
        return unread(&state, &session, &id);
    };
    let draft = Draft::from(choice);
    let (machine, kept) = (id.clone(), draft.clone());
    let saved = off_thread(&state, move |state| {
        let saved = save_blocking(&state.setup, &machine, &kept)?;
        let missing = lab_apps::missing_keys(&state.setup, &saved.lab_app);
        Ok((saved, missing))
    })
    .await;
    let (saved, missing) = match saved {
        Ok(saved) => saved,
        Err(error) => return refused_at(&state, &session, &error, &id, &draft),
    };
    let reason = saved.reason(super::Via::Page);
    let audited = audit(&state, &session, "setup-save", &id, reason).await;
    tracing::info!(machine = id.as_str(), "setup page saved a site file");
    let mut lines = Vec::new();
    if saved.moved {
        lines.push(
            "This lab app is now kept on the Lab apps page; nothing else changed.".to_owned(),
        );
    }
    if !saved.backups.is_empty() {
        lines.push("A dated copy of the old settings was kept.".into());
    }
    let page = SaveStep {
        frame: frame(&state, &session, &id),
        wizard: wizard(&id, Some(saved.model.clone()), None),
        summary: Summary {
            connection: connection_words(saved.link, &saved.connection),
            accepts: (saved.link == Link::TcpServer).then(|| {
                Accepts::Saved(
                    saved.peers.as_ref().map_or_else(
                        || format::OPEN_TO_ANY.into(),
                        |peers| format!("Only {peers}"),
                    ),
                    saved.peers.is_none(),
                )
            }),
            lab_app: sends(&saved.lab_app, missing),
            test: tested(&state, &id, &draft, saved.link),
            versions: Some((
                Hover {
                    shown: format::digest_short(&saved.digest),
                    exact: format::digest_hex(&saved.digest),
                },
                settings_version(running(&state, &id)),
            )),
            model: saved.model,
        },
        outcome: Outcome::Saved {
            csrf: session.csrf.form_value().to_owned(),
            lines,
            problems: audited
                .err()
                .map(|error| format::sentence(&error.to_string()))
                .into_iter()
                .collect(),
            support: copies(&saved.backups),
            done: format::machine_href(&id),
        },
    };
    respond(
        &state,
        Some(&session),
        Section::Setup(Some(&id)),
        StatusCode::OK,
        page.render(),
    )
}

/// The one connection test and the setup choices it was started from.
pub(super) struct Test {
    session: listen::Session,
    machine: String,
    draft: Draft,
    family: Family,
    /// The model's name, beside the machine's name on the test page.
    model: String,
    /// What it listens on, for the words Analyte uses for a machine's link.
    connection: Connection,
    /// What it listens on, in words with the exact setting on hover.
    target: Hover,
    /// How it reaches the machine, for what to check when nothing arrives.
    route: Route,
}

/// POST /setup/{id}/listen: start the listen-only test with the draft's choices.
pub(in crate::admin) async fn listen_start(
    State(state): State<Arc<Context>>,
    Path(id): Path<String>,
    Extension(session): Extension<Arc<Session>>,
    form: PostedForm,
) -> Response {
    let Ok(Form(choice)) = form else {
        return unread(&state, &session, &id);
    };
    let draft = Draft::from(choice);
    let (machine, choices) = (id.clone(), draft.clone());
    let target = off_thread(&state, move |state| {
        let plan = plan(&state.setup, &machine, &choices)?;
        let stack = plan.validated.effective().stack();
        let (family, link) = (Family::of(stack), Link::of(stack));
        let (label, target) = listen_target(&state.setup, &plan)?;
        let route = verdict::route_of(&state.setup, &target);
        Ok((label, target, route, plan, family, link))
    })
    .await;
    let (label, target, route, plan, family, link) = match target {
        Ok(found) => found,
        Err(error) => return refused_at(&state, &session, &error, &id, &draft),
    };
    let test = Test {
        session: listen::Session::start(label, target, state.setup.seams.limits),
        machine: id.clone(),
        draft,
        family,
        connection: Connection::of(link, &plan.connection),
        target: connection_words(link, &plan.connection),
        route,
        model: plan.model,
    };
    match state.setup.listen.lock() {
        // Dropping the previous test stops it and discards its bytes.
        Ok(mut slot) => *slot = Some(test),
        Err(_) => return refused(&state, Some(&session), Some(&id), &SetupError::ListenState),
    }
    see_other("/setup/listen")
}

/// GET /setup/listen: the test's state, the bytes counted, their framing
/// characters and the verdict, never the bytes themselves; reloads every two
/// seconds while running, then offers Test again. Next goes on to step 4.
pub(in crate::admin) async fn listen_view(
    State(state): State<Arc<Context>>,
    session: Option<Extension<Arc<Session>>>,
) -> Response {
    let Some(Extension(session)) = session.filter(|Extension(s)| s.is_signed_in()) else {
        return needs_login(&state);
    };
    let now = Timestamp::now();
    let page = match state.setup.listen.lock() {
        Ok(slot) => slot
            .as_ref()
            .map(|test| listen_page(&state, &session, test, now)),
        // A test thread panicked while holding the state; what it held is unknown.
        Err(_) => return refused(&state, Some(&session), None, &SetupError::ListenState),
    };
    let section = Section::Setup(None);
    let Some(page) = page else {
        let title = "Connection test";
        let report = Report::new(
            section.frame(&state, title, Some(&session)),
            section,
            Tone::Neutral,
            "No test has been started",
            "Start one from a machine’s setup steps, in step 3.",
        );
        return respond(
            &state,
            Some(&session),
            section,
            StatusCode::OK,
            report.render(),
        );
    };
    respond(&state, Some(&session), section, StatusCode::OK, page)
}

/// The test page for `test` as it stands `now`.
fn listen_page(state: &Context, session: &Session, test: &Test, now: Timestamp) -> Rendered {
    let view = test.session.view();
    let limit = state.setup.seams.limits.duration.as_secs();
    let remaining = view.remaining.as_secs();
    let running = view.end.is_none();
    let mut received = format!(
        "{} received",
        format::amount(
            u64::try_from(view.seen).unwrap_or(u64::MAX),
            "byte",
            "bytes"
        )
    );
    if view.size_limit_reached {
        received.push_str(&format!(
            " · reading stopped at the {} limit",
            format::bytes(u64::try_from(state.setup.seams.limits.bytes).unwrap_or(u64::MAX))
        ));
    }
    let id = test.machine.as_str();
    Listen {
        frame: frame(state, session, id),
        wizard: wizard(id, Some(test.model.clone()), Some(Step::Test)),
        foot: foot(id, &test.draft, true),
        live: Live {
            target: view.label.clone(),
            peer: view.peer.map(|peer| peer.to_string()),
            connects: matches!(test.route, Route::Connects(_)),
            clock: running.then(|| format::countdown(remaining)),
            elapsed: limit.saturating_sub(remaining),
            limit,
            received,
            framing: vec![if view.framing.is_empty() {
                "None yet".to_owned()
            } else {
                format::control_names(&view.framing)
            }],
            state: test_state(&view, &test.connection, id, now),
            verdict: verdict::of_view(&view, test.family, &test.route),
            stop: running.then(|| session.csrf.form_value().to_owned()),
            again: (!running).then(|| {
                start(
                    state,
                    session,
                    id,
                    &test.draft,
                    (
                        test.target.clone(),
                        matches!(test.route, Route::Connects(_)),
                    ),
                    "Test again",
                )
            }),
        },
    }
    .render()
}

/// The test's state as a status, with what Analyte observed in the words it
/// uses for a machine's link: "Listening on port 5000, nothing connected since
/// 10:41", "Port COM3 open since 10:41".
fn test_state(view: &listen::View, connection: &Connection, id: &str, now: Timestamp) -> Status {
    let link = |light, since: Timestamp| {
        format::link_sentence(connection, light, &format::clock(since, now), id)
    };
    let (tone, word, note) = match &view.end {
        None if view.closed => (
            Tone::Neutral,
            "Closed",
            format!(
                "The machine closed the connection. The test ends in {}.",
                format::duration(view.remaining.as_secs())
            ),
        ),
        None => match view.connected {
            Some(at) => (Tone::Good, "Connected", link(Light::Live, at)),
            None => (
                Tone::Attention,
                "Waiting",
                link(Light::Waiting, view.started),
            ),
        },
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
        Some(listen::End::Failed(_)) => (
            Tone::Problem,
            "Failed",
            match view.connected {
                Some(_) => "Reading failed; the bytes were discarded.".into(),
                None => link(Light::Off, view.started),
            },
        ),
    };
    Status {
        tone,
        icon: tone.icon(),
        word: word.to_owned(),
        exact: String::new(),
        note: Some(note),
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
    use super::{Draft, Step, marks};

    // Owner request 2026-11-19 (item 3): the stepper lists the four steps,
    // marks the current one and the earlier ones done; after a save all four
    // are done and none is current.
    #[test]
    fn stepper_marks_the_current_step_and_the_done_ones() {
        let shown: Vec<(&str, &str, bool, bool)> = marks(Some(Step::Test))
            .iter()
            .map(|mark| (mark.number, mark.label, mark.done, mark.current))
            .collect();
        assert_eq!(
            shown,
            [
                ("1", "Machine", true, false),
                ("2", "Connection", true, false),
                ("3", "Test", false, true),
                ("4", "Save", false, false),
            ]
        );
        let saved = marks(None);
        assert!(saved.iter().all(|mark| mark.done && !mark.current));
    }

    // Owner request 2026-11-19 (item 3): hidden fields carry the choices, a
    // step leaves out the ones it shows itself, and a refusal's link carries
    // them all as an encoded query.
    #[test]
    fn hidden_fields_skip_shown_ones_and_links_encode_them() {
        let draft = Draft {
            definition: Some("example/serial-astm".into()),
            port: Some("usb:\"><script>".into()),
            lab_app: Some("demo".into()),
            ..Draft::default()
        };
        assert_eq!(
            draft.hidden(&["port"]),
            [
                ("definition", "example/serial-astm".to_owned()),
                ("lab_app", "demo".to_owned())
            ]
        );
        assert_eq!(
            draft.query(),
            "definition=example%2Fserial-astm&port=usb%3A%22%3E%3Cscript%3E&lab_app=demo"
        );
    }
}
