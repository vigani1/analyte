//! Lists configured machines and their live state in text or JSON.
use super::super::{Failure, Installation, Outcome, live, say};
use crate::admin::view::format;
use crate::admin::{
    server::Connection,
    setup::{cable, choices, model, plan, site},
};
use crate::machine::link::status::Link;
use analyte_config::SerialSetting;
use analyte_i18n::{message as tr_args, text as tr};

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
    /// How repeated results are handled, `automatic` or `always_ask` (ADR
    /// 0035); none while not loaded or where the choice does not apply.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in super::super) repeated_results: Option<analyte_config::RepeatedResults>,
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
        let listed_choices = chosen.unwrap_or_default().iter().map(listed).collect();
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
            repeated_results: effective.and_then(analyte_config::Effective::repeated_results),
            id,
        });
    }
    Ok(machines)
}

/// One of the model's own choices as `machines list` shows it.
fn listed(chosen: &analyte_config::Chosen) -> ListedChoice {
    ListedChoice {
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
    }
}

pub(super) async fn list(installation: &Installation, json: bool) -> Result<Outcome, Failure> {
    let mut machines = settings_rows(installation)?;
    let (engine, reason, mut rows, settings_read) = match live::read(&installation.program).await {
        live::Live::Running {
            settings_read,
            machines,
        } => ("running", None, machines, settings_read),
        live::Live::NotRunning => (tr("setup-not-running"), None, Vec::new(), true),
        live::Live::Unknown(why) => ("unknown", Some(why.to_string()), Vec::new(), true),
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
        ("running", _) => {
            tr("setup-engine-running-live-state-as-the-machines-page-shows").to_owned()
        }
        ("not running", _) => tr("setup-engine-not-running-live-state-unknown").to_owned(),
        (_, reason) => tr_args(
            "setup-live-state-unknown",
            [(
                "answer",
                (reason.as_deref().unwrap_or(tr("setup-no-answer")))
                    .to_string()
                    .into(),
            )],
        ),
    })?;
    if !settings_read {
        say(tr(
            "setup-the-engine-could-not-read-the-settings-file-machines",
        ))?;
    }
    if answer.machines.is_empty() {
        say(tr("setup-no-machine-is-listed-in-the-settings-add-one"))?;
    }
    for machine in &answer.machines {
        print_machine(machine, answer.engine == "running")?;
    }
    for row in &answer.removed {
        say(&tr_args(
            "setup-no-longer-in-the-settings",
            [("id", (row.id).to_string().into())],
        ))?;
        print_live(row)?;
    }
    Ok(Outcome::Done)
}

fn print_machine(machine: &Listed, engine: bool) -> std::io::Result<()> {
    let not = format::not_applicable();
    say(&format!("{}:", machine.id))?;
    say(&tr_args(
        "setup-list-model",
        [(
            "model",
            match (&machine.model, &machine.definition) {
                (Some(model), Some(path)) if model != path => format!("{model} ({path})"),
                (Some(model), _) => model.clone(),
                (None, _) => tr("setup-list-none-chosen-yet").to_owned(),
            }
            .into(),
        )],
    ))?;
    say(&tr_args(
        "setup-list-connection",
        [(
            "connection",
            machine.connection.as_deref().unwrap_or(not).into(),
        )],
    ))?;
    if let Some(cable) = &machine.cable {
        say(&tr_args(
            "setup-list-cable",
            [("cable", cable.words.as_str().into())],
        ))?;
    }
    if let Some(words) = &machine.choices_words {
        say(&tr_args(
            "setup-analyzer-settings",
            [("words", (words).to_string().into())],
        ))?;
    }
    if let Some(line) = devices_line(machine) {
        say(&line)?;
    }
    if let Some(choice) = machine.repeated_results {
        say(&tr_args(
            "setup-repeated-results",
            [("label", (choice.label()).to_string().into())],
        ))?;
    }
    say(&tr_args(
        "setup-lab-app-2",
        [(
            "not",
            (if machine.record_only {
                format::recording_only()
            } else {
                machine.lab_app.as_deref().unwrap_or(not)
            })
            .to_string()
            .into(),
        )],
    ))?;
    say(if machine.set_up {
        tr("setup-list-set-up")
    } else {
        tr("setup-list-not-set-up-yet")
    })?;
    say(&tr_args(
        "setup-settings-version",
        [(
            "not",
            (machine.settings_version.as_deref().unwrap_or(not))
                .to_string()
                .into(),
        )],
    ))?;
    match &machine.live {
        Some(row) => print_live(row),
        None if engine => say(tr("setup-state-the-engine-does-not-report-it")),
        None => say(tr("setup-list-state-unknown")),
    }
}

/// What the engine reported for one machine, in the page's words.
fn print_live(row: &live::Row) -> std::io::Result<()> {
    say(&tr_args(
        "setup-state",
        [
            ("status", (row.status).to_string().into()),
            (
                "unwrap-or-default",
                (row.note
                    .as_deref()
                    .map(|note| format!(" - {note}"))
                    .unwrap_or_default())
                .to_string()
                .into(),
            ),
        ],
    ))?;
    if let Some(since) = &row.since {
        say(&tr_args(
            "setup-open-since",
            [
                ("shown", (since.shown).to_string().into()),
                ("exact", (since.exact).to_string().into()),
            ],
        ))?;
    }
    say(&tr_args(
        "setup-last-data",
        [(
            "exact",
            (row.last_data.as_ref().map_or_else(
                || tr("setup-list-nothing-received").to_owned(),
                |at| format!("{} ({})", at.shown, at.exact),
            ))
            .to_string()
            .into(),
        )],
    ))?;
    if let Some(recording) = &row.recording {
        say(&tr_args(
            "setup-recorded",
            [("recording", (recording).to_string().into())],
        ))?;
    }
    match &row.refused {
        Some(refused) => say(&tr_args(
            "setup-latest-refusal",
            [("refused", (refused).to_string().into())],
        )),
        None => Ok(()),
    }
}

/// Which devices a machine that connects to Analyte accepts, as `machines
/// list` says it: the allowed devices, or a warning (ADR 0026); none for
/// other links.
pub(in super::super) fn devices_line(machine: &Listed) -> Option<String> {
    match (&machine.allowed_peers, machine.open_to_any_device) {
        (Some(peers), _) => Some(tr_args(
            "setup-list-devices",
            [("devices", format::only_devices(peers.join(", ")).into())],
        )),
        (None, true) => Some(tr_args(
            "setup-devices-warning-accept-only-the-analyzer-with-analyte-machines",
            [
                ("access", format::open_to_any_in_line().into()),
                ("id", (machine.id).to_string().into()),
            ],
        )),
        (None, false) => None,
    }
}
