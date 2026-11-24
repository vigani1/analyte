//! Runs the `machines` and `lab-apps` commands: the Setup and Lab apps pages' steps from a terminal, through the same checks and writes.
//!
//! Every change goes through the code the pages use: the same validation, the
//! dated backups, the safe write, the reload with roll back, and an audit row
//! that names `--actor` and says the command line made it. A missing choice is
//! asked for only at an interactive terminal ([`ask`]); a script gets the
//! page's refusal instead. Keys are read only from a prompt that does not echo
//! or from standard input, never from arguments. As on the page, changes apply
//! after a restart; the command says so and does not restart the service.
mod ask;
mod lab_apps;
mod live;
mod machines;
#[cfg(test)]
mod tests;

pub use lab_apps::Command as LabApps;
pub use machines::Command as Machines;

use super::{Seams, Setup, SetupError, audit_as};
use crate::admin::view::format;
use crate::installation::config::Program;
use analyte_model::Identifier;
use std::{
    io::{self, Write as _},
    path::Path,
    process::ExitCode,
    sync::Arc,
};

/// How a `machines` or `lab-apps` command ended; each has its own exit code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Done (0); for a test, it saw the model's own format.
    Done,
    /// Refused or failed (1); nothing was changed.
    Refused,
    /// The change was written but a later part failed (3): the audit row or
    /// storing the keys. The message says which.
    Partly,
    /// The connection test ran but did not see the model's own format (4).
    NotSeen,
}

impl Outcome {
    /// The process exit code; clap itself exits with 2 for wrong usage.
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::Done => 0,
            Self::Refused => 1,
            Self::Partly => 3,
            Self::NotSeen => 4,
        }
    }
}

impl From<Outcome> for ExitCode {
    fn from(outcome: Outcome) -> Self {
        Self::from(outcome.code())
    }
}

/// Why a command stopped; printed as "Fix this: …".
#[derive(Debug, thiserror::Error)]
enum Failure {
    #[error("{0}")]
    Setup(#[from] SetupError),
    #[error("Analyte's settings do not load: {0}")]
    Settings(#[source] crate::installation::config::Error),
    #[error("Analyte's stored data could not be opened: {0}")]
    Store(#[source] crate::runtime::Error),
    #[error("listing serial ports failed: {0}")]
    Ports(#[source] io::Error),
    #[error("the console could not be used: {0}")]
    Console(#[from] io::Error),
    #[error("name who makes this change with --actor <name>; it is recorded in the audit trail")]
    NoActor,
    #[error(transparent)]
    BadActor(#[from] crate::admin::ActorError),
    #[error("nothing was saved: it was not confirmed")]
    NotConfirmed,
    /// ADR 0028: the engine is not running and this account may not write the locked keys file.
    #[error("the keys can be stored only by the Analyte engine, which is not running ({0})")]
    EngineStopped(#[source] crate::installation::credentials::CredentialError),
    /// The engine could not be asked; nothing was sent.
    #[error("the keys were not sent: {0}")]
    EngineUnreachable(String),
    /// The engine refused the change with the Lab apps page's reason.
    #[error("{0}")]
    EngineRefused(String),
    /// The engine wrote the settings but could not store the keys.
    #[error("{0}")]
    EngineKeysNotStored(String),
    /// The request may have reached the engine, but no answer came.
    #[error("whether the engine stored the change is unknown: {0}")]
    EngineNoAnswer(String),
}

impl Failure {
    /// What to do next, when the reason alone does not say it.
    fn advice(&self) -> Option<&'static str> {
        match self {
            Self::Setup(SetupError::Credentials(_) | SetupError::KeysNotStored(_)) => Some(
                "Run this command as the account the Analyte service runs as, or set the keys on the Lab apps page.",
            ),
            Self::Setup(SetupError::UnfinishedWork(..)) => {
                Some("To remove it regardless, run the command again with --anyway.")
            }
            Self::Setup(SetupError::HistoryNotConfirmed(_)) => {
                Some("If it is the same analyzer, run the command again with --same-analyzer.")
            }
            Self::Settings(_) => Some("See what is wrong with: analyte check-config <file>."),
            Self::EngineStopped(_) => Some(
                "Start the engine with Start-Service Analyte in an administrator PowerShell, then run this command again.",
            ),
            Self::EngineNoAnswer(_) => {
                Some("Check with analyte lab-apps list before running the command again.")
            }
            Self::Setup(_)
            | Self::Store(_)
            | Self::Ports(_)
            | Self::Console(_)
            | Self::NoActor
            | Self::BadActor(_)
            | Self::NotConfirmed
            | Self::EngineUnreachable(_)
            | Self::EngineRefused(_)
            | Self::EngineKeysNotStored(_) => None,
        }
    }

    /// The change was already written when this failed.
    const fn after_writing(&self) -> bool {
        matches!(
            self,
            Self::Setup(SetupError::KeysNotStored(_))
                | Self::EngineKeysNotStored(_)
                | Self::EngineNoAnswer(_)
        )
    }
}

/// The installation a command works on: the loaded program and its setup files.
struct Installation {
    program: Program,
    setup: Arc<Setup>,
}

impl Installation {
    /// Loads the program as `check-config` does. Every machine it loads counts
    /// as running, so a test never takes a port the service may hold.
    fn load(config: &Path, seams: Seams) -> Result<Self, Failure> {
        let program = Program::load(config).map_err(Failure::Settings)?;
        let setup = Arc::new(Setup::new(&program, seams));
        Ok(Self { program, setup })
    }

    /// [`Installation::load`], with what the engine runs now: a connection
    /// test must not take a port it holds, and may use any other. When the
    /// engine cannot say, every machine the settings load still counts, and
    /// the returned line says why.
    async fn load_running(config: &Path, seams: Seams) -> Result<(Self, Option<String>), Failure> {
        let program = Program::load(config).map_err(Failure::Settings)?;
        let mut setup = Setup::new(&program, seams);
        let note = match live::read(&program).await {
            live::Live::Running { machines, .. } => {
                setup.keep_running(|id| machines.iter().any(|row| row.running && row.id == id));
                None
            }
            live::Live::NotRunning => {
                setup.keep_running(|_| false);
                None
            }
            live::Live::Unknown(why) => Some(format!(
                "What the engine runs is unknown ({why}), so every machine in the settings counts as running."
            )),
        };
        let setup = Arc::new(setup);
        Ok((Self { program, setup }, note))
    }

    /// Opens the store, runs `work` with it and closes it again.
    async fn with_store<T>(
        &self,
        work: impl AsyncFnOnce(&analyte_store::Handle) -> Result<T, Failure>,
    ) -> Result<T, Failure> {
        let mut store = crate::open_system_store(&self.program)
            .await
            .map_err(Failure::Store)?;
        let result = work(&store).await;
        let closed = match crate::deadline() {
            Ok(until) => store
                .shutdown(until)
                .await
                .map_err(|error| error.to_string()),
            Err(error) => Err(error.to_string()),
        };
        if let Err(error) = closed {
            tracing::warn!(%error, "the store did not close cleanly after a setup command");
        }
        result
    }
}

/// The person a change is recorded under: `--actor`, or asked for at a terminal.
fn actor(given: Option<Identifier>, ask: ask::Ask) -> Result<Identifier, Failure> {
    if let Some(actor) = given {
        return Ok(actor);
    }
    let Some(name) = ask.line("Your name for the audit record")? else {
        return Err(Failure::NoActor);
    };
    Ok(crate::admin::command_line_actor(&name)?)
}

/// Records the change and says what happened if the record could not be
/// written: the change itself stays, as on the page.
async fn record(
    store: &analyte_store::Handle,
    actor: &Identifier,
    action: &str,
    id: &str,
    reason: &str,
) -> Result<Outcome, Failure> {
    match audit_as(store, actor, action, id, reason).await {
        Ok(()) => Ok(Outcome::Done),
        Err(error) => {
            problem(&format::sentence(&format!(
                "the change was saved, but {error}"
            )))?;
            Ok(Outcome::Partly)
        }
    }
}

/// Writes one line of the command's result to standard output.
fn say(text: &str) -> io::Result<()> {
    crate::operator::display::line(text)
}

/// Writes one line about a problem to standard error.
fn problem(text: &str) -> io::Result<()> {
    writeln!(io::stderr().lock(), "{text}")
}

/// How changes take effect, said after every change, as the page says it.
fn restart_note() -> io::Result<()> {
    say(&format!(
        "{} On Windows run Restart-Service Analyte in an administrator PowerShell, \
         or press Restart now on the Setup page.",
        super::RESTART_REQUIRED
    ))
}

/// The dated copies a change kept, one line each.
fn backups(kept: &[std::path::PathBuf]) -> io::Result<()> {
    for line in super::copies(kept) {
        say(&line)?;
    }
    Ok(())
}

/// Prints why `failure` stopped the command and turns it into its outcome.
fn finish(result: Result<Outcome, Failure>) -> Outcome {
    match result {
        Ok(outcome) => outcome,
        Err(failure) => {
            // A closed console cannot be told either; the exit code still says it.
            let _ = problem(&format!(
                "Fix this: {}",
                format::sentence(&failure.to_string())
            ));
            if let Some(advice) = failure.advice() {
                let _ = problem(advice);
            }
            if failure.after_writing() {
                Outcome::Partly
            } else {
                let _ = problem("Nothing was changed.");
                Outcome::Refused
            }
        }
    }
}

/// `config` as a full path, as the service holds it, so files beside it are
/// named as the page names them; as given when the current folder is unknown.
fn absolute(config: &Path) -> std::path::PathBuf {
    std::path::absolute(config).unwrap_or_else(|_| config.to_owned())
}

/// Runs one `machines` command on the program file `config`.
pub async fn run_machines(config: &Path, actor: Option<Identifier>, command: Machines) -> Outcome {
    let config = &absolute(config);
    let seams = Seams::system(config);
    finish(machines::run(config, actor, command, ask::Ask::detect(), seams).await)
}

/// Runs one `lab-apps` command on the program file `config`.
pub async fn run_lab_apps(config: &Path, actor: Option<Identifier>, command: LabApps) -> Outcome {
    let config = &absolute(config);
    let seams = Seams::system(config);
    let ask = ask::Ask::detect();
    let keys: lab_apps::Keys = Box::new(move |names| ask::keys(ask, names));
    finish(lab_apps::run(config, actor, command, ask, seams, keys).await)
}
