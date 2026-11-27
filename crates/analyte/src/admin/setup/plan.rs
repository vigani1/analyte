//! Plans a machine's setup from the wizard's choices: the model, the connection and the lab app, checked against the files.
//!
//! Every step and the save build the same [`Plan`] in memory first; only
//! [`save_blocking`] writes, under the write lock. The connection test finds its
//! target here too, refusing a port or address a running machine uses.
use super::{
    Definition, Running, Setup, SetupError, Shipped, Via, cable, choices, definitions,
    lab_apps::{self, SiteLabApp},
    site,
    wizard::{self, listen},
};
use crate::installation::config::{Program, connections};
use crate::machine::link::status::Link;
use analyte_config::{AllowedPeers, Endpoint, RepeatedResults, TransportKind};
use analyte_i18n::message as tr_args;
use analyte_i18n::text as tr;
use analyte_model::Identifier;
use analyte_transport::serial;
use serde::Deserialize;
use std::path::PathBuf;

/// The form value that keeps the installed model when it is not a shipped one.
pub(super) const KEEP: &str = "installed";

/// The machine model a setup step uses.
pub(super) struct Model {
    /// The shipped model's `vendor/model` key; `None` for an installed model
    /// kept from outside the shipped list.
    pub(super) key: Option<String>,
    pub(super) path: PathBuf,
    pub(super) details: std::sync::Arc<analyte_config::DefinitionDetails>,
    /// The name for people; a kept model shows its file.
    pub(super) name: String,
}

impl Model {
    /// The model as the step forms carry it: its key, or [`KEEP`].
    pub(super) fn form_value(&self) -> &str {
        self.key.as_deref().unwrap_or(KEEP)
    }
}

/// Finds the chosen shipped definition, or the installed one when none is
/// chosen. A model not ready yet can be chosen only to record, through its
/// `record.toml` (ADR 0034); a ready model records from its `machine.toml`.
pub(super) fn chosen(
    list: &[Definition],
    key: Option<&str>,
    installed: &site::Installed,
    records_only: bool,
) -> Result<Model, SetupError> {
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
            let current = site::file_identity(installed);
            match list.iter().find(|d| {
                let path = match &d.state {
                    Shipped::Ready { source, .. }
                    | Shipped::Blocked {
                        record: Some(source),
                    } => &source.path,
                    Shipped::Unreadable { path, .. } => path,
                    Shipped::Blocked { record: None } => return false,
                };
                site::file_identity(path) == current
            }) {
                Some(found) => found,
                None => {
                    return Ok(Model {
                        key: None,
                        path: installed.clone(),
                        details: std::sync::Arc::new(
                            definitions::read(installed).map_err(std::sync::Arc::new)?,
                        ),
                        name: installed.display().to_string(),
                    });
                }
            }
        }
    };
    let source = match &found.state {
        Shipped::Ready { source, .. } => source,
        Shipped::Blocked {
            record: Some(source),
        } if records_only => source,
        Shipped::Blocked { record: Some(_) } => {
            return Err(SetupError::RecordOnlyModel(found.key.clone()));
        }
        Shipped::Blocked { record: None } => {
            return Err(SetupError::BlockedDefinition(found.key.clone()));
        }
        Shipped::Unreadable { error, .. } => return Err(error.clone().into()),
    };
    Ok(Model {
        key: Some(found.key.clone()),
        path: source.path.clone(),
        details: source.details.clone(),
        name: found.display.clone(),
    })
}

/// Record only (ADR 0034): what the draft answers for the model's capture
/// profile, read from its protocol. Only the answers that protocol asks for
/// are kept: a value carried from another model's step 2 is not used.
pub(super) mod recording {
    use super::{SetupError, wizard};
    use analyte_config::{ProfileAnswers, VidasAcknowledgment};
    use analyte_i18n::text as tr;

    /// The form value of the mode field that asks for record only.
    pub(in crate::admin::setup) const RECORD: &str = "record";

    /// Whether the draft asks for a record-only connection.
    pub(in crate::admin::setup) fn chosen(draft: &wizard::Draft) -> bool {
        draft.mode.as_deref().map(str::trim) == Some(RECORD)
    }

    /// The saved `[recording]` answer `key` of a site table, as a form value:
    /// `yes` or `no` for a flag, the text or number as written otherwise.
    pub(in crate::admin::setup) fn saved(site: &toml::Table, key: &str) -> Option<String> {
        match site.get("recording")?.get(key)? {
            toml::Value::Boolean(true) => Some("yes".to_owned()),
            toml::Value::Boolean(false) => Some("no".to_owned()),
            toml::Value::String(text) => Some(text.clone()),
            toml::Value::Integer(number) => Some(number.to_string()),
            toml::Value::Float(_)
            | toml::Value::Datetime(_)
            | toml::Value::Array(_)
            | toml::Value::Table(_) => None,
        }
    }

    /// The `[recording]` answers in the draft for this model's protocol.
    fn field(value: &Option<String>) -> Option<&str> {
        value.as_deref().map(str::trim).filter(|v| !v.is_empty())
    }

    pub(in crate::admin::setup) fn settings(
        definition: &analyte_config::DefinitionDetails,
        draft: &wizard::Draft,
    ) -> Result<ProfileAnswers, SetupError> {
        let mut settings = ProfileAnswers::default();
        match definition.protocol.as_str() {
            "autolyser" => {
                settings.poll_seconds = field(&draft.poll_seconds)
                    .map(|seconds| {
                        seconds
                            .parse::<u32>()
                            .ok()
                            .filter(|s| analyte_config::recording::POLL_SECONDS.contains(s))
                            .ok_or(SetupError::RecordingAnswer(tr(
                                "setup-enter-the-seconds-between-asks-for-the-next-report",
                            )))
                    })
                    .transpose()?;
                settings.controls = field(&draft.controls)
                    .map(|value| match value {
                        "yes" => Ok(true),
                        "no" => Ok(false),
                        _ => Err(SetupError::RecordingAnswer(tr(
                            "setup-choose-whether-the-analyzer-s-reports-carry-its-controls",
                        ))),
                    })
                    .transpose()?;
            }
            "vidas" => {
                settings.acknowledgment = field(&draft.acknowledgment)
                    .map(|value| match value {
                        "literal" => Ok(VidasAcknowledgment::Literal),
                        "alternate" => Ok(VidasAcknowledgment::Alternate),
                        _ => Err(SetupError::RecordingAnswer(tr(
                            "setup-choose-the-vidas-pc-acknowledgment-literal-or-alternate",
                        ))),
                    })
                    .transpose()?;
            }
            _ => {}
        }
        // Asked only when the model asks it; the loader checks the value is
        // one it offers.
        if definition.analyzer_mode.is_some() {
            settings.analyzer_mode = field(&draft.analyzer_mode)
                .map(|value| {
                    analyte_config::ChoiceName::new(value).map_err(|_| {
                        SetupError::RecordingAnswer(tr(
                            "setup-choose-the-analyzer-mode-from-the-values-the-model",
                        ))
                    })
                })
                .transpose()?;
        }
        Ok(settings)
    }
}

/// The lab app the machine sends to now: the running one's, or for a machine
/// not running yet, the one its site file names or writes.
pub(super) fn current_lab_app(
    running: Option<&Running>,
    current_site: &toml::Table,
) -> Option<String> {
    if let Some(running) = running {
        return running
            .lab_app
            .as_ref()
            .map(|name| name.as_str().to_owned());
    }
    match lab_apps::site_lab_app(current_site) {
        SiteLabApp::Own(own) => Some(own.name.as_str().to_owned()),
        SiteLabApp::Named(name) => Some(name.to_owned()),
        SiteLabApp::Unnamed => None,
    }
}

/// A setup step posted for the test or the save: the CSRF value, once, and
/// the [`wizard::Draft`] fields. It holds no key.
#[derive(Deserialize)]
#[serde(try_from = "Vec<(String, String)>")]
pub(in crate::admin) struct Choice {
    pub(super) draft: wizard::Draft,
}

impl TryFrom<Vec<(String, String)>> for Choice {
    type Error = String;

    fn try_from(mut fields: Vec<(String, String)>) -> Result<Self, Self::Error> {
        // The CSRF middleware has already checked the value; it is required
        // once, as the form writes it.
        let before = fields.len();
        fields.retain(|(name, _)| name != "csrf");
        if before.checked_sub(fields.len()) != Some(1) {
            return Err("the csrf field is required once".into());
        }
        Ok(Self {
            draft: wizard::Draft::try_from(fields)?,
        })
    }
}

/// Splits a saved TCP endpoint for display or editing. Validation happens
/// when the draft is submitted, so an invalid saved value is not trusted.
pub(super) fn saved_tcp(endpoint: &str) -> Option<(&str, &str)> {
    let (host, port) = endpoint.rsplit_once(':')?;
    Some((host.trim_start_matches('[').trim_end_matches(']'), port))
}

/// The devices a site file lets connect, as `--allowed-peer` takes them;
/// none when it names none (ADR 0026).
pub(super) fn saved_peers(table: &toml::Table) -> Option<String> {
    let listed: Vec<&str> = table
        .get("allowed_peers")?
        .as_array()?
        .iter()
        .filter_map(toml::Value::as_str)
        .collect();
    (!listed.is_empty()).then(|| listed.join(", "))
}

/// The devices the draft lets connect (ADR 0026): addresses typed with
/// commas or spaces between them; none when nothing is typed, which accepts
/// any device. Only a machine that connects to this PC can name them.
fn peers_of(
    kind: TransportKind,
    choice: &wizard::Draft,
) -> Result<Option<AllowedPeers>, SetupError> {
    let typed: Vec<&str> = choice
        .allowed_peers
        .as_deref()
        .unwrap_or_default()
        .split([',', ' ', '\t', '\r', '\n'])
        .filter(|address| !address.is_empty())
        // One past the most, so an overlong list is refused, never cut short.
        .take(AllowedPeers::MOST.saturating_add(1))
        .collect();
    if typed.is_empty() {
        return Ok(None);
    }
    if kind != TransportKind::TcpServer {
        return Err(SetupError::Connection(tr(
            "setup-only-a-machine-that-connects-to-this-pc-can",
        )));
    }
    AllowedPeers::parse(&typed)
        .map(Some)
        .map_err(SetupError::Connection)
}

/// The connection the draft names for the definition's transport.
pub(super) fn connection_of(
    kind: TransportKind,
    choice: &wizard::Draft,
) -> Result<Endpoint, SetupError> {
    let field = |value: &Option<String>| value.as_deref().unwrap_or_default().trim().to_owned();
    match kind {
        TransportKind::Serial => {
            let typed = field(&choice.port_name);
            if !typed.is_empty() {
                return Ok(Endpoint::Fixed(typed));
            }
            match choice.port.as_deref().and_then(|v| v.split_once(':')) {
                Some(("usb", number)) => Ok(Endpoint::UsbSerial(
                    analyte_config::UsbSerial::new(number.into())
                        .map_err(SetupError::Connection)?,
                )),
                Some(("name", name)) if !name.is_empty() => Ok(Endpoint::Fixed(name.into())),
                _ => Err(SetupError::Connection(tr("setup-choose-a-serial-port"))),
            }
        }
        TransportKind::TcpServer | TransportKind::TcpClient => {
            let host: std::net::IpAddr = field(&choice.host).parse().map_err(|_| {
                SetupError::Connection(tr("setup-enter-an-ip-address-such-as-192-168-1"))
            })?;
            let port: u16 = field(&choice.tcp_port)
                .parse()
                .ok()
                .filter(|port| *port != 0)
                .ok_or(SetupError::Connection(tr(
                    "setup-enter-a-port-number-from-1-to-65535",
                )))?;
            Ok(Endpoint::Fixed(
                std::net::SocketAddr::new(host, port).to_string(),
            ))
        }
        TransportKind::WatchedFolder => {
            let folder = field(&choice.folder);
            if folder.is_empty() {
                return Err(SetupError::Connection(tr("setup-enter-the-folder-path")));
            }
            Ok(Endpoint::Fixed(folder))
        }
    }
}

/// The serial cable settings the draft chooses for a definition of transport
/// `kind` (ADR 0031): every typed value, the rest from the definition's
/// suggestion; none for other links, which refuse typed cable settings.
fn serial_of(
    definition: &analyte_config::DefinitionDetails,
    draft: &wizard::Draft,
) -> Result<Option<analyte_config::SerialValues>, SetupError> {
    match definition.kind {
        TransportKind::Serial => cable::chosen(draft, definition.serial.as_ref()).map(Some),
        _ if cable::given(draft) => Err(SetupError::Connection(tr(
            "setup-only-a-machine-on-a-serial-cable-has-cable",
        ))),
        TransportKind::TcpServer | TransportKind::TcpClient | TransportKind::WatchedFolder => {
            Ok(None)
        }
    }
}

/// The site interval, if one was saved for normal operation.
pub(super) fn saved_autolyser_poll(table: &toml::Table) -> Option<String> {
    table
        .get("autolyser_poll_seconds")
        .and_then(toml::Value::as_integer)
        .map(|seconds| seconds.to_string())
}

/// The repeated-results choice saved in a site table, as a form value.
pub(super) fn saved_repeated_results(table: &toml::Table) -> Option<String> {
    table
        .get("repeated_results")
        .and_then(toml::Value::as_str)
        .map(str::to_owned)
}

/// The repeated-results choice to save (ADR 0035, 2026-11-26): the draft's,
/// else the saved one, else automatic. Only a model whose results reconcile
/// through identity recipes asks it; for any other the key goes, and a value
/// other than automatic given for it is refused rather than ignored.
pub(super) fn repeated_results_of(
    definition: &analyte_config::DefinitionDetails,
    draft: &wizard::Draft,
    current: &toml::Table,
) -> Result<RepeatedResults, SetupError> {
    let given = draft
        .repeated_results
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let parse = |value: &str| {
        RepeatedResults::parse(value).ok_or(SetupError::Connection(tr(
            "setup-choose-how-repeated-results-are-handled-automatic-or-always",
        )))
    };
    if !definition.asks_repeated_results {
        return match given.map(parse).transpose()? {
            None | Some(RepeatedResults::Automatic) => Ok(RepeatedResults::Automatic),
            Some(RepeatedResults::AlwaysAsk) => Err(SetupError::Connection(tr(
                "setup-this-model-keeps-its-protocol-s-own-rules-for",
            ))),
        };
    }
    match given {
        Some(value) => parse(value),
        None => saved_repeated_results(current)
            .as_deref()
            .map_or(Ok(RepeatedResults::Automatic), parse),
    }
}

/// Everything a save or a test needs, checked and validated in memory.
pub(super) struct Plan {
    pub(super) program: String,
    pub(super) installed: site::Installed,
    pub(super) definition: PathBuf,
    /// The model's name for people.
    pub(super) model: String,
    pub(super) connection: Endpoint,
    /// The only devices it will accept, when it listens and some are chosen.
    pub(super) peers: Option<AllowedPeers>,
    pub(super) site_text: String,
    /// The new `lab-apps.toml` text, when the save moves a lab app into it.
    pub(super) lab_apps: Option<String>,
    pub(super) validated: analyte_config::Validated,
}

/// Checks every choice of `draft` against the files, in step order, and
/// validates the result as the program loader does; writes nothing.
pub(super) fn plan(setup: &Setup, id: &str, draft: &wizard::Draft) -> Result<Plan, SetupError> {
    let (program, installed) = site::installed(setup.config(), id)?;
    let list = definitions(&setup.seams)?;
    let Model {
        path: definition,
        name: model,
        details,
        ..
    } = model_of(&list, draft, &installed)?;
    let kind = details.kind;
    let connection = connection_of(kind, draft)?;
    let peers = peers_of(kind, draft)?;
    let serial = serial_of(&details, draft)?;
    let records_only = recording::chosen(draft);
    let current = site::read_document(&installed.site)?.into_table();
    let scan = lab_apps::scan_with_site(setup.config(), Some((&installed.site, &current)))?;
    let link = (&connection, peers.as_ref(), serial.as_ref());
    let candidate = if records_only {
        // Record only asks its own questions; the model's choices are not used.
        if draft
            .lab_app
            .as_deref()
            .is_some_and(|name| !name.trim().is_empty())
        {
            return Err(SetupError::LabAppWhileRecording);
        }
        let answers = recording::settings(&details, draft)?;
        site::recording_candidate(current, link, &answers, &scan.file)?
    } else {
        let chosen = choices::chosen(draft, &details.choices)?;
        let lab_app = Identifier::new(draft.lab_app.as_deref().unwrap_or_default().trim())
            .map_err(|_| SetupError::NoLabAppChosen)?;
        let poll = draft
            .poll_seconds
            .as_deref()
            .filter(|seconds| !seconds.trim().is_empty())
            .map(|seconds| {
                seconds.trim().parse::<u32>().map_err(|_| {
                    SetupError::Connection(tr(
                        "setup-enter-the-poll-interval-in-seconds-as-a-whole",
                    ))
                })
            })
            .transpose()?;
        let vidas_namespace = details
            .asks_vidas_patient_namespace
            .then(|| {
                draft
                    .vidas_patient_namespace
                    .as_deref()
                    .map(str::to_owned)
                    .or_else(|| {
                        current
                            .get("vidas_patient_namespace")
                            .and_then(toml::Value::as_str)
                            .map(str::to_owned)
                    })
            })
            .flatten()
            .filter(|value| !value.is_empty());
        let repeats = repeated_results_of(&details, draft, &current)?;
        site::candidate(
            current,
            (link.0, link.1, link.2, &chosen),
            (poll, vidas_namespace.as_deref(), repeats),
            &lab_app,
            &scan.file,
            &scan.settings(),
        )?
    };
    let definition = installed.definition_to_save(definition);
    let validated = site::validate(
        &program,
        &installed,
        &definition,
        &candidate.text,
        &candidate.lab_apps,
    )?;
    let lab_apps = if candidate.lab_apps == scan.file {
        None
    } else {
        Some(candidate.lab_apps.text()?)
    };
    Ok(Plan {
        program,
        installed,
        definition,
        model,
        connection,
        peers,
        site_text: candidate.text,
        lab_apps,
        validated,
    })
}

impl Plan {
    /// The serial cable settings in use in words, with what differs from the
    /// definition's suggestion; none for other links.
    pub(super) fn cable(&self) -> Option<String> {
        let effective = self.validated.effective();
        cable::of(effective.stack())
            .map(|serial| cable::summary(serial, effective.serial_suggestion()))
    }

    /// The model's own choices in words, with what differs from the
    /// definition's suggestion; none when it declares none (ADR 0032).
    pub(super) fn choices(&self) -> Option<String> {
        choices::summary(self.validated.effective().choices())
    }
}

/// What a save wrote, for the page, the command line and the audit row.
pub(super) struct Saved {
    pub(super) digest: [u8; 32],
    pub(super) model: String,
    pub(super) link: Link,
    pub(super) connection: Endpoint,
    /// The only devices it accepts, as saved.
    pub(super) peers: Option<AllowedPeers>,
    /// The lab app it sends results to; none while it records only (ADR 0034).
    pub(super) lab_app: Option<analyte_config::Destination>,
    /// The serial cable settings in words, with what differs from the
    /// definition's suggestion; none for other links.
    pub(super) cable: Option<String>,
    /// The model's own choices in words; none when it declares none.
    pub(super) choices: Option<String>,
    /// How repeated results are handled, where the model asks it (ADR 0035).
    pub(super) repeated_results: Option<RepeatedResults>,
    /// The save moved a lab app into `lab-apps.toml`.
    pub(super) moved: bool,
    pub(super) backups: Vec<PathBuf>,
}

impl Saved {
    /// The audit reason: the connection and the lab app saved through `via`.
    pub(super) fn reason(&self, via: Via) -> String {
        format!(
            "{} saved: {}{}{}{}, {}{}",
            via.machines(),
            site::describe(&self.connection),
            self.cable
                .as_deref()
                .map(|cable| format!(", cable {cable}"))
                .unwrap_or_default(),
            self.choices
                .as_deref()
                .map(|choices| format!(", analyzer settings {choices}"))
                .unwrap_or_default(),
            // Named only when it is not the default, so earlier audit rows
            // and an automatic save read as before (ADR 0035, 2026-11-26).
            match self.repeated_results {
                Some(RepeatedResults::AlwaysAsk) => ", repeated results: always ask me",
                Some(RepeatedResults::Automatic) | None => "",
            },
            self.lab_app.as_ref().map_or_else(
                || "record only, no lab app".to_owned(),
                |lab_app| format!(
                    "lab app {} ({})",
                    lab_app.name.as_str(),
                    lab_app.base_url.as_str()
                )
            ),
            if self.moved {
                "; its lab app settings moved to lab-apps.toml"
            } else {
                ""
            }
        )
    }
}

/// Every check a save makes, writing nothing: the [`plan`], then that no
/// other machine already uses the connection.
pub(super) fn checked(setup: &Setup, id: &str, draft: &wizard::Draft) -> Result<Plan, SetupError> {
    let plan = plan(setup, id, draft)?;
    shares_connection(setup, id, &plan)?;
    Ok(plan)
}

pub(super) fn save_blocking(
    setup: &Setup,
    id: &str,
    draft: &wizard::Draft,
) -> Result<Saved, SetupError> {
    let _writing = setup.writing()?;
    let plan = checked(setup, id, draft)?;
    let moved = plan.lab_apps.is_some();
    let cable = plan.cable();
    let choices = plan.choices();
    let written = site::apply(
        setup.config(),
        &plan.program,
        &plan.installed,
        &plan.definition,
        &plan.site_text,
        plan.lab_apps,
    )?;
    Ok(Saved {
        digest: plan.validated.digest(),
        model: plan.model,
        link: Link::of(plan.validated.effective().stack()),
        connection: plan.connection,
        peers: plan.peers,
        lab_app: plan.validated.effective().site().destination.clone(),
        cable,
        choices,
        repeated_results: plan.validated.effective().repeated_results(),
        moved,
        backups: written.backups,
    })
}

/// The model the draft chose; [`KEEP`] keeps the current one.
pub(super) fn model_of(
    list: &[Definition],
    draft: &wizard::Draft,
    installed: &site::Installed,
) -> Result<Model, SetupError> {
    match draft.definition.as_deref().map(str::trim) {
        None | Some("") => Err(SetupError::UnknownDefinition),
        Some(KEEP) => chosen(list, None, installed, recording::chosen(draft)),
        Some(key) => chosen(list, Some(key), installed, recording::chosen(draft)),
    }
}

/// Refuses a test on a serial port or network address a running machine uses.
fn in_use(setup: &Setup, chosen: &Endpoint, port: Option<&str>) -> Result<(), SetupError> {
    match holder(setup, chosen, port, &|| (setup.seams.ports)()) {
        Some(running) => Err(SetupError::InUse(running.as_str().into())),
        None => Ok(()),
    }
}

/// The running machine that uses the serial port `port` (`chosen` names it)
/// or, without a port, the network address `chosen`; none when no running
/// machine does. A running machine named by `usb_serial` is looked up in
/// `ports`, so choosing its port by name finds it too.
pub(super) fn holder<'a>(
    setup: &'a Setup,
    chosen: &Endpoint,
    port: Option<&str>,
    ports: &dyn Fn() -> std::io::Result<Vec<serial::Listed>>,
) -> Option<&'a Identifier> {
    for running in &setup.running {
        let same = if let Some(port) = port {
            running.link == Link::Serial && {
                // A fixed name comes back as written, without listing ports.
                let running_port =
                    serial::locate(&serial::Device::from(&running.endpoint), ports).ok();
                let same_number = matches!(
                    (&running.endpoint, chosen),
                    (Endpoint::UsbSerial(left), Endpoint::UsbSerial(right))
                        if serial::same_usb_serial(left.as_str(), right.as_str())
                );
                same_number
                    || running_port.is_some_and(|name| connections::same_port_name(&name, port))
            }
        } else {
            matches!(running.link, Link::TcpServer | Link::TcpClient)
                && connections::listeners_overlap(&running.endpoint, chosen)
        };
        if same {
            return Some(&running.id);
        }
    }
    None
}

/// Refuses, before writing, a connection another machine in the program file
/// already uses. When the current program does not load, the reload after
/// writing refuses instead, so a broken file can still be fixed here.
fn shares_connection(setup: &Setup, id: &str, plan: &Plan) -> Result<(), SetupError> {
    let Ok(program) = Program::load(setup.config()) else {
        return Ok(());
    };
    let chosen = (
        Link::of(plan.validated.effective().stack()),
        &plan.connection,
    );
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

pub(super) fn listen_target(
    setup: &Setup,
    plan: &Plan,
) -> Result<(String, listen::Target), SetupError> {
    let stack = plan.validated.effective().stack();
    let endpoint = &plan.connection;
    match Link::of(stack) {
        Link::Serial => {
            let device = serial::Device::from(endpoint);
            let ports = setup.seams.ports.clone();
            let name = serial::locate(&device, || ports())?;
            in_use(setup, endpoint, Some(&name))?;
            let settings = crate::machine::link::serial::line_settings(
                stack,
                serial::Device::Named(name.clone()),
                listen::SERIAL_READ,
            )
            .ok_or(SetupError::NothingToListen)?
            .map_err(SetupError::SerialSettings)?;
            let words = cable::of(stack).map(cable::words).unwrap_or_default();
            Ok((
                tr_args(
                    "setup-serial-port-2",
                    [
                        ("name", (name).to_string().into()),
                        ("words", (words).to_string().into()),
                    ],
                ),
                listen::Target::Serial {
                    name,
                    settings,
                    open: setup.seams.open.clone(),
                },
            ))
        }
        link @ (Link::TcpServer | Link::TcpClient) => {
            in_use(setup, endpoint, None)?;
            let Endpoint::Fixed(text) = endpoint else {
                return Err(SetupError::Connection(tr(
                    "setup-a-network-link-needs-an-ip-address-and-port",
                )));
            };
            let address: std::net::SocketAddr = text
                .parse()
                .map_err(|_| SetupError::Connection(tr("setup-enter-an-ip-address-and-port")))?;
            Ok(if link == Link::TcpServer {
                (
                    tr_args(
                        "setup-listening-on",
                        [("address", (address).to_string().into())],
                    ),
                    listen::Target::TcpServer(address),
                )
            } else {
                (
                    tr_args(
                        "setup-connecting-to",
                        [("address", (address).to_string().into())],
                    ),
                    listen::Target::TcpClient(address),
                )
            })
        }
        Link::Folder => Err(SetupError::NothingToListen),
    }
}
