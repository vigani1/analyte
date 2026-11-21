//! The `machines` commands: list, models, ports, add, set, test and remove, through the Setup page's own checks and writes.
use super::ask::{self, Ask};
use super::{
    Failure, Installation, Outcome, actor, backups, live, problem, record, restart_note, say,
};
use crate::admin::setup::{
    Definition, Running, Seams, Setup, SetupError, Shipped, Via, definitions, lab_apps,
    level_and_state,
    machines::{add_machine, preview_removal, remove_machine},
    model, on_files,
    plan::{self, KEEP, Plan},
    site,
    wizard::{Draft, connection_words, listen, verdict},
};
use crate::admin::view::{Tone, format};
use crate::machine::link::status::Link;
use analyte_config::TransportKind;
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
    /// Adds a machine to the settings; it is not set up yet.
    Add {
        /// Letters A to Z, digits, - and _; it also names its settings file.
        name: String,
        /// The name has stored history and this is the same analyzer.
        #[arg(long)]
        same_analyzer: bool,
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
}

impl Choice {
    /// `draft` with these choices applied; a new connection replaces the saved one.
    fn over(self, mut draft: Draft) -> Draft {
        let connection = self.serial_port.is_some()
            || self.usb_serial.is_some()
            || self.address.is_some()
            || self.folder.is_some();
        if connection {
            draft = Draft {
                definition: draft.definition,
                lab_app: draft.lab_app,
                ..Draft::default()
            };
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
        } => {
            let installation = Installation::load(config, seams)?;
            let actor = actor(given, ask)?;
            add(&installation, &actor, &name, same_analyzer, ask).await
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
            test(&installation, &name, choice, seconds, show_bytes, ask).await
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
struct Listed {
    id: String,
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

async fn list(installation: &Installation, json: bool) -> Result<Outcome, Failure> {
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
        let version = installation
            .program
            .machines()
            .iter()
            .find(|machine| machine.id().as_str() == id)
            .map(|machine| format::digest_short(&machine.snapshot().digest()));
        machines.push(Listed {
            model: model(&config, root.as_deref(), definition.as_deref()).map(|m| m.shown),
            definition,
            connection,
            lab_app: plan::current_lab_app(running(setup, &id), &table),
            set_up: entry.definition.is_some(),
            settings_version: version,
            live: None,
            id,
        });
    }
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
    ))
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
    same_analyzer: bool,
    ask: Ask,
) -> Result<Outcome, Failure> {
    let setup = &installation.setup;
    installation
        .with_store(async |store| {
            let added = match add_machine(setup, store, name, same_analyzer).await {
                Err(SetupError::HistoryNotConfirmed(id)) if ask == Ask::Terminal => {
                    say(&format::sentence(
                        &SetupError::HistoryNotConfirmed(id).to_string(),
                    ))?;
                    if !ask.confirm("Is it the same analyzer?")? {
                        return Err(Failure::NotConfirmed);
                    }
                    add_machine(setup, store, name, true).await?
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
        ..Draft::default()
    };
    let kind = installed.definition.as_deref().and_then(plan::kind_of);
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
        ..Draft::default()
    };
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
        Some(kind @ (TransportKind::TcpServer | TransportKind::TcpClient)) => {
            draft.host = ask.line(if kind == TransportKind::TcpServer {
                "Step 2, Connection: this PC's IP address the machine connects to, such as 192.168.1.20"
            } else {
                "Step 2, Connection: the machine's IP address, such as 192.168.1.20"
            })?;
            draft.tcp_port = ask.line("Port number, 1 to 65535")?;
        }
        Some(TransportKind::WatchedFolder) | None => {
            draft.folder = ask.line("Step 2, Connection: the folder Analyte reads results from")?;
        }
    }
    Ok(())
}

/// What a plan sets, in the Save step's words.
fn summary(id: &str, plan: &Plan) -> std::io::Result<()> {
    let effective = plan.validated.effective();
    let words = connection_words(Link::of(effective.stack()), &plan.connection);
    let destination = &effective.site().destination;
    say(&format!("Machine: {id}"))?;
    say(&format!("Model: {}", plan.model))?;
    say(&format!("Connection: {} ({})", words.shown, words.exact))?;
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
    let asked = fill(&setup, id, &mut draft, ask)?;
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
                &actor,
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
    id: &str,
    choice: Choice,
    seconds: u64,
    show_bytes: bool,
    ask: Ask,
) -> Result<Outcome, Failure> {
    let setup = installation.setup.clone();
    let mut draft = choice.over(saved(&setup, id)?);
    fill(&setup, id, &mut draft, ask)?;
    let plan = plan::plan(&setup, id, &draft)?;
    let stack = plan.validated.effective().stack();
    let family = verdict::Family::of(stack);
    let (label, target) = plan::listen_target(&setup, &plan)?;
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
    let found = verdict::of_view(&view, family);
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
    Ok(match (&view.end, found.tone) {
        (Some(listen::End::Failed(_)), _) => Outcome::Refused,
        (_, Tone::Good) => Outcome::Done,
        (_, Tone::Attention | Tone::Problem | Tone::Neutral) => Outcome::NotSeen,
    })
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
