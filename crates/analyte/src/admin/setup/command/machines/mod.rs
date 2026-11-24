//! The `machines` commands: list, models, ports, add, set, test and remove, through the Setup page's own checks and writes.
mod args;
mod list;
mod questions;
mod test;

use super::ask::Ask;
use super::{Failure, Installation, Outcome, actor, backups, record, restart_note, say};
use crate::admin::setup::{
    Seams, Setup, SetupError, Shipped, Via, definitions, lab_apps, level_and_state,
    machines::{add_machine, preview_removal, remove_machine},
    off_thread,
    plan::{self, Plan},
    wizard::{Draft, connection_words},
};
use crate::admin::view::format;
use crate::machine::link::status::Link;
use analyte_model::Identifier;
use analyte_transport::serial;
use args::{Cable, Choice};
use list::list;
#[cfg(test)]
pub(super) use list::{devices_line, settings_rows};
use questions::{ask_peers, fill, port_line};
use std::{path::Path, sync::Arc};
use test::test;

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
        if let Shipped::Ready { source, .. } = &definition.state {
            if source.details.asks_autolyser_poll {
                let range = analyte_config::AUTOLYSER_POLL_SECONDS;
                say(&format!(
                    "  --poll-seconds <{}..{}>: seconds between report polls (required)",
                    range.start(),
                    range.end(),
                ))?;
            }
            for choice in &source.details.choices {
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
        if let Shipped::Blocked {
            record: Some(source),
        } = &definition.state
            && let Some(mode) = &source.details.analyzer_mode
        {
            say(&format!(
                "  --record-only --analyzer-mode <{}>: {}",
                mode.allowed().replace(", ", "|"),
                mode.label.as_str()
            ))?;
        }
    }
    Ok(Outcome::Done)
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

/// What a plan sets, in the Save step's words.
fn summary(id: &str, plan: &Plan) -> std::io::Result<()> {
    let effective = plan.validated.effective();
    say(&format!("Machine: {id}"))?;
    say(&format!("Model: {}", plan.model))?;
    settings_summary(
        Link::of(effective.stack()),
        &plan.connection,
        plan.cable().as_deref(),
        plan.choices().as_deref(),
        plan.peers.as_ref(),
        effective.site().destination.as_ref(),
    )
}

/// Prints the settings in the same words on a dry run and after a save.
fn settings_summary(
    link: Link,
    connection: &analyte_config::Endpoint,
    cable: Option<&str>,
    choices: Option<&str>,
    peers: Option<&analyte_config::AllowedPeers>,
    destination: Option<&analyte_config::Destination>,
) -> std::io::Result<()> {
    let words = connection_words(link, connection);
    say(&format!("Connection: {} ({})", words.shown, words.exact))?;
    if let Some(cable) = cable {
        say(&format!("Cable: {cable}"))?;
    }
    if let Some(choices) = choices {
        say(&format!("Analyzer settings: {choices}"))?;
    }
    if let Some(accepts) = accepts(link, peers) {
        say(&format!("Devices: {accepts}"))?;
    }
    match destination {
        Some(destination) => say(&format!(
            "Lab app: {} ({})",
            destination.name.as_str(),
            destination.base_url.as_str()
        )),
        None => say(&format!("Lab app: {}", format::RECORDING_ONLY)),
    }
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
    let mut draft = args::draft(&setup, id, choice)?;
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
            let (saved, missing) = off_thread(&setup, move |setup| {
                let saved = plan::save_blocking(setup, &machine, &kept)?;
                let missing = saved
                    .lab_app
                    .as_ref()
                    .and_then(|lab_app| lab_apps::missing_keys(setup, lab_app));
                Ok((saved, missing))
            })
            .await?;
            say(&format!("Saved {id}: {}.", saved.model))?;
            settings_summary(
                saved.link,
                &saved.connection,
                saved.cable.as_deref(),
                saved.choices.as_deref(),
                saved.peers.as_ref(),
                saved.lab_app.as_ref(),
            )?;
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
