//! Foreground service and protected operator command line.
#![forbid(unsafe_code)]
use analyte::{Error, config::Program, service};
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
    #[arg(long, global = true, default_value = "local-operator", value_parser = Identifier::new)]
    actor: Identifier,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
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
    Resolve {
        id: i64,
        #[arg(long)]
        reason: String,
    },
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
        return analyte::windows_service::run(
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
    match cli.command {
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
            analyte::orders::run(&Program::load(&cli.config)?, cli.actor, command).await
        }
        Command::CheckConfig { file } => {
            let program = Program::load(&file)?;
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
                analyte::output::line(&format!("{} {digest} {level}", machine.id().as_str()))?;
            }
            Ok(())
        }
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
        Command::Status { file } => analyte::status::print(&Program::load(&file)?).await,
        Command::Parked { command } => operator(&cli.config, cli.actor, command).await,
        Command::Restore { from, reason } => {
            let program = Program::load(&cli.config)?;
            let audit = Audit {
                actor: cli.actor,
                action: Identifier::new("store-restore")?,
                targets: Vec::new(),
                time: jiff::Timestamp::now().as_second(),
                reason: Reason::new(&reason)?,
            };
            let kept = service::restore(program, from, audit).await?;
            analyte::output::line(&format!(
                "Restored; order sending paused pending reconciliation. Previous store: {}",
                kept.display()
            ))?;
            Ok(())
        }
        Command::Backup { target } => {
            let program = Program::load(&cli.config)?;
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
            analyte::output::line("Backup verified.")?;
            Ok(())
        }
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
            analyte::output::line(&format!(
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
