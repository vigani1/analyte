//! The `analyte-contract-check` command: checks a lab app against contract v1 and prints a table.
#![forbid(unsafe_code)]
use analyte_contract_check::{RunError, Settings, parse_seed, run};
use clap::Parser;
use std::{io::Read as _, io::Write as _, path::PathBuf, process::ExitCode};

/// Command-line options; the app's address and credentials are never among them.
#[derive(Parser)]
#[command(
    version,
    about = "Runs the published contract v1 vectors against your app's real endpoints.",
    long_about = concat!(
        "Runs the published contract v1 vectors against your app's real endpoints.

",
        "The app's address and credentials come from ANALYTE_APP_URL, ANALYTE_API_KEY and ",
        "ANALYTE_SIGNING_SECRET (optionally ANALYTE_REQUEST_TIMEOUT_SECONDS), read from ",
        "--env-file or the environment. They are never command-line arguments and never printed.

",
        "Exit code: 0 when no case failed, 1 when a case failed, 2 when the run could not start."
    )
)]
struct Arguments {
    /// File of NAME=value lines; a name missing from it is read from the environment.
    #[arg(long)]
    env_file: Option<PathBuf>,
    /// The orders you loaded into your app (start from docs/contract/vectors/seed-orders.json).
    #[arg(long)]
    seed_file: Option<PathBuf>,
}

fn main() -> ExitCode {
    let arguments = Arguments::parse();
    match start(&arguments) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(message) => {
            let _ = writeln!(std::io::stderr(), "analyte-contract-check: {message}");
            ExitCode::from(2)
        }
    }
}

fn start(arguments: &Arguments) -> Result<bool, String> {
    let settings =
        Settings::load(arguments.env_file.as_deref()).map_err(|error| error.to_string())?;
    let seed = match &arguments.seed_file {
        None => None,
        Some(path) => {
            let mut bytes = Vec::new();
            std::fs::File::open(path)
                .and_then(|file| {
                    file.take(
                        u64::try_from(analyte_contract::MAX_DOCUMENT_BYTES)
                            .unwrap_or(u64::MAX)
                            .saturating_add(1),
                    )
                    .read_to_end(&mut bytes)
                })
                .map_err(|error| format!("seed file could not be read: {error}"))?;
            Some(parse_seed(&bytes).map_err(|error| error.to_string())?)
        }
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("runtime could not start: {error}"))?;
    let report = runtime
        .block_on(run(settings, seed.as_deref()))
        .map_err(|error: RunError| error.to_string())?;
    let mut output = std::io::stdout().lock();
    writeln!(
        output,
        "Analyte contract v1 check, {} cases",
        report.cases.len()
    )
    .and_then(|()| write!(output, "{}", report.render()))
    .and_then(|()| output.flush())
    .map_err(|error| format!("report could not be written: {error}"))?;
    Ok(report.passed())
}
