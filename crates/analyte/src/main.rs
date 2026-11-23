//! Runs the foreground service and protected operator command line.
#![forbid(unsafe_code)]
use analyte::runtime::Error;
use analyte::{
    installation::{config::Program, program_file},
    runtime::service,
};
use analyte_model::{Identifier, Reason};
use analyte_store::Audit;
use clap::{Parser, Subcommand};
use std::{
    io::Write as _,
    path::PathBuf,
    process::ExitCode,
    time::{Duration, Instant},
};

use analyte::runtime::{DeliveryCommandError, ParkedCommandError};

/// Allows backup execution plus queue and response overhead.
const BACKUP_REQUEST_BUDGET: Duration = Duration::from_secs(30);
/// Leaves five seconds of the request budget for host overhead.
const BACKUP_WORK_BUDGET: Duration = Duration::from_secs(25);

#[derive(Parser)]
#[command(version, about = "Faithful analyzer receive service")]
struct Cli {
    /// Program file for operator commands. Without it: the file named by
    /// ANALYTE_CONFIG; else analyte.toml in the current folder, if there is
    /// one; else the installed program file (Windows:
    /// %ProgramData%\Analyte\config\analyte.toml; Linux:
    /// /etc/analyte/analyte.toml), if there is one. A file found this way is
    /// named on standard error.
    #[expect(
        clippy::doc_markdown,
        reason = "clap prints this comment as --help, where backticks would show"
    )]
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    /// Explicit local operator identity, recorded in the audit trail; it may
    /// not begin like an admin page sign-in's (`admin-session-`, `windows:`).
    #[arg(long, global = true, value_parser = analyte::admin::command_line_actor)]
    actor: Option<Identifier>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Retries exhausted alert events with their original identities and bytes.
    RetryAlerts {
        #[arg(value_parser = Identifier::new)]
        endpoint: Identifier,
        #[arg(long)]
        reason: String,
    },
    /// Recorded conversations (ADR 0009, ADR 0034): list them, or copy one to
    /// a folder. They contain patient data; both are audited.
    Captures {
        #[command(subcommand)]
        command: analyte::operator::captures::Command,
    },
    /// Places or releases an indefinite retention hold.
    Holds {
        #[command(subcommand)]
        command: analyte::operator::holds::Command,
    },
    /// The admin page's key.
    Admin {
        #[command(subcommand)]
        command: Admin,
    },
    /// Lists, adds, sets up, tests and removes machines through the Setup
    /// page's own checks; changes apply after a restart.
    Machines {
        // Boxed: its setup choices are much larger than every other command.
        #[command(subcommand)]
        command: Box<analyte::admin::Machines>,
    },
    /// Lists, adds, changes and removes lab apps and stores their keys
    /// through the Lab apps page's own checks; changes apply after a restart.
    LabApps {
        #[command(subcommand)]
        command: analyte::admin::LabApps,
    },
    /// Machine orders: list, cancelled by a machine, reconcile, resume after a restore.
    Orders {
        #[command(subcommand)]
        command: analyte::orders::Command,
    },
    /// Checks a program file and lists each machine's settings version and evidence level.
    CheckConfig { file: PathBuf },
    /// Runs the engine in this window until Ctrl+C (the service uses `service run`).
    Run {
        file: Option<PathBuf>,
        /// Hard process deadline after shutdown begins.
        #[arg(long, default_value_t = 30, value_parser = clap::value_parser!(u64).range(30..=300))]
        shutdown_timeout_secs: u64,
    },
    /// Entry point for the Windows service; start it with Start-Service Analyte.
    #[cfg(windows)]
    Service {
        #[command(subcommand)]
        command: ServiceCommand,
    },
    /// Technical health lines for support: alerts, queues, last data per machine.
    Status { file: PathBuf },
    /// Needs review: items Analyte kept but could not process. `review` and
    /// `parked` are the same command.
    #[command(visible_alias = "review")]
    Parked {
        #[command(subcommand)]
        command: Parked,
    },
    /// Reprocesses one stored message with the current settings.
    Receipts {
        #[command(subcommand)]
        command: Receipts,
    },
    /// Deliveries to lab apps: list, send again.
    Deliveries {
        #[command(subcommand)]
        command: Deliveries,
    },
    /// Restores the store from a verified backup; order sending stays paused until reconciled.
    Restore {
        #[arg(long)]
        from: PathBuf,
        #[arg(long)]
        reason: String,
    },
    /// Writes a verified backup of the store to a file.
    Backup { target: PathBuf },
    #[command(flatten)]
    Pages(analyte::admin::command::Command),
    /// Prints a script that completes analyte's commands in `shell`.
    /// PowerShell: analyte completions powershell | Out-String | Invoke-Expression
    Completions {
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

#[derive(Subcommand)]
enum Admin {
    /// Replaces the protected admin credential and prints it once.
    Token {
        #[arg(long, required = true)]
        rotate: bool,
    },
}

#[cfg(windows)]
#[derive(Subcommand)]
enum ServiceCommand {
    Run {
        #[arg(long, default_value_t = 30, value_parser = clap::value_parser!(u64).range(30..=300))]
        shutdown_timeout_secs: u64,
    },
}

#[derive(Subcommand)]
enum Parked {
    /// Show the protected result comparison and its confirmation snapshot.
    Compare {
        id: i64,
    },
    /// List missing-sample suggestions for one page of open items.
    Suggestions {
        #[arg(long, default_value_t = 0)]
        after: i64,
    },
    /// Confirm a displayed measurement comparison.
    Decide {
        id: i64,
        #[arg(value_enum)]
        decision: MeasurementChoice,
        #[arg(long)]
        snapshot: String,
        #[arg(long)]
        reason: String,
    },
    List {
        /// Continue after the last ID of a previous bounded page.
        #[arg(long, default_value_t = 0)]
        after: i64,
    },
    Show {
        id: i64,
    },
    Reprocess {
        id: i64,
        #[arg(long)]
        reason: String,
    },
    Resolve {
        id: i64,
        #[arg(long)]
        reason: String,
    },
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum MeasurementChoice {
    Resend,
    Rerun,
    Supersede,
}

#[derive(Subcommand)]
enum Receipts {
    Reprocess {
        #[arg(value_parser = receipt_id)]
        id: analyte_model::ReceiptId,
        #[arg(long)]
        reason: String,
    },
}
#[derive(Subcommand)]
enum Deliveries {
    List {
        #[arg(long, default_value_t = 0)]
        after: i64,
        #[arg(long)]
        reason: String,
    },
    Resend {
        #[arg(value_parser = delivery_id)]
        id: analyte_model::DeliveryId,
        #[arg(long)]
        reason: String,
    },
}
fn receipt_id(value: &str) -> Result<analyte_model::ReceiptId, String> {
    uuid::Uuid::parse_str(value)
        .map_err(|e| e.to_string())?
        .try_into()
        .map_err(|e: analyte_model::InvalidUuid| e.to_string())
}
fn delivery_id(value: &str) -> Result<analyte_model::DeliveryId, String> {
    uuid::Uuid::parse_str(value)
        .map_err(|e| e.to_string())?
        .try_into()
        .map_err(|e: analyte_model::InvalidUuid| e.to_string())
}
fn required_actor(actor: Option<Identifier>) -> Result<Identifier, Error> {
    actor.ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "--actor is required for recovery commands",
        )
        .into()
    })
}
/// Test builds only: `ANALYTE_TEST_ASTM_PAUSE_MARKER` makes the real program pause after
/// storing an ASTM frame and before its ACK, so a test can hard-kill it there.
#[cfg(feature = "test-hooks")]
fn astm_pause_for_test(
    mut program: Program,
) -> Result<(Program, Option<analyte::runtime::test_hooks::Control>), Error> {
    let pause = std::env::var_os("ANALYTE_TEST_ASTM_PAUSE_MARKER")
        .map(|marker| program.pause_astm_committed_with_marker(marker.into()))
        .transpose()?;
    Ok((program, pause))
}

async fn recover(
    config: &std::path::Path,
    actor: Identifier,
    target: analyte::operator::recovery::ReprocessTarget,
    reason: String,
) -> Result<(), Error> {
    let reason = Reason::new(&reason)?;
    let program = Program::load(config)?;
    let mut store = analyte::open_system_store(&program).await?;
    let result =
        analyte::operator::recovery::reprocess(&store, &program, target, actor, reason).await;
    store.shutdown(analyte::deadline()?).await?;
    let commit = result?;
    analyte::operator::display::line(&format!(
        "Interpretation {} recorded; decisions: {}. Inspect parked items for failures.",
        commit.version,
        analyte::operator::display::list(&commit.outcomes)
    ))?;
    Ok(())
}

fn main() -> Result<ExitCode, Error> {
    let cli = Cli::parse();
    if let Command::Completions { shell } = cli.command {
        let mut command = <Cli as clap::CommandFactory>::command();
        clap_complete::generate(shell, &mut command, "analyte", &mut std::io::stdout());
        return Ok(ExitCode::SUCCESS);
    }
    #[cfg(windows)]
    if let Command::Service {
        command: ServiceCommand::Run {
            shutdown_timeout_secs,
        },
    } = &cli.command
    {
        return analyte::runtime::windows_service::run(
            program_file::locate(cli.config)?.path,
            Duration::from_secs(*shutdown_timeout_secs),
        )
        .map(|()| ExitCode::SUCCESS);
    }
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init()
        .map_err(Error::Tracing)?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let Cli {
        config,
        actor,
        command,
    } = cli;
    // `overview` and `doctor` say how things are in their exit code.
    if let Command::Pages(pages) = command {
        let actor = actor.map_or_else(|| Identifier::new("local-operator"), Ok)?;
        let config = program_file(config)?;
        return Ok(runtime.block_on(analyte::admin::command::run(&config, actor, pages))?);
    }
    // The setup commands print their own "Fix this" line and end with their
    // own exit code (0 done, 1 refused, 3 partly done, 4 test saw no data).
    if let Command::Machines { command } = command {
        let config = program_file(config)?;
        let outcome = runtime.block_on(analyte::admin::run_machines(&config, actor, *command));
        return Ok(outcome.into());
    }
    if let Command::LabApps { command } = command {
        let config = program_file(config)?;
        let outcome = runtime.block_on(analyte::admin::run_lab_apps(&config, actor, command));
        return Ok(outcome.into());
    }
    runtime.block_on(execute(Cli {
        config,
        actor,
        command,
    }))
}

/// The program file this command reads: `--config`, or the one
/// [`program_file::locate`] finds, named on standard error so standard output
/// stays as the command writes it.
fn program_file(explicit: Option<PathBuf>) -> Result<PathBuf, Error> {
    let found = program_file::locate(explicit)?;
    if found.source != program_file::Source::Explicit {
        writeln!(
            std::io::stderr().lock(),
            "Using {} ({}).",
            found.path.display(),
            found.source.describe()
        )?;
    }
    Ok(found.path)
}

async fn execute(cli: Cli) -> Result<ExitCode, Error> {
    let actor = cli
        .actor
        .clone()
        .unwrap_or(Identifier::new("local-operator")?);
    let explicit = cli.config;
    let config = || program_file(explicit.clone());
    let done = match cli.command {
        Command::RetryAlerts { endpoint, reason } => analyte::operator::alert_recovery::retry(
            &Program::load(&config()?)?,
            required_actor(cli.actor)?,
            endpoint,
            Reason::new(&reason)?,
        )
        .await
        .map_err(Into::into),
        Command::Captures { command } => captures(&config()?, cli.actor, command).await,
        Command::Holds { command } => analyte::operator::holds::run(
            &Program::load(&config()?)?,
            required_actor(cli.actor)?,
            command,
        )
        .await
        .map_err(Into::into),
        Command::Admin {
            command: Admin::Token { rotate: _ },
        } => {
            let program = Program::load(&config()?)?;
            let directory = program.data_directory().to_owned();
            let token =
                tokio::task::spawn_blocking(move || analyte::admin::rotate_token(&directory))
                    .await??;
            analyte::operator::display::line(&token)?;
            Ok(())
        }
        #[cfg(windows)]
        Command::Service {
            command: ServiceCommand::Run {
                shutdown_timeout_secs,
            },
        } => Err(std::io::Error::other(format!(
            "SCM dispatch was not selected ({shutdown_timeout_secs})"
        ))
        .into()),
        Command::Orders { command } => {
            analyte::orders::run(&Program::load(&config()?)?, actor, command)
                .await
                .map_err(Into::into)
        }
        Command::CheckConfig { file } => check_config(&file),
        Command::Run {
            file,
            shutdown_timeout_secs,
        } => {
            let program = Program::load(&file.map_or_else(config, Ok)?)?;
            #[cfg(feature = "test-hooks")]
            let (program, _pause) = astm_pause_for_test(program)?;
            service::run_bounded(program, Duration::from_secs(shutdown_timeout_secs)).await
        }
        Command::Status { file } => analyte::operator::status::print(&Program::load(&file)?)
            .await
            .map_err(Into::into),
        Command::Parked {
            command: Parked::Reprocess { id, reason },
        } => {
            recover(
                &config()?,
                required_actor(cli.actor)?,
                analyte::operator::recovery::ReprocessTarget::Parked(id),
                reason,
            )
            .await
        }
        Command::Parked { command } => operator(&config()?, actor, command).await,
        Command::Receipts {
            command: Receipts::Reprocess { id, reason },
        } => {
            recover(
                &config()?,
                required_actor(cli.actor)?,
                analyte::operator::recovery::ReprocessTarget::Receipt(id),
                reason,
            )
            .await
        }
        Command::Deliveries { command } => deliveries(&config()?, cli.actor, command).await,
        Command::Restore { from, reason } => restore(&config()?, actor, from, &reason).await,
        Command::Backup { target } => backup(&config()?, target).await,
        Command::Pages(_)
        | Command::Completions { .. }
        | Command::Machines { .. }
        | Command::LabApps { .. } => Err(std::io::Error::other("dispatched in main").into()),
    };
    done.map(|()| ExitCode::SUCCESS)
}

async fn operator(
    config: &std::path::Path,
    actor: Identifier,
    command: Parked,
) -> Result<(), Error> {
    let program = Program::load(config)?;
    let mut store = analyte::open_system_store(&program).await?;
    let result = async {
        if let Parked::Resolve { id, reason } = &command {
            return Ok(analyte::operator::recovery::resolve(
                &store,
                *id,
                actor,
                Reason::new(reason)?,
            )
            .await?);
        }
        if let Some(result) = measurement_command(&store, &program, &actor, &command).await {
            return result;
        }
        let (action, targets, reason) = match &command {
            Parked::List { .. } => (
                "parked-list",
                Vec::new(),
                "operator inspected parked list".to_owned(),
            ),
            Parked::Show { id } => (
                "parked-show",
                vec![Identifier::new(&id.to_string())?],
                "operator inspected parked item".to_owned(),
            ),
            Parked::Reprocess { .. } => {
                return Err(
                    std::io::Error::other("reprocess must use the recovery entry point").into(),
                );
            }
            Parked::Compare { .. }
            | Parked::Suggestions { .. }
            | Parked::Decide { .. }
            | Parked::Resolve { .. } => {
                return Err(
                    std::io::Error::other("resolve must use the recovery entry point").into(),
                );
            }
        };
        let audit = Audit {
            actor,
            action: Identifier::new(action)?,
            targets,
            time: jiff::Timestamp::now().as_second(),
            reason: Reason::new(&reason)?,
        };
        let rows = store
            .request::<_, ParkedCommandError>(analyte::deadline()?, move |store| match command {
                Parked::Reprocess { .. } => {
                    Err(ParkedCommandError::ReprocessRequiresInterpretation)
                }
                Parked::Compare { .. }
                | Parked::Suggestions { .. }
                | Parked::Decide { .. }
                | Parked::Resolve { .. } => Err(ParkedCommandError::ResolveUsesRecovery),
                Parked::List { after } => {
                    store.audit(&audit)?;
                    Ok(store.parked(after)?)
                }
                Parked::Show { id } => {
                    store.audit(&audit)?;
                    let after = id
                        .checked_sub(1)
                        .ok_or(ParkedCommandError::OperatorCursorOverflow)?;
                    let rows = store.parked(after)?;
                    let row = rows.into_iter().find(|r| r.id == id).ok_or(
                        ParkedCommandError::Evidence(analyte_store::OperatorError::Conflict),
                    )?;
                    Ok(vec![row])
                }
            })
            .await?;
        for row in rows {
            analyte::operator::display::line(&format!(
                "{} {} resolution={}",
                row.id,
                row.evidence,
                analyte::operator::display::optional_text(row.resolution.as_deref())
            ))?;
        }
        Ok::<_, Error>(())
    }
    .await;
    store.shutdown(analyte::deadline()?).await?;
    result
}

async fn measurement_command(
    store: &analyte_store::Handle,
    program: &Program,
    actor: &Identifier,
    command: &Parked,
) -> Option<Result<(), Error>> {
    use analyte::operator::recovery::measurement;
    use analyte_store::MeasurementDecision;
    match command {
        Parked::Compare { id } => Some(
            async { print_measurement(*id, measurement::review(store, *id, actor.clone()).await?) }
                .await,
        ),
        Parked::Suggestions { after } => Some(measurement_suggestions(store, *after, actor).await),
        Parked::Decide {
            id,
            decision,
            snapshot,
            reason,
        } => {
            let decision = match decision {
                MeasurementChoice::Resend => MeasurementDecision::Resend,
                MeasurementChoice::Rerun => MeasurementDecision::Rerun,
                MeasurementChoice::Supersede => MeasurementDecision::Supersede,
            };
            Some(
                measurement::decide(
                    store,
                    Some(program),
                    *id,
                    snapshot.clone(),
                    decision,
                    actor.clone(),
                    reason,
                )
                .await
                .map_err(Error::from),
            )
        }
        Parked::List { .. }
        | Parked::Show { .. }
        | Parked::Reprocess { .. }
        | Parked::Resolve { .. } => None,
    }
}

fn print_measurement(
    id: i64,
    review: Option<analyte_store::MeasurementReview>,
) -> Result<(), Error> {
    #[derive(serde::Serialize)]
    struct Output {
        item: i64,
        comparison: analyte_store::MeasurementReview,
    }
    let text = match review {
        Some(comparison) => serde_json::to_string_pretty(&Output {
            item: id,
            comparison,
        })
        .map_err(analyte::operator::Error::from)?,
        None => format!("Item {id} has no eligible measurement comparison."),
    };
    Ok(analyte::operator::display::line(&text)?)
}

async fn measurement_suggestions(
    store: &analyte_store::Handle,
    after: i64,
    actor: &Identifier,
) -> Result<(), Error> {
    let rows = store
        .request::<_, ParkedCommandError>(analyte::deadline()?, move |s| {
            Ok(s.parked_filtered(after, analyte_store::ParkedFilter::Open)?)
        })
        .await?;
    for item in &rows {
        if !item
            .details()
            .map_err(analyte::operator::Error::from)?
            .reason
            .as_str()
            .ends_with("}: MissingSample")
        {
            continue;
        }
        if let Some(review) =
            analyte::operator::recovery::measurement::review(store, item.id, actor.clone()).await?
        {
            print_measurement(item.id, Some(review))?;
        }
    }
    if let Some(last) = rows.last() {
        analyte::operator::display::line(&format!("Continue with --after {}", last.id))?;
    }
    Ok(())
}

/// Lists or exports recordings, audited under the required `--actor` (ADR 0034).
async fn captures(
    config: &std::path::Path,
    actor: Option<Identifier>,
    command: analyte::operator::captures::Command,
) -> Result<(), Error> {
    let program = Program::load(config)?;
    Ok(analyte::operator::captures::run(&program, required_actor(actor)?, command).await?)
}

fn check_config(file: &std::path::Path) -> Result<(), Error> {
    let program = Program::load(file)?;
    for machine in program.machines() {
        let digest = machine
            .snapshot()
            .digest()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let level = match machine.snapshot().effective().machine().evidence.level {
            analyte_config::EvidenceLevel::Documented => "documented",
            analyte_config::EvidenceLevel::Simulated => "simulated",
            analyte_config::EvidenceLevel::Proven => "proven",
        };
        analyte::operator::display::line(&format!("{} {digest} {level}", machine.id().as_str()))?;
    }
    for id in program.awaiting_setup() {
        analyte::operator::display::line(&format!(
            "{} not set up: choose its definition on the setup page",
            id.as_str()
        ))?;
    }
    if program.machines().is_empty() && program.awaiting_setup().is_empty() {
        analyte::operator::display::line("no machines: add one on the setup page")?;
    }
    Ok(())
}

async fn deliveries(
    config: &std::path::Path,
    actor: Option<Identifier>,
    command: Deliveries,
) -> Result<(), Error> {
    let actor = required_actor(actor)?;
    let program = Program::load(config)?;
    let mut store = analyte::open_system_store(&program).await?;
    let result = async {
        match command {
            Deliveries::List { after, reason } => {
                let audit = Audit {
                    actor,
                    action: Identifier::new("deliveries-list")?,
                    targets: Vec::new(),
                    time: jiff::Timestamp::now().as_second(),
                    reason: Reason::new(&reason)?,
                };
                let rows = store
                    .request::<_, DeliveryCommandError>(analyte::deadline()?, move |s| {
                        let rows = s.deliveries(after)?;
                        s.audit(&audit)?;
                        Ok(rows)
                    })
                    .await?;
                for row in rows {
                    analyte::operator::display::line(&format!(
                        "{} {} {} {}",
                        row.sequence,
                        uuid::Uuid::from(row.id),
                        row.destination.as_str(),
                        row.state
                    ))?;
                }
                Ok::<_, Error>(())
            }
            Deliveries::Resend { id, reason } => {
                analyte::operator::recovery::resend(&store, id, true, actor, Reason::new(&reason)?)
                    .await?;
                analyte::operator::display::line(
                    "Delivery queued with its original ID and bytes.",
                )?;
                Ok(())
            }
        }
    }
    .await;
    store.shutdown(analyte::deadline()?).await?;
    result
}

async fn restore(
    config: &std::path::Path,
    actor: Identifier,
    from: PathBuf,
    reason: &str,
) -> Result<(), Error> {
    let program = Program::load(config)?;
    let audit = Audit {
        actor,
        action: Identifier::new("store-restore")?,
        targets: Vec::new(),
        time: jiff::Timestamp::now().as_second(),
        reason: Reason::new(reason)?,
    };
    let kept = service::restore(program, from, audit).await?;
    analyte::operator::display::line(&format!(
        "Restored; order sending paused pending reconciliation. Previous store: {}",
        kept.display()
    ))?;
    Ok(())
}

async fn backup(config: &std::path::Path, target: PathBuf) -> Result<(), Error> {
    let program = Program::load(config)?;
    let mut store = analyte::open_system_store(&program).await?;
    let result = store
        .request::<_, analyte_store::MaintenanceError>(
            Instant::now()
                .checked_add(BACKUP_REQUEST_BUDGET)
                .unwrap_or(Instant::now()),
            move |store| {
                store.backup(
                    &target,
                    Instant::now()
                        .checked_add(BACKUP_WORK_BUDGET)
                        .unwrap_or(Instant::now()),
                )
            },
        )
        .await;
    store.shutdown(analyte::deadline()?).await?;
    result?;
    analyte::operator::display::line("Backup verified.")?;
    Ok(())
}
