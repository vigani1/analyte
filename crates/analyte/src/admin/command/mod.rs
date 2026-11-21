//! The admin pages on the command line: the Overview's verdict, Activity, active alerts, Data keeping and Cancelled by machine in the pages' own words, plus `doctor` and `logs` for IT.
//!
//! Each command reuses its page's reads and wording, so a page and its
//! command never disagree; only the layout differs. What only the running
//! engine knows (each machine's live link, whether its service runs) comes
//! from the engine itself through [`engine`]. Output holds names, counts and,
//! where the page shows them, sample IDs; never result values, keys or tokens.
pub(in crate::admin) mod doctor;
pub(in crate::admin) mod engine;
mod logs;
mod overview;
mod pages;

use crate::installation::config::Program;
use analyte_model::Identifier;
use clap::Subcommand;
use std::{collections::BTreeMap, path::Path, process::ExitCode};

use crate::admin::Error;

/// The most lines one `activity` page prints.
const ACTIVITY_LIMIT: u16 = 200;

/// The commands that show what the admin pages show, and IT's checks.
#[derive(Subcommand)]
pub enum Command {
    /// The Overview in plain words: "All good", or each thing that needs you
    /// with the page and the command that fixes it. Asks the running engine.
    /// Exit code: 0 all good, 1 needs a look, 2 a problem stops data, 3 unknown.
    Overview {
        /// Print the answer as JSON for scripts.
        #[arg(long)]
        json: bool,
    },
    /// The Activity page: messages stored, delivery tries (failed tries in a
    /// row as one line), orders and changes, newest first. Lines name sample
    /// IDs and counts, never result values; each run is recorded in the audit.
    Activity {
        /// How many lines to print (1 to 200).
        #[arg(long, default_value_t = 50, value_parser = clap::value_parser!(u16).range(1..=i64::from(ACTIVITY_LIMIT)))]
        limit: u16,
        /// Only this machine's lines.
        #[arg(long, value_parser = Identifier::new)]
        machine: Option<Identifier>,
        /// Continue before this position, as the previous page printed it.
        #[arg(long, value_parser = crate::admin::activity::parse_position)]
        before: Option<analyte_store::ActivityCursor>,
        /// Print the lines as JSON for scripts.
        #[arg(long)]
        json: bool,
    },
    /// Active alerts, each with its reason and where it is fixed.
    Alerts {
        #[command(subcommand)]
        /// What to do with the alerts.
        command: Alerts,
    },
    /// The Data keeping page: how long each machine's messages are kept, the
    /// last clean-up, the oldest message kept and the active holds.
    DataKeeping {
        /// Continue after this hold number, as the previous page printed it.
        #[arg(long, default_value_t = 0)]
        after: u64,
        /// Print the page as JSON for scripts.
        #[arg(long)]
        json: bool,
    },
    /// A read-only health check for IT: the program file, the engine, each
    /// machine's connection, each lab app, disk space and Windows sign-in.
    /// Every line says OK, WARN or FAIL and what to do. It changes nothing.
    /// Exit code: 0 all OK, 1 a warning, 2 a failure.
    Doctor {
        /// Print the checks as JSON for scripts.
        #[arg(long)]
        json: bool,
    },
    /// Where the engine's log is, and its last lines. The log holds no patient
    /// data: Analyte logs names, counts and IDs, never messages or values.
    Logs {
        /// How many of the last lines to print (1 to 1000).
        #[arg(long, default_value_t = 50, value_parser = clap::value_parser!(u16).range(1..=1000))]
        tail: u16,
    },
}

/// What `alerts` does.
#[derive(Subcommand)]
pub enum Alerts {
    /// Lists every active alert.
    List {
        /// Print the alerts as JSON for scripts.
        #[arg(long)]
        json: bool,
    },
}

/// Runs `command` with the program file at `config`, recording `actor` in
/// the audit for reads that show sample IDs. The exit code says how things
/// are for `overview` and `doctor`, and success for the others.
/// # Errors
/// Reports a program file that cannot be loaded, store failures and output
/// errors; `overview` and `doctor` report an engine that does not answer in
/// their output and exit code instead.
pub async fn run(config: &Path, actor: Identifier, command: Command) -> Result<ExitCode, Error> {
    let shown = match command {
        Command::Overview { json } => return overview::run(&Program::load(config)?, json).await,
        Command::Doctor { json } => return doctor::run(config, json).await,
        Command::Logs { tail } => logs::run(&Program::load(config)?, tail.into()).await,
        Command::Activity {
            limit,
            machine,
            before,
            json,
        } => {
            let page = pages::Page {
                machine,
                before,
                limit: limit.into(),
            };
            pages::activity(&Program::load(config)?, actor, page, json).await
        }
        Command::Alerts {
            command: Alerts::List { json },
        } => pages::alerts(&Program::load(config)?, json).await,
        Command::DataKeeping { after, json } => {
            pages::keeping(&Program::load(config)?, after, json).await
        }
    };
    shown.map(|()| ExitCode::SUCCESS)
}

/// Cancelled by machine for `analyte orders cancelled`: one page after the
/// store's cursor `after`, recorded in the audit as `actor`'s, since it
/// shows sample IDs like the page.
/// # Errors
/// Reports store, audit and output failures.
pub async fn cancelled(
    program: &Program,
    actor: Identifier,
    after: u64,
    json: bool,
) -> Result<(), Error> {
    pages::cancelled(program, actor, after, json).await
}

/// Each set-up machine's model, by ID, as the pages name it.
fn models(program: &Program) -> BTreeMap<String, String> {
    let definitions = crate::admin::setup::Seams::system(&program.source_path).definitions;
    program
        .machines()
        .iter()
        .filter_map(|machine| {
            crate::admin::setup::model_name(&machine.definition, definitions.as_deref())
                .map(|model| (machine.id().as_str().to_owned(), model))
        })
        .collect()
}

/// Writes one line of output.
fn say(text: &str) -> Result<(), Error> {
    Ok(crate::operator::display::line(text)?)
}

/// Writes `value` as one JSON document.
fn json<T: serde::Serialize>(value: &T) -> Result<(), Error> {
    say(&serde_json::to_string_pretty(value)?)
}

/// An error and every cause under it, in one line, as the operator needs it
/// to tell a refused connection from a bad certificate.
pub(super) fn chain(error: &dyn std::error::Error) -> String {
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        let words = cause.to_string();
        if !text.contains(&words) {
            text.push_str(": ");
            text.push_str(&words);
        }
        source = cause.source();
    }
    text
}
