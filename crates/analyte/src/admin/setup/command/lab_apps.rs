//! The `lab-apps` commands: list, add, set, keys and remove, through the Lab apps page's own checks and writes.
//!
//! Keys are read only from a prompt that shows nothing as it is typed, or
//! from standard input, one per line; there is no option for them, so they
//! never sit in a command line, a shell history or a process list. They go to
//! the locked keys file exactly as the page stores them, and only when this
//! account may write it ([`crate::installation::credentials::Locked::check_writer`]).
use super::ask::Ask;
use super::{Failure, Installation, Outcome, actor, backups, record, restart_note, say};
use crate::admin::setup::{
    Seams, Setup, SetupError, Via,
    lab_apps::{
        API_KEY, Change, Entered, KeySource, SIGNING_KEY, Saved, add_blocking, conflict_sentence,
        count, reason, remove_blocking, removed_reason, save_blocking, scan, waiting,
        where_written,
    },
    on_files,
};
use crate::admin::view::format;
use crate::installation::credentials::Locked;
use analyte_model::Identifier;
use jiff::Timestamp;
use std::path::Path;

/// The `lab-apps` commands.
#[derive(clap::Subcommand)]
pub enum Command {
    /// Lists the lab apps: address, timing, the machines that use each, and
    /// where each key is found (never its value).
    List,
    /// Adds a lab app. Its two keys are then asked for without showing them,
    /// or read from standard input, one per line: the API key, then the signing key.
    Add {
        /// Letters A to Z, digits, - and _ (at most 64); it also names its keys.
        name: String,
        /// The choices; see the options below.
        #[command(flatten)]
        settings: Settings,
    },
    /// Changes a lab app's address or timing; values not given stay as they are.
    Set {
        /// The lab app's name.
        name: String,
        /// The choices; see the options below.
        #[command(flatten)]
        settings: Settings,
    },
    /// Replaces a lab app's keys, asked for without showing them or read from
    /// standard input (the API key, then the signing key, one per line). An
    /// empty answer keeps the stored key. Keys are never taken from options.
    Keys {
        /// The lab app's name.
        name: String,
    },
    /// Removes a lab app that no machine uses and nothing waits for.
    Remove {
        /// The lab app's name.
        name: String,
    },
}

/// A lab app's address and timing, as the Lab apps page's form takes them.
#[derive(clap::Args)]
pub struct Settings {
    /// Its HTTPS address, such as `https://lab.example.org`.
    #[arg(long)]
    address: Option<String>,
    /// How long its order list counts as current, in whole seconds.
    #[arg(long)]
    freshness_seconds: Option<String>,
    /// How long to wait for its answer, in whole seconds.
    #[arg(long)]
    request_timeout_seconds: Option<String>,
}

impl Settings {
    /// `entered` with the values given here.
    fn over(self, mut entered: Entered) -> Entered {
        if let Some(address) = self.address {
            entered.base_url = address;
        }
        if let Some(seconds) = self.freshness_seconds {
            entered.freshness_seconds = seconds;
        }
        if let Some(seconds) = self.request_timeout_seconds {
            entered.request_timeout_seconds = seconds;
        }
        entered
    }
}

/// Reads the named keys; at the console [`ask::keys`], in tests a fixed list.
pub(super) type Keys = Box<dyn FnOnce(&[&str]) -> std::io::Result<Vec<String>> + Send>;

pub(super) async fn run(
    config: &Path,
    given: Option<Identifier>,
    command: Command,
    ask: Ask,
    seams: Seams,
    keys: Keys,
) -> Result<Outcome, Failure> {
    let installation = Installation::load(config, seams)?;
    match command {
        Command::List => list(&installation.setup),
        Command::Add { name, settings } => {
            let actor = actor(given, ask)?;
            add(&installation, &actor, name, settings, ask, keys).await
        }
        Command::Set { name, settings } => {
            let actor = actor(given, ask)?;
            let current = current(&installation.setup, &name)?;
            let entered = settings.over(current);
            save(&installation, &actor, &name, entered).await
        }
        Command::Keys { name } => {
            let actor = actor(given, ask)?;
            let mut entered = current(&installation.setup, &name)?;
            let [api, signing] = read_keys(&installation.setup, keys)?;
            if api.is_empty() && signing.is_empty() {
                say("No key was entered, so nothing was stored.")?;
                return Ok(Outcome::Done);
            }
            entered.api_key = api;
            entered.signing_key = signing;
            save(&installation, &actor, &name, entered).await
        }
        Command::Remove { name } => {
            let actor = actor(given, ask)?;
            remove(&installation, &actor, &name, ask).await
        }
    }
}

/// The settings of lab app `name` as entered values, so a change keeps the rest.
fn current(setup: &Setup, name: &str) -> Result<Entered, SetupError> {
    let scan = scan(setup.config())?;
    let known = scan
        .get(name)
        .ok_or_else(|| SetupError::UnknownLabApp(name.into()))?;
    Ok(Entered::of(&known.settings))
}

/// Checks that this account may store keys, then reads both.
fn read_keys(setup: &Setup, keys: Keys) -> Result<[String; 2], Failure> {
    Locked::new(&setup.data_directory)
        .check_writer()
        .map_err(SetupError::Credentials)?;
    let mut keys = keys(&[API_KEY.0, SIGNING_KEY.0])?.into_iter();
    Ok([
        keys.next().unwrap_or_default(),
        keys.next().unwrap_or_default(),
    ])
}

fn list(setup: &Setup) -> Result<Outcome, Failure> {
    let scan = scan(setup.config())?;
    let source = KeySource::read(setup);
    let now = Timestamp::now();
    for conflict in &scan.conflicts {
        say(&conflict_sentence(conflict))?;
    }
    if scan.known.is_empty() {
        say(
            "No lab app is set up yet. Add one with: analyte lab-apps add <name> --address <https://...>",
        )?;
    }
    for known in &scan.known {
        let settings = &known.settings;
        say(&format!("{}:", settings.name.as_str()))?;
        say(&format!("  Address: {}", settings.base_url.as_str()))?;
        say(&format!(
            "  Order list current for: {} s",
            settings.freshness_seconds
        ))?;
        say(&format!(
            "  Request wait: {} s",
            settings.request_timeout_seconds
        ))?;
        say(&format!(
            "  Used by: {}",
            if known.users.is_empty() {
                "no machine yet".to_owned()
            } else {
                known.users.join(", ")
            }
        ))?;
        for (place, exact) in where_written(known) {
            say(&format!("  {place} ({exact})"))?;
        }
        for (label, name) in [
            (API_KEY, &settings.api_key_name),
            (SIGNING_KEY, &settings.signing_key_name),
        ] {
            let key = source.key(label, name, now);
            say(&format!("  {}: {} ({})", label.0, key.word, key.exact))?;
        }
    }
    Ok(Outcome::Done)
}

/// What a save wrote, as the page's saved page says it; never a key value.
fn print_saved(change: Change, saved: &Saved) -> std::io::Result<()> {
    say(&format!(
        "{} is {}.",
        saved.settings.name.as_str(),
        change.verb()
    ))?;
    say(&format!("Address: {}", saved.settings.base_url.as_str()))?;
    say(&if saved.users.is_empty() {
        "Used by no machine yet: pick it with analyte machines set <machine> --lab-app <name>."
            .to_owned()
    } else {
        format!("Used by {}.", saved.users.join(", "))
    })?;
    if !saved.changed {
        say("The settings were already the same; no file was written.")?;
    }
    for key in &saved.replaced {
        say(&format!("Key stored: {key}"))?;
    }
    if !saved.replaced.is_empty() {
        say(format::KEYS_APPLY)?;
    }
    backups(&saved.backups)
}

async fn add(
    installation: &Installation,
    actor: &Identifier,
    name: String,
    settings: Settings,
    ask: Ask,
    keys: Keys,
) -> Result<Outcome, Failure> {
    let setup = &installation.setup;
    let mut entered = settings.over(Entered {
        name,
        ..Entered::default()
    });
    let asked = entered.base_url.trim().is_empty() && ask == Ask::Terminal;
    if asked
        && let Some(address) = ask.line("Its HTTPS address, such as https://lab.example.org")?
    {
        entered.base_url = address;
    }
    let [api, signing] = read_keys(setup, keys)?;
    entered.api_key = api;
    entered.signing_key = signing;
    if asked {
        say(&format!("Lab app: {}", entered.name.trim()))?;
        say(&format!("Address: {}", entered.base_url.trim()))?;
        if !ask.confirm("Save?")? {
            return Err(Failure::NotConfirmed);
        }
    }
    installation
        .with_store(async |store| {
            let saved = on_files(setup, move |setup| add_blocking(setup, entered)).await?;
            print_saved(Change::Added, &saved)?;
            let name = saved.settings.name.as_str();
            let why = reason(Via::CommandLine, Change::Added, &saved);
            let outcome = record(store, actor, Change::Added.action(), name, &why).await?;
            restart_note()?;
            Ok(outcome)
        })
        .await
}

async fn save(
    installation: &Installation,
    actor: &Identifier,
    name: &str,
    entered: Entered,
) -> Result<Outcome, Failure> {
    let setup = &installation.setup;
    installation
        .with_store(async |store| {
            let lab_app = name.to_owned();
            let saved =
                on_files(setup, move |setup| save_blocking(setup, &lab_app, entered)).await?;
            print_saved(Change::Saved, &saved)?;
            let why = reason(Via::CommandLine, Change::Saved, &saved);
            let outcome = record(store, actor, Change::Saved.action(), name, &why).await?;
            restart_note()?;
            Ok(outcome)
        })
        .await
}

async fn remove(
    installation: &Installation,
    actor: &Identifier,
    name: &str,
    ask: Ask,
) -> Result<Outcome, Failure> {
    let setup = &installation.setup;
    if ask == Ask::Terminal && !ask.confirm(&format!("Remove lab app {name}?"))? {
        return Err(Failure::NotConfirmed);
    }
    installation
        .with_store(async |store| {
            let open = waiting(store).await.map(|open| count(&open, name));
            let lab_app = name.to_owned();
            let (kept, keys) =
                on_files(setup, move |setup| remove_blocking(setup, &lab_app, &open)).await?;
            say(&format!("{name} is no longer listed."))?;
            say(keys)?;
            backups(&kept)?;
            let why = removed_reason(Via::CommandLine, name);
            let outcome = record(store, actor, "lab-app-remove", name, &why).await?;
            restart_note()?;
            Ok(outcome)
        })
        .await
}

#[cfg(test)]
mod tests {
    use super::Command;
    use clap::Parser as _;

    #[derive(clap::Parser)]
    struct Cli {
        #[command(subcommand)]
        command: Command,
    }

    // AGENTS.md (never put values in command arguments): no lab-apps command
    // takes a key as an option, so a key never reaches a shell history.
    #[test]
    fn keys_cannot_be_given_as_arguments() {
        for attempt in [
            ["analyte", "keys", "demo", "--api-key", "secret"],
            ["analyte", "keys", "demo", "--signing-key", "whsec_x"],
            ["analyte", "add", "demo", "--api-key", "secret"],
            ["analyte", "set", "demo", "--signing-key", "whsec_x"],
        ] {
            assert!(Cli::try_parse_from(attempt).is_err(), "{attempt:?}");
        }
        assert!(Cli::try_parse_from(["analyte", "keys", "demo"]).is_ok());
    }
}
