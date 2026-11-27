//! The `lab-apps` commands: list, add, set, keys and remove, through the Lab apps page's own checks and writes.
//!
//! Keys are read only from a prompt that shows nothing as it is typed, or
//! from standard input, one per line; there is no option for them, so they
//! never sit in a command line, a shell history or a process list. They go to
//! the locked keys file exactly as the page stores them. While the engine
//! runs, the engine stores them, after it proved it holds the admin key
//! (ADR 0028); otherwise the command stores them itself, only when this
//! account may write that file ([`crate::installation::credentials::Locked::check_writer`]).
//! Where they go is settled before they are asked for.
use super::ask::Ask;
use super::{Failure, Installation, Outcome, actor, backups, problem, record, restart_note, say};
use crate::admin::engine::{Engine, EngineError};
use crate::admin::setup::{
    Seams, Setup, SetupError, Via,
    lab_apps::{
        API_KEY, KeySource, SIGNING_KEY,
        change::{
            Change, Entered, Saved, add_blocking, count, reason, remove_blocking, removed_reason,
            save_blocking, saved_lines, waiting,
        },
        conflict_sentence,
        relay::{Answer, PATH, Sent},
        scan, where_written,
    },
    off_thread,
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
    /// Whether to send it Needs review notices: `yes` or `no` (ADR 0038). A
    /// new lab app gets none unless this says yes.
    #[arg(long, value_parser = ["yes", "no"])]
    review_notices: Option<String>,
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
        if let Some(choice) = self.review_notices {
            entered.review_notices = choice;
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
            let place = keys_go(&installation).await?;
            let [api, signing] = read_keys(keys)?;
            if api.is_empty() && signing.is_empty() {
                say("No key was entered, so nothing was stored.")?;
                return Ok(Outcome::Done);
            }
            entered.api_key = api;
            entered.signing_key = signing;
            match place {
                Place::Engine(engine) => {
                    through_engine(&engine, &actor, Change::Saved, entered).await
                }
                Place::Here => save(&installation, &actor, &name, entered).await,
            }
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

/// Where a lab app's keys are stored.
enum Place {
    /// The running engine stores them (ADR 0028).
    Engine(Engine),
    /// The engine is not running, and this account may write the locked file.
    Here,
}

/// Where the keys will go, settled before they are asked for: the running
/// engine once it proved itself; with no engine, here, if this account may
/// write the locked file. Anything else stores nothing and says why.
async fn keys_go(installation: &Installation) -> Result<Place, Failure> {
    let here = || {
        Locked::new(&installation.setup.data_directory)
            .check_writer()
            .map(|()| Place::Here)
    };
    match Engine::proven(&installation.program).await {
        Ok(engine) => Ok(Place::Engine(engine)),
        Err(EngineError::NotRunning(_)) => here().map_err(Failure::EngineStopped),
        Err(EngineError::AdminOff) => Ok(here().map_err(SetupError::Credentials)?),
        Err(error) => Err(Failure::EngineUnreachable(error.to_string())),
    }
}

/// Reads both keys.
fn read_keys(keys: Keys) -> Result<[String; 2], Failure> {
    let mut keys = keys(&[API_KEY.0, SIGNING_KEY.0])?.into_iter();
    Ok([
        keys.next().unwrap_or_default(),
        keys.next().unwrap_or_default(),
    ])
}

/// The largest answer read from the engine's lab app write.
const ANSWER_LIMIT: usize = 65_536;

/// Has the running engine make `change` of `entered`, keys included, and
/// says what it stored, as a save here says it (ADR 0028).
async fn through_engine(
    engine: &Engine,
    actor: &Identifier,
    change: Change,
    entered: Entered,
) -> Result<Outcome, Failure> {
    let sent = Sent::new(actor.as_str(), change, entered);
    let posted = engine.post(PATH, &sent, ANSWER_LIMIT).await;
    drop(sent);
    let (status, body) = posted.map_err(|error| {
        if error.not_sent() {
            Failure::EngineUnreachable(error.to_string())
        } else {
            Failure::EngineNoAnswer(error.to_string())
        }
    })?;
    if status == 401 {
        return Err(Failure::EngineUnreachable(format!(
            "the engine refused the admin key ({status})"
        )));
    }
    let answer = serde_json::from_slice::<Answer>(&body).map_err(|_| {
        Failure::EngineNoAnswer(format!("the engine answered {status} in an unknown form"))
    })?;
    match answer {
        Answer::Saved {
            lines,
            audit_failed,
        } => {
            for line in &lines {
                say(line)?;
            }
            restart_note()?;
            match audit_failed {
                None => Ok(Outcome::Done),
                Some(error) => {
                    problem(&format::sentence(&format!(
                        "the change was saved, but {error}"
                    )))?;
                    Ok(Outcome::Partly)
                }
            }
        }
        Answer::Refused {
            reason,
            keys_not_stored: true,
        } => Err(Failure::EngineKeysNotStored(reason)),
        Answer::Refused {
            reason,
            keys_not_stored: false,
        } => Err(Failure::EngineRefused(reason)),
    }
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
            "  Send Needs review notices: {}",
            crate::admin::setup::lab_apps::change::notices_word(settings.review_notices)
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
    for line in saved_lines(change, saved) {
        say(&line)?;
    }
    Ok(())
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
    let place = keys_go(installation).await?;
    let [api, signing] = read_keys(keys)?;
    entered.api_key = api;
    entered.signing_key = signing;
    if asked {
        say(&format!("Lab app: {}", entered.name.trim()))?;
        say(&format!("Address: {}", entered.base_url.trim()))?;
        if !ask.confirm("Save?")? {
            return Err(Failure::NotConfirmed);
        }
    }
    if let Place::Engine(engine) = place {
        return through_engine(&engine, actor, Change::Added, entered).await;
    }
    installation
        .with_store(async |store| {
            let saved = off_thread(setup, move |setup| add_blocking(setup, entered)).await?;
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
                off_thread(setup, move |setup| save_blocking(setup, &lab_app, entered)).await?;
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
                off_thread(setup, move |setup| remove_blocking(setup, &lab_app, &open)).await?;
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
