//! Runs the foreground service and protected operator command line.
#![forbid(unsafe_code)]
use analyte::{Error, installation::config::Program, runtime::service};
use analyte_model::{Identifier, Reason};
use analyte_store::Audit;
use clap::{Parser, Subcommand};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(Parser)]
#[command(version, about = "Faithful analyzer receive service")]
struct Cli {
    /// Program file for operator commands.
    #[arg(long, global = true, default_value = "analyte.toml")]
    config: PathBuf,
    /// Explicit local operator identity, recorded in the audit trail.
    #[arg(long, global = true, value_parser = Identifier::new)]
    actor: Option<Identifier>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Places or releases an indefinite retention hold.
    Holds {
        #[command(subcommand)]
        command: analyte::operator::holds::Command,
    },
    Admin {
        #[command(subcommand)]
        command: Admin,
    },
    Orders {
        #[command(subcommand)]
        command: analyte::orders::Command,
    },
    CheckConfig {
        file: PathBuf,
    },
    Run {
        file: Option<PathBuf>,
        /// Hard process deadline after shutdown begins.
        #[arg(long, default_value_t = 30, value_parser = clap::value_parser!(u64).range(30..=300))]
        shutdown_timeout_secs: u64,
    },
    #[cfg(windows)]
    Service {
        #[command(subcommand)]
        command: ServiceCommand,
    },
    Status {
        file: PathBuf,
    },
    Parked {
        #[command(subcommand)]
        command: Parked,
    },
    Receipts {
        #[command(subcommand)]
        command: Receipts,
    },
    Deliveries {
        #[command(subcommand)]
        command: Deliveries,
    },
    Restore {
        #[arg(long)]
        from: PathBuf,
        #[arg(long)]
        reason: String,
    },
    Backup {
        target: PathBuf,
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
    store.shutdown(until()).await?;
    let commit = result?;
    analyte::operator::output::line(&format!(
        "Interpretation {} recorded; decisions: {:?}. Inspect parked items for failures.",
        commit.version, commit.outcomes
    ))?;
    Ok(())
}
fn until() -> Instant {
    Instant::now()
        .checked_add(Duration::from_secs(5))
        .unwrap_or(Instant::now())
}

fn main() -> Result<(), Error> {
    let cli = Cli::parse();
    #[cfg(windows)]
    if let Command::Service {
        command: ServiceCommand::Run {
            shutdown_timeout_secs,
        },
    } = &cli.command
    {
        return analyte::runtime::windows_service::run(
            cli.config,
            Duration::from_secs(*shutdown_timeout_secs),
        );
    }
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init()?;
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(execute(cli))
}

async fn execute(cli: Cli) -> Result<(), Error> {
    let actor = cli
        .actor
        .clone()
        .unwrap_or(Identifier::new("local-operator")?);
    match cli.command {
        Command::Holds { command } => {
            analyte::operator::holds::run(
                &Program::load(&cli.config)?,
                required_actor(cli.actor)?,
                command,
            )
            .await
        }
        Command::Admin {
            command: Admin::Token { rotate: _ },
        } => {
            let program = Program::load(&cli.config)?;
            let directory = program.data_directory().to_owned();
            let token =
                tokio::task::spawn_blocking(move || analyte::admin::rotate_token(&directory))
                    .await??;
            analyte::operator::output::line(&token)?;
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
            analyte::orders::run(&Program::load(&cli.config)?, actor, command).await
        }
        Command::CheckConfig { file } => check_config(&file),
        Command::Run {
            file,
            shutdown_timeout_secs,
        } => {
            service::run_bounded(
                Program::load(&file.unwrap_or(cli.config))?,
                Duration::from_secs(shutdown_timeout_secs),
            )
            .await
        }
        Command::Status { file } => analyte::operator::status::print(&Program::load(&file)?).await,
        Command::Parked {
            command: Parked::Reprocess { id, reason },
        } => {
            recover(
                &cli.config,
                required_actor(cli.actor)?,
                analyte::operator::recovery::ReprocessTarget::Parked(id),
                reason,
            )
            .await
        }
        Command::Parked { command } => operator(&cli.config, actor, command).await,
        Command::Receipts {
            command: Receipts::Reprocess { id, reason },
        } => {
            recover(
                &cli.config,
                required_actor(cli.actor)?,
                analyte::operator::recovery::ReprocessTarget::Receipt(id),
                reason,
            )
            .await
        }
        Command::Deliveries { command } => deliveries(&cli.config, cli.actor, command).await,
        Command::Restore { from, reason } => restore(&cli.config, actor, from, &reason).await,
        Command::Backup { target } => backup(&cli.config, target).await,
    }
}

async fn operator(
    config: &std::path::Path,
    actor: Identifier,
    command: Parked,
) -> Result<(), Error> {
    let program = Program::load(config)?;
    let mut store = analyte::open_system_store(&program).await?;
    let result = async {
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
            Parked::Resolve { id, reason } => (
                "parked-resolve",
                vec![Identifier::new(&id.to_string())?],
                reason.clone(),
            ),
        };
        let audit = Audit {
            actor,
            action: Identifier::new(action)?,
            targets,
            time: jiff::Timestamp::now().as_second(),
            reason: Reason::new(&reason)?,
        };
        let rows = store
            .request(
                Instant::now()
                    .checked_add(Duration::from_secs(5))
                    .unwrap_or(Instant::now()),
                move |store| match command {
                    Parked::Reprocess { .. } => Err(analyte_store::Error::InvalidInput),
                    Parked::Resolve { id, .. } => {
                        store.resolve(id, &audit)?;
                        Ok(Vec::new())
                    }
                    Parked::List { after } => {
                        store.audit(&audit)?;
                        store.parked(after)
                    }
                    Parked::Show { id } => {
                        store.audit(&audit)?;
                        let after = id
                            .checked_sub(1)
                            .ok_or(analyte_store::Error::InvalidInput)?;
                        let rows = store.parked(after)?;
                        let row = rows
                            .into_iter()
                            .find(|r| r.id == id)
                            .ok_or(analyte_store::Error::Conflict)?;
                        Ok(vec![row])
                    }
                },
            )
            .await?;
        for row in rows {
            analyte::operator::output::line(&format!(
                "{} {} resolution={:?}",
                row.id, row.evidence, row.resolution
            ))?;
        }
        Ok::<_, Error>(())
    }
    .await;
    store
        .shutdown(
            Instant::now()
                .checked_add(Duration::from_secs(5))
                .unwrap_or(Instant::now()),
        )
        .await?;
    result
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
        analyte::operator::output::line(&format!("{} {digest} {level}", machine.id().as_str()))?;
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
                    .request(until(), move |s| {
                        let rows = s.deliveries(after)?;
                        s.audit(&audit)?;
                        Ok(rows)
                    })
                    .await?;
                for row in rows {
                    analyte::operator::output::line(&format!(
                        "{} {} {} {:?}",
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
                analyte::operator::output::line("Delivery queued with its original ID and bytes.")?;
                Ok(())
            }
        }
    }
    .await;
    store.shutdown(until()).await?;
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
    analyte::operator::output::line(&format!(
        "Restored; order sending paused pending reconciliation. Previous store: {}",
        kept.display()
    ))?;
    Ok(())
}

async fn backup(config: &std::path::Path, target: PathBuf) -> Result<(), Error> {
    let program = Program::load(config)?;
    let mut store = analyte::open_system_store(&program).await?;
    let result = store
        .request(
            Instant::now()
                .checked_add(Duration::from_secs(30))
                .unwrap_or(Instant::now()),
            move |store| {
                store.backup(
                    &target,
                    Instant::now()
                        .checked_add(Duration::from_secs(25))
                        .unwrap_or(Instant::now()),
                )
            },
        )
        .await;
    store
        .shutdown(
            Instant::now()
                .checked_add(Duration::from_secs(5))
                .unwrap_or(Instant::now()),
        )
        .await?;
    result?;
    analyte::operator::output::line("Backup verified.")?;
    Ok(())
}
