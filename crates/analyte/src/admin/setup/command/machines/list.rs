//! Lists configured machines and their live state in text or JSON.
use super::super::{Failure, Installation, Outcome, live, say};
use crate::admin::view::format;
use crate::admin::{
    server::Connection,
    setup::{cable, choices, model, plan, site},
};
use crate::machine::link::status::Link;
use analyte_config::SerialSetting;

/// One machine of the settings file, read for `machines list`.
#[derive(serde::Serialize)]
pub(in super::super) struct Listed {
    pub(in super::super) id: String,
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
    pub(in super::super) cable: Option<ListedCable>,
    /// Each of the model's own choices the files load now (ADR 0032); empty
    /// when it declares none.
    pub(in super::super) choices: Vec<ListedChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in super::super) autolyser_poll_seconds: Option<u32>,
    /// The choices in words, with what differs from the suggestion.
    pub(in super::super) choices_words: Option<String>,
    /// Its site file asks for record only (ADR 0034): nothing is sent to a lab app.
    pub(in super::super) record_only: bool,
}

/// One of a model's own choices as `machines list` shows it.
#[derive(serde::Serialize)]
pub(in super::super) struct ListedChoice {
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
pub(in super::super) struct ListedCable {
    /// The values in use.
    settings: analyte_config::SerialValues,
    /// The settings whose value is not the model's suggestion.
    differs_from_suggestion: Vec<&'static str>,
    /// The values in words, with what differs.
    pub(in super::super) words: String,
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
pub(in super::super) fn settings_rows(installation: &Installation) -> Result<Vec<Listed>, Failure> {
    let setup = &installation.setup;
    let config = setup.config().to_owned();
    let root = setup.seams.definitions.clone();
    let (_, entries) = site::entries(&config)?;
    let mut machines = Vec::new();
    for entry in &entries {
        let id = entry.id.get_ref().clone();
        let definition = entry.definition.as_ref().map(|d| d.get_ref().clone());
        let loaded = installation
            .program
            .machines()
            .iter()
            .find(|machine| machine.id().as_str() == id);
        let effective = loaded.map(|machine| machine.snapshot().effective());
        let (connection, lab_app, peers, open_to_any_device, record_only) =
            if let Some(effective) = effective {
                let site = effective.site();
                let connection = Connection::of(Link::of(effective.stack()), &site.endpoint);
                (
                    Some(site::describe(&site.endpoint)),
                    site.destination
                        .as_ref()
                        .map(|app| app.name.as_str().to_owned()),
                    site.allowed_peers
                        .as_ref()
                        .map(|peers| peers.addresses().iter().map(ToString::to_string).collect()),
                    connection.accepts_any_device(site.allowed_peers.as_ref()),
                    effective.records_only(),
                )
            } else {
                let document = site::read_document(
                    &crate::installation::config::folder(&config).join(entry.site.get_ref()),
                )?;
                let table = document.table();
                let connection = ["usb_serial", "endpoint"].iter().find_map(|key| {
                    table
                        .get(*key)
                        .and_then(toml::Value::as_str)
                        .map(|value| format!("{key} = {value}"))
                });
                (
                    connection,
                    plan::current_lab_app(None, table),
                    None,
                    false,
                    document.records_only(),
                )
            };
        let version = loaded.map(|machine| format::digest_short(&machine.snapshot().digest()));
        let cable = effective.and_then(|effective| {
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
        let chosen = effective.map(analyte_config::Effective::choices);
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
        machines.push(Listed {
            open_to_any_device,
            allowed_peers: peers,
            model: model(&config, root.as_deref(), definition.as_deref()).map(|m| m.shown),
            definition,
            connection,
            lab_app,
            set_up: entry.definition.is_some(),
            settings_version: version,
            live: None,
            cable,
            choices: listed_choices,
            autolyser_poll_seconds: effective
                .and_then(analyte_config::Effective::site_autolyser_poll_seconds),
            choices_words: chosen.and_then(choices::summary),
            record_only,
            id,
        });
    }
    Ok(machines)
}

pub(super) async fn list(installation: &Installation, json: bool) -> Result<Outcome, Failure> {
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
        if machine.record_only {
            format::RECORDING_ONLY
        } else {
            machine.lab_app.as_deref().unwrap_or(not)
        }
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
    if let Some(recording) = &row.recording {
        say(&format!("  Recorded: {recording}"))?;
    }
    match &row.refused {
        Some(refused) => say(&format!("  Latest refusal: {refused}")),
        None => Ok(()),
    }
}

/// Which devices a machine that connects to Analyte accepts, as `machines
/// list` says it: the allowed devices, or a warning (ADR 0026); none for
/// other links.
pub(in super::super) fn devices_line(machine: &Listed) -> Option<String> {
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
