//! The `machines` commands: list, models, ports, add, set, test and remove, through the Setup page's own checks and writes.
use super::ask::{self, Ask};
use super::{
    Failure, Installation, Outcome, actor, backups, live, problem, record, restart_note, say,
};
use crate::admin::setup::{
    Definition, Running, Seams, Setup, SetupError, Shipped, Via, cable, choices, definitions,
    lab_apps, level_and_state,
    machines::{add_machine, preview_removal, remove_machine},
    model, on_files,
    plan::{self, KEEP, Plan},
    site,
    wizard::{Draft, connection_words, host_choices, listen, verdict},
};
use crate::admin::view::{Tone, format};
use crate::machine::link::status::Link;
use analyte_config::{SerialSetting, TransportKind};
use analyte_model::Identifier;
use analyte_transport::serial;
use std::{path::Path, sync::Arc, time::Duration};

/// The `machines` commands.
#[derive(clap::Subcommand)]
pub enum Command {
    /// Lists the machines in the settings: model, connection, lab app, whether
    /// set up, settings version and, from the running engine, the live state.
    List {
        /// Prints JSON for scripts instead of lines.
        #[arg(long)]
        json: bool,
    },
    /// Lists the machine models in the machines folder: ready or not, and how
    /// each was checked.
    Models,
    /// Lists this PC's serial ports with their adapter and USB serial number.
    Ports,
    /// Adds a machine to the settings; it is not set up yet. Serial cable
    /// settings given here are kept for its setup.
    Add {
        /// Letters A to Z, digits, - and _; it also names its settings file.
        name: String,
        /// The name has stored history and this is the same analyzer.
        #[arg(long)]
        same_analyzer: bool,
        /// The serial cable settings; see the options below.
        #[command(flatten)]
        cable: Cable,
    },
    /// Sets a machine up: its model, connection and lab app, checked as the
    /// Save step checks them. Choices not given keep their saved values.
    Set {
        /// The machine's name.
        name: String,
        /// The choices; see the options below.
        #[command(flatten)]
        choice: Choice,
        /// Prints what would be written and writes nothing.
        #[arg(long)]
        dry_run: bool,
    },
    /// Runs the listen-only connection test of step 3 with the saved settings
    /// or the choices given: nothing is sent and nothing is stored.
    Test {
        /// The machine's name.
        name: String,
        /// The choices; see the options below.
        #[command(flatten)]
        choice: Choice,
        /// How long to listen, 1 to 60 seconds.
        #[arg(long, default_value_t = 60, value_parser = clap::value_parser!(u64).range(1..=60))]
        seconds: u64,
        /// Also prints the bytes received (at most 64 KiB). They may contain
        /// patient data: never save or share them.
        #[arg(long)]
        show_bytes: bool,
    },
    /// Removes a machine from the settings; its stored results and history stay.
    Remove {
        /// The machine's name.
        name: String,
        /// Removes it even though work is still unfinished.
        #[arg(long)]
        anyway: bool,
    },
}

/// A serial machine's cable settings, as step 2 chooses them (ADR 0031):
/// match the analyzer's LIS/host settings screen. A setting not given keeps
/// its saved value, else the model's suggestion.
#[derive(clap::Args, Clone, Default)]
pub struct Cable {
    /// Serial cable: the baud rate: 300, 600, 1200, 2400, 4800, 9600, 19200,
    /// 38400, 57600 or 115200.
    #[arg(long, value_name = "RATE")]
    baud: Option<String>,
    /// Serial cable: data bits, 5 to 8.
    #[arg(long, value_name = "BITS")]
    data_bits: Option<String>,
    /// Serial cable: parity: none, odd or even.
    #[arg(long, value_name = "PARITY")]
    parity: Option<String>,
    /// Serial cable: stop bits, 1 or 2.
    #[arg(long, value_name = "BITS")]
    stop_bits: Option<String>,
    /// Serial cable: flow control: none, hardware (RTS/CTS) or software (XON/XOFF).
    #[arg(long, value_name = "FLOW")]
    flow_control: Option<String>,
}

impl Cable {
    /// `draft` with each given setting applied.
    fn over(self, mut draft: Draft) -> Draft {
        for (setting, given) in [
            (SerialSetting::Baud, self.baud),
            (SerialSetting::DataBits, self.data_bits),
            (SerialSetting::Parity, self.parity),
            (SerialSetting::StopBits, self.stop_bits),
            (SerialSetting::FlowControl, self.flow_control),
        ] {
            if given.is_some() {
                cable::set(&mut draft, setting, given);
            }
        }
        draft
    }

    /// The settings given, read and checked as step 2 reads them; none when
    /// none is given.
    fn chosen(self) -> Result<Option<analyte_config::SerialValues>, SetupError> {
        let draft = self.over(Draft::default());
        if !cable::given(&draft) {
            return Ok(None);
        }
        cable::chosen(&draft, None).map(Some)
    }
}

/// A machine's model, connection and lab app, as step 1 and step 2 choose them.
#[derive(clap::Args, Clone, Default)]
pub struct Choice {
    /// The machine model: its key as `machines models` lists it, or
    /// `installed` to keep the current one.
    #[arg(long)]
    model: Option<String>,
    /// A serial port by name, such as COM3.
    #[arg(long, group = "connection")]
    serial_port: Option<String>,
    /// A USB serial adapter by its serial number, so it is found in any USB socket.
    #[arg(long, group = "connection")]
    usb_serial: Option<String>,
    /// The IP address of a network link (this PC's when the machine connects
    /// to Analyte, the machine's when Analyte connects to it), with --port.
    #[arg(long, group = "connection", requires = "port")]
    address: Option<String>,
    /// The network port number, with --address.
    #[arg(long, requires = "address")]
    port: Option<String>,
    /// The folder Analyte reads results from.
    #[arg(long, group = "connection")]
    folder: Option<String>,
    /// The lab app this machine sends results to.
    #[arg(long)]
    lab_app: Option<String>,
    /// For a machine that connects to Analyte: accept connections only from
    /// this device, the analyzer's own IP address; repeat for several. Any
    /// other device is refused (ADR 0026).
    #[arg(
        long = "allowed-peer",
        value_name = "IP",
        conflicts_with = "any_device"
    )]
    allowed_peer: Vec<String>,
    /// For a machine that connects to Analyte: accept connections from any
    /// device again, removing its allowed devices.
    #[arg(long)]
    any_device: bool,
    /// The serial cable settings.
    #[command(flatten)]
    cable: Cable,
    /// One of the model's own settings, as step 2 lists them under Analyzer
    /// settings and `machines models` names them; repeat for several. A
    /// setting not given keeps its saved value, else the model's suggestion.
    #[arg(long = "choice", value_name = "NAME=VALUE", value_parser = choice_pair)]
    choice: Vec<(String, String)>,
}

/// Reads `--choice NAME=VALUE`; the model's own checks follow.
fn choice_pair(text: &str) -> Result<(String, String), String> {
    text.split_once('=')
        .map(|(name, value)| (name.trim().to_owned(), value.trim().to_owned()))
        .filter(|(name, value)| !name.is_empty() && !value.is_empty())
        .ok_or_else(|| "write NAME=VALUE, such as protocol_mode=alternate".to_owned())
}

impl Choice {
    /// `draft` with these choices applied; a new connection replaces the saved one.
    fn over(self, mut draft: Draft) -> Draft {
        let serial = self.serial_port.is_some() || self.usb_serial.is_some();
        if serial || self.address.is_some() || self.folder.is_some() {
            // The analyzer is the same device whichever address Analyte listens
            // on, and the same cable whichever port it is plugged into.
            let kept = Draft {
                definition: draft.definition.take(),
                lab_app: draft.lab_app.take(),
                allowed_peers: draft.allowed_peers.take(),
                ..Draft::default()
            };
            draft = if serial {
                Draft {
                    baud: draft.baud,
                    data_bits: draft.data_bits,
                    parity: draft.parity,
                    stop_bits: draft.stop_bits,
                    flow_control: draft.flow_control,
                    ..kept
                }
            } else {
                kept
            };
        }
        draft = self.cable.over(draft);
        draft.choices.extend(self.choice);
        if !self.allowed_peer.is_empty() {
            draft.allowed_peers = Some(self.allowed_peer.join(", "));
        }
        if self.any_device {
            // Chosen: none, so nothing is asked about it either.
            draft.allowed_peers = Some(String::new());
        }
        if let Some(model) = self.model {
            draft.definition = Some(model);
        }
        draft.port_name = self.serial_port.or(draft.port_name);
        if let Some(number) = self.usb_serial {
            draft.port = Some(format!("usb:{number}"));
        }
        draft.host = self.address.or(draft.host);
        draft.tcp_port = self.port.or(draft.tcp_port);
        draft.folder = self.folder.or(draft.folder);
        if let Some(lab_app) = self.lab_app {
            draft.lab_app = Some(lab_app);
        }
        draft
    }
}

pub(super) async fn run(
    config: &Path,
    given: Option<Identifier>,
    command: Command,
    ask: Ask,
    seams: Seams,
) -> Result<Outcome, Failure> {
    match command {
        Command::Models => models(&seams),
        Command::Ports => ports(config, seams),
        Command::List { json } => list(&Installation::load(config, seams)?, json).await,
        Command::Add {
            name,
            same_analyzer,
            cable,
        } => {
            let installation = Installation::load(config, seams)?;
            let serial = cable.chosen()?;
            let actor = actor(given, ask)?;
            add(&installation, &actor, &name, (same_analyzer, serial), ask).await
        }
        Command::Set {
            name,
            choice,
            dry_run,
        } => {
            let installation = Installation::load(config, seams)?;
            set(&installation, given, &name, choice, dry_run, ask).await
        }
        Command::Test {
            name,
            choice,
            seconds,
            show_bytes,
        } => {
            let (installation, note) = Installation::load_running(config, seams).await?;
            if let Some(note) = note {
                say(&note)?;
            }
            test(
                &installation,
                given,
                &name,
                choice,
                seconds,
                show_bytes,
                ask,
            )
            .await
        }
        Command::Remove { name, anyway } => {
            let installation = Installation::load(config, seams)?;
            let actor = actor(given, ask)?;
            remove(&installation, &actor, &name, anyway, ask).await
        }
    }
}

/// Each shipped model with its state and how it was checked, in the page's words.
fn models(seams: &Seams) -> Result<Outcome, Failure> {
    for definition in definitions(seams)? {
        let (checked, status, _) = level_and_state(&definition);
        say(&format!(
            "{}: {} - {}; {} ({})",
            definition.key, definition.display, status.word, checked.shown, checked.exact
        ))?;
        if let Some(note) = status.note {
            say(&format!("  {note}"))?;
        }
        if let Shipped::Ready { path, .. } = &definition.state {
            for choice in choices::declared(path) {
                say(&format!(
                    "  --choice {}=<{}>: {}{}",
                    choice.name,
                    choice.allowed().replace(", ", "|"),
                    choice.label.as_str(),
                    choice
                        .suggested
                        .as_ref()
                        .map(|value| format!("; the manual suggests {value}"))
                        .unwrap_or_default()
                ))?;
            }
        }
    }
    Ok(Outcome::Done)
}

/// One detected serial port in step 2's words: its adapter, how it is
/// remembered, and the running machine that uses it, if any.
fn port_line(port: &serial::Listed, used_by: Option<&Identifier>) -> String {
    let remembered = match port.factory_serial() {
        Some(number) => format!("remembered by its serial number {number} (--usb-serial {number})"),
        None => format!(
            "no serial number; remembered by its port name (--serial-port {})",
            port.name
        ),
    };
    format!(
        "{} - {} - {}",
        port.name,
        port.description.as_deref().unwrap_or("unknown adapter"),
        used_by.map_or(remembered, |holder| format!("used by {}", holder.as_str()))
    )
}

/// The detected serial ports; when the settings load, a port a configured
/// machine uses says so, as step 2 shows it.
fn ports(config: &Path, seams: Seams) -> Result<Outcome, Failure> {
    let listed: Vec<serial::Listed> = (seams.ports)()
        .map_err(Failure::Ports)?
        .into_iter()
        .take(serial::LIST_LIMIT)
        .collect();
    if listed.is_empty() {
        say("No serial port was found on this PC.")?;
        return Ok(Outcome::Done);
    }
    let installation = Installation::load(config, seams).ok();
    for port in &listed {
        let endpoint = port.factory_serial().map_or_else(
            || analyte_config::Endpoint::Fixed(port.name.clone()),
            |number| {
                analyte_config::UsbSerial::new(number.into()).map_or_else(
                    |_| analyte_config::Endpoint::Fixed(port.name.clone()),
                    analyte_config::Endpoint::UsbSerial,
                )
            },
        );
        let used_by = installation.as_ref().and_then(|installation| {
            plan::holder(&installation.setup, &endpoint, Some(&port.name), &|| {
                Ok(listed.clone())
            })
        });
        say(&port_line(port, used_by))?;
    }
    Ok(Outcome::Done)
}

/// One machine of the settings file, read for `machines list`.
#[derive(serde::Serialize)]
pub(super) struct Listed {
    pub(super) id: String,
    /// The model for people; none while not set up.
    model: Option<String>,
    definition: Option<String>,
    /// The site-file line of its connection; none while none is saved.
    connection: Option<String>,
    lab_app: Option<String>,
    set_up: bool,
    /// The digest of what the files load now, which a restart runs.
    settings_version: Option<String>,
    /// What the running engine says, in the Machines page's words.
    live: Option<live::Row>,
    /// The only devices it accepts, when it connects to Analyte and its site
    /// file names them (ADR 0026).
    allowed_peers: Option<Vec<String>>,
    /// It connects to Analyte and accepts any device: a warning.
    open_to_any_device: bool,
    /// The serial cable settings the files load now, when it is on a serial
    /// cable (ADR 0031).
    pub(super) cable: Option<ListedCable>,
    /// Each of the model's own choices the files load now (ADR 0032); empty
    /// when it declares none.
    pub(super) choices: Vec<ListedChoice>,
    /// The choices in words, with what differs from the suggestion.
    pub(super) choices_words: Option<String>,
}

/// One of a model's own choices as `machines list` shows it.
#[derive(serde::Serialize)]
pub(super) struct ListedChoice {
    name: String,
    /// `mode` or `software_version`.
    kind: &'static str,
    /// The value in use.
    value: String,
    /// The manual's suggestion; none when it suggests none.
    suggested: Option<String>,
    /// The site file chose it; otherwise the suggestion applies.
    selected: bool,
    differs_from_suggestion: bool,
}

/// A serial machine's cable settings as `machines list` shows them.
#[derive(serde::Serialize)]
pub(super) struct ListedCable {
    /// The values in use.
    settings: analyte_config::SerialValues,
    /// The settings whose value is not the model's suggestion.
    differs_from_suggestion: Vec<&'static str>,
    /// The values in words, with what differs.
    pub(super) words: String,
}

/// The whole JSON answer of `machines list --json`.
#[derive(serde::Serialize)]
struct ListAnswer {
    /// `running`, `not running` or `unknown`.
    engine: &'static str,
    /// Why the live state is unknown.
    reason: Option<String>,
    machines: Vec<Listed>,
    /// Machines the engine still runs that the settings no longer list.
    removed: Vec<live::Row>,
}

/// Every machine of the settings file as `machines list` reads it, before
/// the engine is asked how they stand.
pub(super) fn settings_rows(installation: &Installation) -> Result<Vec<Listed>, Failure> {
    let setup = &installation.setup;
    let config = setup.config().to_owned();
    let root = setup.seams.definitions.clone();
    let (_, entries) = site::entries(&config)?;
    let mut machines = Vec::new();
    for entry in &entries {
        let id = entry.id.get_ref().clone();
        let definition = entry.definition.as_ref().map(|d| d.get_ref().clone());
        let table = crate::installation::config::read(
            &crate::installation::config::folder(&config).join(entry.site.get_ref()),
        )
        .ok()
        .and_then(|text| text.parse::<toml::Table>().ok())
        .unwrap_or_default();
        let connection = ["usb_serial", "endpoint"].iter().find_map(|key| {
            table
                .get(*key)
                .and_then(toml::Value::as_str)
                .map(|value| format!("{key} = {value}"))
        });
        let loaded = installation
            .program
            .machines()
            .iter()
            .find(|machine| machine.id().as_str() == id);
        let version = loaded.map(|machine| format::digest_short(&machine.snapshot().digest()));
        let cable = loaded.and_then(|machine| {
            let effective = machine.snapshot().effective();
            let suggestion = effective.serial_suggestion();
            cable::of(effective.stack()).map(|serial| ListedCable {
                settings: cable::values(serial),
                differs_from_suggestion: serial
                    .differs_from(suggestion)
                    .into_iter()
                    .map(SerialSetting::key)
                    .collect(),
                words: cable::summary(serial, suggestion),
            })
        });
        let chosen = loaded.map(|machine| machine.snapshot().effective().choices());
        let listed_choices = chosen
            .unwrap_or_default()
            .iter()
            .map(|chosen| ListedChoice {
                name: chosen.name.as_str().to_owned(),
                kind: match chosen.kind {
                    analyte_config::ChoiceKind::Mode => "mode",
                    analyte_config::ChoiceKind::SoftwareVersion => "software_version",
                },
                value: chosen.value.as_str().to_owned(),
                suggested: chosen
                    .suggested
                    .as_ref()
                    .map(|(value, _)| value.as_str().to_owned()),
                selected: chosen.selected,
                differs_from_suggestion: chosen.differs(),
            })
            .collect();
        let listens = definition.as_deref().is_some_and(|path| {
            plan::kind_of(&crate::installation::config::folder(&config).join(path))
                == Some(TransportKind::TcpServer)
        });
        let allowed_peers = saved_peers(&table)
            .map(|peers| peers.split(", ").map(str::to_owned).collect::<Vec<_>>());
        machines.push(Listed {
            open_to_any_device: listens && allowed_peers.is_none(),
            allowed_peers: allowed_peers.filter(|_| listens),
            model: model(&config, root.as_deref(), definition.as_deref()).map(|m| m.shown),
            definition,
            connection,
            lab_app: plan::current_lab_app(running(setup, &id), &table),
            set_up: entry.definition.is_some(),
            settings_version: version,
            live: None,
            cable,
            choices: listed_choices,
            choices_words: chosen.and_then(choices::summary),
            id,
        });
    }
    Ok(machines)
}

async fn list(installation: &Installation, json: bool) -> Result<Outcome, Failure> {
    let mut machines = settings_rows(installation)?;
    let (engine, reason, mut rows, settings_read) = match live::read(&installation.program).await {
        live::Live::Running {
            settings_read,
            machines,
        } => ("running", None, machines, settings_read),
        live::Live::NotRunning => ("not running", None, Vec::new(), true),
        live::Live::Unknown(why) => ("unknown", Some(why), Vec::new(), true),
    };
    for machine in &mut machines {
        if let Some(at) = rows.iter().position(|row| row.id == machine.id) {
            machine.live = Some(rows.remove(at));
        }
    }
    let answer = ListAnswer {
        engine,
        reason,
        machines,
        removed: rows,
    };
    if json {
        let text = serde_json::to_string_pretty(&answer)
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        say(&text)?;
        return Ok(Outcome::Done);
    }
    say(&match (answer.engine, &answer.reason) {
        ("running", _) => "Engine running: live state as the Machines page shows it.".to_owned(),
        ("not running", _) => "Engine not running: live state unknown.".to_owned(),
        (_, reason) => format!(
            "Live state unknown: {}.",
            reason.as_deref().unwrap_or("no answer")
        ),
    })?;
    if !settings_read {
        say(
            "The engine could not read the settings file; machines changed since it started may be missing from its list.",
        )?;
    }
    if answer.machines.is_empty() {
        say("No machine is listed in the settings. Add one with: analyte machines add <name>")?;
    }
    for machine in &answer.machines {
        print_machine(machine, answer.engine == "running")?;
    }
    for row in &answer.removed {
        say(&format!("{}: no longer in the settings", row.id))?;
        print_live(row)?;
    }
    Ok(Outcome::Done)
}

fn print_machine(machine: &Listed, engine: bool) -> std::io::Result<()> {
    let not = format::NOT_APPLICABLE;
    say(&format!("{}:", machine.id))?;
    say(&format!(
        "  Model: {}",
        match (&machine.model, &machine.definition) {
            (Some(model), Some(path)) if model != path => format!("{model} ({path})"),
            (Some(model), _) => model.clone(),
            (None, _) => "none chosen yet".to_owned(),
        }
    ))?;
    say(&format!(
        "  Connection: {}",
        machine.connection.as_deref().unwrap_or(not)
    ))?;
    if let Some(cable) = &machine.cable {
        say(&format!("  Cable: {}", cable.words))?;
    }
    if let Some(words) = &machine.choices_words {
        say(&format!("  Analyzer settings: {words}"))?;
    }
    if let Some(line) = devices_line(machine) {
        say(&line)?;
    }
    say(&format!(
        "  Lab app: {}",
        machine.lab_app.as_deref().unwrap_or(not)
    ))?;
    say(&format!(
        "  Setup: {}",
        if machine.set_up {
            "set up"
        } else {
            "not set up yet"
        }
    ))?;
    say(&format!(
        "  Settings version: {}",
        machine.settings_version.as_deref().unwrap_or(not)
    ))?;
    match &machine.live {
        Some(row) => print_live(row),
        None if engine => say("  State: the engine does not report it"),
        None => say("  State: unknown"),
    }
}

/// What the engine reported for one machine, in the page's words.
fn print_live(row: &live::Row) -> std::io::Result<()> {
    say(&format!(
        "  State: {}{}",
        row.status,
        row.note
            .as_deref()
            .map(|note| format!(" - {note}"))
            .unwrap_or_default()
    ))?;
    if let Some(since) = &row.since {
        say(&format!("  Open since: {} ({})", since.shown, since.exact))?;
    }
    say(&format!(
        "  Last data: {}",
        row.last_data.as_ref().map_or_else(
            || "nothing received since the start".to_owned(),
            |at| format!("{} ({})", at.shown, at.exact)
        )
    ))?;
    match &row.refused {
        Some(refused) => say(&format!("  Latest refusal: {refused}")),
        None => Ok(()),
    }
}

/// Which devices a machine that connects to Analyte accepts, as `machines
/// list` says it: the allowed devices, or a warning (ADR 0026); none for
/// other links.
pub(super) fn devices_line(machine: &Listed) -> Option<String> {
    match (&machine.allowed_peers, machine.open_to_any_device) {
        (Some(peers), _) => Some(format!("  Devices: only {}", peers.join(", "))),
        (None, true) => Some(format!(
            "  Devices: warning - {}. Accept only the analyzer with: analyte machines set {} --allowed-peer <IP>",
            format::OPEN_TO_ANY.to_lowercase(),
            machine.id
        )),
        (None, false) => None,
    }
}

/// What the service runs for `id`; every machine the settings load counts.
fn running<'a>(setup: &'a Setup, id: &str) -> Option<&'a Running> {
    setup
        .running
        .iter()
        .find(|running| running.id.as_str() == id)
}

async fn add(
    installation: &Installation,
    actor: &Identifier,
    name: &str,
    (same_analyzer, serial): (bool, Option<analyte_config::SerialValues>),
    ask: Ask,
) -> Result<Outcome, Failure> {
    let setup = &installation.setup;
    installation
        .with_store(async |store| {
            let added = match add_machine(setup, store, name, same_analyzer, serial.clone()).await {
                Err(SetupError::HistoryNotConfirmed(id)) if ask == Ask::Terminal => {
                    say(&format::sentence(
                        &SetupError::HistoryNotConfirmed(id).to_string(),
                    ))?;
                    if !ask.confirm("Is it the same analyzer?")? {
                        return Err(Failure::NotConfirmed);
                    }
                    add_machine(setup, store, name, true, serial.clone()).await?
                }
                added => added?,
            };
            let id = added.id.as_str();
            say(&format!("{id} is added. It is listed and not set up yet."))?;
            if added.reused {
                say("It joins the stored history of the earlier machine with this name.")?;
            }
            say(&format!("Its settings file: {}", added.path.display()))?;
            backups(&added.written.backups)?;
            let outcome = record(
                store,
                actor,
                "setup-add",
                id,
                &added.reason(Via::CommandLine),
            )
            .await?;
            say(&format!(
                "Set it up next: analyte machines set {id} --model <key> ... --lab-app <name>"
            ))?;
            restart_note()?;
            Ok(outcome)
        })
        .await
}

/// The machine's saved choices as a draft: its model kept, its connection and
/// its lab app, as step 2 shows them before anything is changed.
fn saved(setup: &Setup, id: &str) -> Result<Draft, SetupError> {
    let (_, installed) = site::installed(setup.config(), id)?;
    let table = crate::installation::config::read(&installed.site)
        .ok()
        .and_then(|text| text.parse::<toml::Table>().ok())
        .unwrap_or_default();
    let text = |key: &str| {
        table
            .get(key)
            .and_then(toml::Value::as_str)
            .map(str::to_owned)
    };
    let mut draft = Draft {
        definition: installed.definition.as_ref().map(|_| KEEP.to_owned()),
        lab_app: plan::current_lab_app(running(setup, id), &table),
        allowed_peers: saved_peers(&table),
        ..Draft::default()
    };
    let kind = installed.definition.as_deref().and_then(plan::kind_of);
    // Also before a model is chosen: `machines add` may have kept cable settings.
    if matches!(kind, Some(TransportKind::Serial) | None) {
        cable::keep_saved(&mut draft, &table);
    }
    match (kind, text("usb_serial"), text("endpoint")) {
        (_, Some(number), _) => draft.port = Some(format!("usb:{number}")),
        (Some(TransportKind::Serial), None, endpoint) => draft.port_name = endpoint,
        (Some(TransportKind::TcpServer | TransportKind::TcpClient), None, Some(endpoint)) => {
            if let Some((host, port)) = endpoint.rsplit_once(':') {
                draft.host = Some(
                    host.trim_start_matches('[')
                        .trim_end_matches(']')
                        .to_owned(),
                );
                draft.tcp_port = Some(port.to_owned());
            }
        }
        (Some(TransportKind::TcpServer | TransportKind::TcpClient), None, None) => {}
        (Some(TransportKind::WatchedFolder) | None, None, endpoint) => draft.folder = endpoint,
    }
    Ok(draft)
}

/// Fills each of the draft model's own choices not given with `--choice`
/// from the saved site file, as step 2 starts from them; a saved choice the
/// model does not declare (another model's) is left behind. Nothing is
/// filled while the model is unknown; the plan refuses that by itself.
fn keep_saved_choices(setup: &Setup, id: &str, draft: &mut Draft) -> Result<(), SetupError> {
    let (_, installed) = site::installed(setup.config(), id)?;
    let list = definitions(&setup.seams)?;
    let Ok(model) = plan::model_of(&list, draft, &installed) else {
        return Ok(());
    };
    let table = crate::installation::config::read(&installed.site)
        .ok()
        .and_then(|text| text.parse::<toml::Table>().ok())
        .unwrap_or_default();
    choices::keep_saved(draft, &table, &choices::declared(&model.path));
    Ok(())
}

/// The devices a site file lets connect, as `--allowed-peer` takes them;
/// none when it names none (ADR 0026).
fn saved_peers(table: &toml::Table) -> Option<String> {
    let listed: Vec<&str> = table
        .get("allowed_peers")?
        .as_array()?
        .iter()
        .filter_map(toml::Value::as_str)
        .collect();
    (!listed.is_empty()).then(|| listed.join(", "))
}

/// For a machine that connects to Analyte whose devices are not chosen yet,
/// asks at a terminal which devices it accepts, as step 2's field does (ADR
/// 0026). Returns whether it asked.
fn ask_peers(setup: &Setup, id: &str, draft: &mut Draft, ask: Ask) -> Result<bool, Failure> {
    if ask == Ask::Never || draft.allowed_peers.is_some() {
        return Ok(false);
    }
    let plan = plan::plan(setup, id, draft)?;
    if Link::of(plan.validated.effective().stack()) != Link::TcpServer {
        return Ok(false);
    }
    say(concat!(
        "Any device that reaches this port could send results as this machine. ",
        "Run analyte machines test to see the analyzer's address."
    ))?;
    let typed = ask.line(
        "Only accept these devices: the analyzer's IP address (several with commas; empty accepts any device)",
    )?;
    draft.allowed_peers = Some(typed.unwrap_or_default());
    Ok(true)
}

/// Which devices a listening machine accepts, in the pages' words; none for
/// other links.
fn accepts(link: Link, peers: Option<&analyte_config::AllowedPeers>) -> Option<String> {
    (link == Link::TcpServer).then(|| {
        peers.map_or_else(
            || format!("{} - {}", format::OPEN_TO_ANY, format::OPEN_TO_ANY_ADVICE),
            |peers| format!("Accepts connections only from {peers}"),
        )
    })
}

/// Asks at a terminal for each choice the draft still lacks, in step order:
/// the model, the connection, the lab app. Returns whether anything was asked.
fn fill(setup: &Setup, id: &str, draft: &mut Draft, ask: Ask) -> Result<bool, Failure> {
    if ask == Ask::Never {
        return Ok(false);
    }
    let mut asked = false;
    let (_, installed) = site::installed(setup.config(), id)?;
    let list = definitions(&setup.seams)?;
    let model = match plan::model_of(&list, draft, &installed) {
        Ok(model) => model,
        Err(_) => {
            asked = true;
            let choices: Vec<ask::Choice> = list
                .iter()
                .filter(|definition| matches!(definition.state, Shipped::Ready { .. }))
                .map(|definition| ask::Choice {
                    value: definition.key.clone(),
                    label: model_label(definition),
                })
                .collect();
            draft.definition = ask.choose("Step 1, Machine: which model is it?", &choices)?;
            plan::model_of(&list, draft, &installed)?
        }
    };
    let kind = plan::kind_of(&model.path);
    if plan::connection_of(kind, draft).is_err() {
        asked = true;
        ask_connection(setup, kind, draft, ask)?;
    }
    let declared = choices::declared(&model.path);
    for choice in choices::missing(draft, &declared) {
        asked = true;
        let options: Vec<ask::Choice> = choice
            .values
            .iter()
            .map(|value| ask::Choice {
                value: value.value.as_str().to_owned(),
                label: value.label.as_str().to_owned(),
            })
            .collect();
        let question = format!(
            "Step 2, Analyzer settings: {}? The model suggests none; {}",
            choice.label.as_str(),
            choices::hint(choice.kind).to_lowercase()
        );
        if let Some(value) = ask.choose(&question, &options)? {
            draft.choices.insert(choice.name.as_str().to_owned(), value);
        }
    }
    if kind == Some(TransportKind::Serial) {
        for setting in cable::missing(draft, cable::suggested(&model.path).as_ref()) {
            asked = true;
            let choices: Vec<ask::Choice> = cable::options(setting)
                .into_iter()
                .map(|(value, label)| ask::Choice { value, label })
                .collect();
            let question = format!(
                "Step 2, Cable: {}? The model suggests none; match the analyzer's LIS/host settings screen",
                cable::label(setting).to_lowercase()
            );
            let chosen = ask.choose(&question, &choices)?;
            cable::set(draft, setting, chosen);
        }
    }
    let scan = lab_apps::scan(setup.config())?;
    let known = draft
        .lab_app
        .as_deref()
        .is_some_and(|name| scan.get(name.trim()).is_some());
    if !known {
        asked = true;
        let choices: Vec<ask::Choice> = scan
            .known
            .iter()
            .map(|known| ask::Choice {
                value: known.settings.name.as_str().to_owned(),
                label: format!(
                    "{} - {}",
                    known.settings.name.as_str(),
                    known.settings.base_url.as_str()
                ),
            })
            .collect();
        if choices.is_empty() {
            say(
                "There is no lab app yet; add one first with: analyte lab-apps add <name> --address <https://...>",
            )?;
        }
        draft.lab_app = ask.choose("Which lab app does it send results to?", &choices)?;
    }
    Ok(asked)
}

/// A shipped model as a numbered choice shows it.
fn model_label(definition: &Definition) -> String {
    let (checked, _, _) = level_and_state(definition);
    format!(
        "{} ({}) - {}",
        definition.display, definition.key, checked.shown
    )
}

/// Step 2 at the terminal: a detected port or a typed name, an address and a
/// port number, or a folder, by the model's kind of link.
fn ask_connection(
    setup: &Setup,
    kind: Option<TransportKind>,
    draft: &mut Draft,
    ask: Ask,
) -> Result<(), Failure> {
    *draft = Draft {
        definition: draft.definition.take(),
        lab_app: draft.lab_app.take(),
        baud: draft.baud.take(),
        data_bits: draft.data_bits.take(),
        parity: draft.parity.take(),
        stop_bits: draft.stop_bits.take(),
        flow_control: draft.flow_control.take(),
        ..Draft::default()
    };
    if kind != Some(TransportKind::Serial) {
        for setting in SerialSetting::ALL {
            cable::set(draft, setting, None);
        }
    }
    match kind {
        Some(TransportKind::Serial) => {
            let ports: Vec<serial::Listed> = (setup.seams.ports)()
                .unwrap_or_default()
                .into_iter()
                .take(serial::LIST_LIMIT)
                .collect();
            let mut choices: Vec<ask::Choice> = ports
                .iter()
                .map(|port| ask::Choice {
                    value: port.factory_serial().map_or_else(
                        || format!("name:{}", port.name),
                        |number| format!("usb:{number}"),
                    ),
                    label: port_line(port, None),
                })
                .collect();
            choices.push(ask::Choice {
                value: String::new(),
                label: "Type a port name".into(),
            });
            match ask
                .choose("Step 2, Connection: which serial port?", &choices)?
                .as_deref()
            {
                Some("") => draft.port_name = ask.line("Port name, such as COM3")?,
                Some(port) => draft.port = Some(port.to_owned()),
                None => {}
            }
        }
        Some(TransportKind::TcpServer) => {
            draft.host = ask_listen_address(setup, ask)?;
            draft.tcp_port = ask.line("Port number, 1 to 65535")?;
        }
        Some(TransportKind::TcpClient) => {
            draft.host =
                ask.line("Step 2, Connection: the machine's IP address, such as 192.168.1.20")?;
            draft.tcp_port = ask.line("Port number, 1 to 65535")?;
        }
        Some(TransportKind::WatchedFolder) | None => {
            draft.folder = ask.line("Step 2, Connection: the folder Analyte reads results from")?;
        }
    }
    Ok(())
}

/// Step 2 for a machine that connects to Analyte: this PC's addresses and
/// every network card (0.0.0.0), as step 2 lists them, or a typed address.
fn ask_listen_address(setup: &Setup, ask: Ask) -> Result<Option<String>, Failure> {
    let found = match (setup.seams.addresses)() {
        Ok(found) => found,
        Err(error) => {
            say(&format::sentence(&error.to_string()))?;
            Vec::new()
        }
    };
    let mut choices: Vec<ask::Choice> = host_choices(&found, "")
        .into_iter()
        .map(|host| ask::Choice {
            value: host.value,
            label: format!("{} - {}", host.address, host.meta),
        })
        .collect();
    choices.push(ask::Choice {
        value: String::new(),
        label: "Type an address".into(),
    });
    let question = "Step 2, Connection: the machine connects to this PC. Which address does Analyte listen on?";
    Ok(match ask.choose(question, &choices)?.as_deref() {
        Some("") => ask.line("IP address, such as 192.168.1.20")?,
        chosen => chosen.map(str::to_owned),
    })
}

/// What a plan sets, in the Save step's words.
fn summary(id: &str, plan: &Plan) -> std::io::Result<()> {
    let effective = plan.validated.effective();
    let words = connection_words(Link::of(effective.stack()), &plan.connection);
    let destination = &effective.site().destination;
    say(&format!("Machine: {id}"))?;
    say(&format!("Model: {}", plan.model))?;
    say(&format!("Connection: {} ({})", words.shown, words.exact))?;
    if let Some(cable) = plan.cable() {
        say(&format!("Cable: {cable}"))?;
    }
    if let Some(choices) = plan.choices() {
        say(&format!("Analyzer settings: {choices}"))?;
    }
    if let Some(accepts) = accepts(Link::of(effective.stack()), plan.peers.as_ref()) {
        say(&format!("Devices: {accepts}"))?;
    }
    say(&format!(
        "Lab app: {} ({})",
        destination.name.as_str(),
        destination.base_url.as_str()
    ))
}

async fn set(
    installation: &Installation,
    given: Option<Identifier>,
    id: &str,
    choice: Choice,
    dry_run: bool,
    ask: Ask,
) -> Result<Outcome, Failure> {
    let setup = installation.setup.clone();
    let mut draft = choice.over(saved(&setup, id)?);
    keep_saved_choices(&setup, id, &mut draft)?;
    let filled = fill(&setup, id, &mut draft, ask)?;
    let asked = ask_peers(&setup, id, &mut draft, ask)? || filled;
    if dry_run {
        return preview(&setup, id, &draft);
    }
    let actor = actor(given, ask)?;
    let plan = plan::checked(&setup, id, &draft)?;
    if asked {
        summary(id, &plan)?;
        if !ask.confirm("Save?")? {
            return Err(Failure::NotConfirmed);
        }
    }
    save(installation, &actor, id, draft).await
}

/// Saves `draft` for `id` through the Save step's code, says what was saved
/// and records it under `actor`.
async fn save(
    installation: &Installation,
    actor: &Identifier,
    id: &str,
    draft: Draft,
) -> Result<Outcome, Failure> {
    let setup = installation.setup.clone();
    installation
        .with_store(async |store| {
            let (machine, kept) = (id.to_owned(), draft.clone());
            let (saved, missing) = on_files(&setup, move |setup| {
                let saved = plan::save_blocking(setup, &machine, &kept)?;
                let missing = lab_apps::missing_keys(setup, &saved.lab_app);
                Ok((saved, missing))
            })
            .await?;
            let words = connection_words(saved.link, &saved.connection);
            say(&format!("Saved {id}: {}.", saved.model))?;
            say(&format!("Connection: {} ({})", words.shown, words.exact))?;
            if let Some(cable) = &saved.cable {
                say(&format!("Cable: {cable}"))?;
            }
            if let Some(choices) = &saved.choices {
                say(&format!("Analyzer settings: {choices}"))?;
            }
            if let Some(accepts) = accepts(saved.link, saved.peers.as_ref()) {
                say(&format!("Devices: {accepts}"))?;
            }
            say(&format!(
                "Lab app: {} ({})",
                saved.lab_app.name.as_str(),
                saved.lab_app.base_url.as_str()
            ))?;
            if let Some(missing) = missing {
                say(&missing)?;
            }
            if saved.moved {
                say("This lab app is now kept on the Lab apps page; nothing else changed.")?;
            }
            say(&format!(
                "Settings version: {} ({})",
                format::digest_short(&saved.digest),
                format::digest_hex(&saved.digest)
            ))?;
            backups(&saved.backups)?;
            let outcome = record(
                store,
                actor,
                "setup-save",
                id,
                &saved.reason(Via::CommandLine),
            )
            .await?;
            restart_note()?;
            Ok(outcome)
        })
        .await
}

/// `--dry-run`: every check of the save, then the files it would write; none is written.
fn preview(setup: &Arc<Setup>, id: &str, draft: &Draft) -> Result<Outcome, Failure> {
    let plan = plan::checked(setup, id, draft)?;
    summary(id, &plan)?;
    say(&format!("Would write {}:", plan.installed.site.display()))?;
    say(&plan.site_text)?;
    if let Some(text) = &plan.lab_apps {
        say(&format!(
            "Would write {}:",
            crate::installation::config::lab_apps::path(setup.config()).display()
        ))?;
        say(text)?;
    }
    let changes_model = plan
        .installed
        .definition
        .as_deref()
        .is_none_or(|current| current != plan.definition);
    if changes_model {
        say(&format!(
            "Would set its model in {}: {}",
            setup.config().display(),
            plan.definition.display()
        ))?;
    }
    say("Dry run: nothing was written.")?;
    Ok(Outcome::Done)
}

/// How often the test's state is looked at while it runs.
const TICK: Duration = Duration::from_millis(250);

async fn test(
    installation: &Installation,
    given: Option<Identifier>,
    id: &str,
    choice: Choice,
    seconds: u64,
    show_bytes: bool,
    ask: Ask,
) -> Result<Outcome, Failure> {
    let setup = installation.setup.clone();
    let mut draft = choice.over(saved(&setup, id)?);
    keep_saved_choices(&setup, id, &mut draft)?;
    fill(&setup, id, &mut draft, ask)?;
    let plan = plan::plan(&setup, id, &draft)?;
    let stack = plan.validated.effective().stack();
    let family = verdict::Family::of(stack);
    let (label, target) = plan::listen_target(&setup, &plan)?;
    let route = verdict::route_of(&setup, &target);
    let limits = listen::Limits {
        duration: Duration::from_secs(seconds).min(setup.seams.limits.duration),
        bytes: setup.seams.limits.bytes,
    };
    if show_bytes {
        problem(concat!(
            "Warning: the bytes received are printed at the end. They may contain patient ",
            "data: do not save, paste or share this output."
        ))?;
    }
    say(&format!(
        "Testing {id} ({}): {label}, for up to {} s. Nothing is sent and nothing is stored. Press Ctrl+C to stop.",
        plan.model,
        limits.duration.as_secs()
    ))?;
    let (session, bytes) = if show_bytes {
        let (session, bytes) = listen::Session::start_showing(label, target, limits);
        (session, Some(bytes))
    } else {
        (listen::Session::start(label, target, limits), None)
    };
    let mut told = (false, false);
    let view = loop {
        tokio::select! {
            () = tokio::time::sleep(TICK) => {}
            _ = tokio::signal::ctrl_c() => session.stop(),
        }
        let view = session.view();
        if view.connected.is_some() && !told.0 {
            told.0 = true;
            say("Connected; listening.")?;
        }
        if view.closed && !told.1 {
            told.1 = true;
            say("The machine closed the connection.")?;
        }
        if view.end.is_some() {
            break view;
        }
    };
    let found = verdict::of_view(&view, family, &route);
    say(&format!("{}. {}", found.title, found.text))?;
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
            "; reading stopped at the {} limit",
            format::bytes(u64::try_from(limits.bytes).unwrap_or(u64::MAX))
        ));
    }
    say(&received)?;
    say(&format!(
        "Framing characters: {}",
        if view.framing.is_empty() {
            "none".to_owned()
        } else {
            format::control_names(&view.framing)
        }
    ))?;
    if let Some(bytes) = bytes
        && let Ok(bytes) = bytes.await
    {
        say("Bytes received (may contain patient data):")?;
        say(&shown(&bytes))?;
    }
    drop(session);
    let outcome = match (&view.end, found.tone) {
        (Some(listen::End::Failed(_)), _) => Outcome::Refused,
        (_, Tone::Good) => Outcome::Done,
        (_, Tone::Attention | Tone::Problem | Tone::Neutral) => Outcome::NotSeen,
    };
    match view.peer {
        Some(peer) => offer_peer(installation, given, id, draft, &plan, peer, ask).await,
        None => Ok(outcome),
    }
    .map(|saved| {
        if saved == Outcome::Done {
            outcome
        } else {
            saved
        }
    })
}

/// After a test of a machine that connects to Analyte, names the device that
/// connected and, at a terminal, offers to accept only that device from now
/// on, through the Save step's code (ADR 0026); a script is told the command.
async fn offer_peer(
    installation: &Installation,
    given: Option<Identifier>,
    id: &str,
    mut draft: Draft,
    plan: &Plan,
    peer: std::net::IpAddr,
    ask: Ask,
) -> Result<Outcome, Failure> {
    say(&format!("The device that connected: {peer}."))?;
    if plan.peers.as_ref().is_some_and(|peers| peers.allows(peer)) {
        say("It is one of the devices this machine accepts.")?;
        return Ok(Outcome::Done);
    }
    let command = format!("analyte machines set {id} --allowed-peer {peer}");
    if ask == Ask::Never {
        say(&format!(
            "Check that this is the analyzer's own address, then accept only it with: {command}"
        ))?;
        return Ok(Outcome::Done);
    }
    say("Check that this is the analyzer's own address in its network settings.")?;
    if !ask.confirm(&format!("Only accept this device ({peer}) from now on?"))? {
        say(&format!("Nothing was changed. To do it later: {command}"))?;
        return Ok(Outcome::Done);
    }
    draft.allowed_peers = Some(peer.to_string());
    let actor = actor(given, ask)?;
    let plan = plan::checked(&installation.setup, id, &draft)?;
    summary(id, &plan)?;
    save(installation, &actor, id, draft).await
}

/// The received bytes as text: printable ASCII as it is, every other byte by
/// name in angle brackets, a new line after each CR or LF.
fn shown(bytes: &[u8]) -> String {
    let mut text = String::new();
    for &byte in bytes {
        match byte {
            b' '..=b'~' => text.push(char::from(byte)),
            0x00..=0x1f | 0x7f => {
                // Already in angle brackets, such as <STX>.
                text.push_str(&format::control_names(&[byte]));
                if matches!(byte, b'\r' | b'\n') {
                    text.push('\n');
                }
            }
            other => text.push_str(&format!("<{other:02X}>")),
        }
    }
    text
}

async fn remove(
    installation: &Installation,
    actor: &Identifier,
    id: &str,
    anyway: bool,
    ask: Ask,
) -> Result<Outcome, Failure> {
    let setup = &installation.setup;
    installation
        .with_store(async |store| {
            let preview = preview_removal(setup, store, id).await?;
            say(&format!("Remove {id}?"))?;
            say(format::REMOVED_ADVICE)?;
            say(&preview.sending.shown)?;
            say(if preview.warned {
                "Still unfinished:"
            } else {
                "Before you remove it:"
            })?;
            for item in &preview.items {
                say(&format!("  - {}", item.text))?;
            }
            let mut anyway = anyway;
            if ask == Ask::Terminal {
                let question = if preview.warned && !anyway {
                    "Remove anyway?"
                } else {
                    "Remove it?"
                };
                if !ask.confirm(question)? {
                    return Err(Failure::NotConfirmed);
                }
                anyway = anyway || preview.warned;
            }
            let removed = remove_machine(setup, store, id, anyway).await?;
            say(&format!(
                "{id} is removed from the settings; its stored results and history stay."
            ))?;
            say(&format::sentence(&removed.kept_sentence()))?;
            say(&format::sentence(&removed.site_sentence()))?;
            backups(&removed.backups)?;
            let outcome = record(
                store,
                actor,
                "setup-remove",
                id,
                &removed.reason(Via::CommandLine, id),
            )
            .await?;
            restart_note()?;
            Ok(outcome)
        })
        .await
}

#[cfg(test)]
mod tests {
    use super::shown;

    // Owner request 2026-11-21 (command line): --show-bytes prints control
    // bytes by name and starts a new line after each segment end.
    #[test]
    fn shown_bytes_name_control_characters() {
        assert_eq!(shown(b"\x021H|\\^&\r\x03"), "<STX>1H|\\^&<CR>\n<ETX>");
        assert_eq!(shown(&[0xC3, b'a']), "<C3>a");
    }
}
