//! Records operator order decisions and app events without issuing machine commands.
use crate::operator::Error;
use crate::{deadline, installation::config::Program};
use analyte_i18n::text as tr;
use analyte_model::{Identifier, OrderState, Reason};
use analyte_store::{Audit, MachineOrderFilter};
use clap::{Subcommand, ValueEnum};

/// Store boundaries combined by the operator's order command.
#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    /// Protected audit recording failed.
    #[error(transparent)]
    Audit(#[from] analyte_store::StorageError),
    /// Order lookup or lifecycle storage failed.
    #[error(transparent)]
    Order(#[from] analyte_store::OrderError),
    /// Restore reconciliation failed.
    #[error(transparent)]
    Maintenance(#[from] analyte_store::MaintenanceError),
    /// Worker admission or completion failed.
    #[error(transparent)]
    Worker(#[from] analyte_store::WorkerError),
}

/// Protected operator operations; every invocation records its actor and reason.
#[derive(Subcommand)]
pub enum Command {
    /// Releases the restore pause after machines and app have been reconciled.
    #[command(about = tr("cli-help-operator-command-resume-after-restore"))]
    ResumeAfterRestore {
        /// Evidence for the operator decision.
        #[arg(help = tr("cli-help-operator-command-resume-after-restore-reason"))]
        #[arg(long)]
        reason: String,
    },
    /// Lists one bounded page of machine-order lifecycle snapshots.
    #[command(about = tr("cli-help-operator-command-list"))]
    List {
        /// Continue after this local machine-order ID.
        #[arg(help = tr("cli-help-operator-command-list-after"))]
        #[arg(long, default_value_t = 0)]
        after: i64,
        /// Restrict the page to one installed machine.
        #[arg(help = tr("cli-help-operator-command-list-machine"))]
        #[arg(long,value_parser=Identifier::new)]
        machine: Option<Identifier>,
    },
    /// The Cancelled by machine page: order items a machine cancelled on its
    /// own, to check with the lab and the lab app. Shows each item's sample ID
    /// from the lab app's copy, as the page does, never result values; each
    /// run is recorded in the audit.
    #[command(about = tr("cli-help-operator-command-cancelled"))]
    Cancelled {
        /// Continue after this position, as the previous page printed it.
        #[arg(help = tr("cli-help-operator-command-cancelled-after"))]
        #[arg(long, default_value_t = 0)]
        after: u64,
        /// Print the items as JSON for scripts.
        #[arg(help = tr("cli-help-operator-command-cancelled-json"))]
        #[arg(long)]
        json: bool,
    },
    /// Resolves an uncertain machine outcome using operator evidence.
    #[command(about = tr("cli-help-operator-command-reconcile"))]
    Reconcile {
        /// Local machine-order ID printed by orders list.
        #[arg(help = tr("cli-help-operator-command-reconcile-id"))]
        id: i64,
        /// Outcome verified by the operator.
        #[arg(help = tr("cli-help-operator-command-reconcile-outcome"))]
        #[arg(long = "as", value_enum)]
        outcome: Outcome,
        /// Required explanation retained in the audit and app event.
        #[arg(help = tr("cli-help-operator-command-reconcile-reason"))]
        #[arg(long)]
        reason: String,
    },
}

/// Explicit reconciliation outcomes; none issues or retries a machine command.
#[derive(Clone, Copy, ValueEnum)]
pub enum Outcome {
    /// Machine acceptance has been verified.
    Accepted,
    /// Machine rejection has been verified.
    Rejected,
    /// Cancellation has been verified, rather than merely requested.
    Cancelled,
}

impl Outcome {
    fn state(self) -> OrderState {
        match self {
            Self::Accepted => OrderState::Accepted,
            Self::Rejected => OrderState::Rejected,
            Self::Cancelled => OrderState::CancelConfirmed,
        }
    }
}

/// Runs a bounded operator operation and joins its store worker on every exit.
/// # Errors
/// Reports invalid reasons, unknown orders, illegal reconciliation and I/O failures.
pub async fn run(program: &Program, actor: Identifier, command: Command) -> Result<(), Error> {
    let (action, targets, reason) = match &command {
        // A page read with its own audit; it opens the store itself.
        Command::Cancelled { after, json } => {
            return crate::admin::command::cancelled(program, actor, *after, *json)
                .await
                .map_err(|error| Error::Page(Box::new(error)));
        }
        Command::ResumeAfterRestore { reason } => (
            "orders-resume-after-restore",
            Vec::new(),
            Reason::new(reason)?,
        ),
        Command::List { .. } => (
            "orders-list",
            Vec::new(),
            Reason::new("operator inspected machine orders")?,
        ),
        Command::Reconcile { id, reason, .. } => (
            "orders-reconcile",
            vec![Identifier::new(&id.to_string())?],
            Reason::new(reason)?,
        ),
    };
    let audit = Audit {
        actor,
        action: Identifier::new(action)?,
        targets,
        time: jiff::Timestamp::now().as_second(),
        reason,
    };
    let mut store = crate::open_system_store(program).await?;
    let paused = store
        .request(deadline()?, |store| store.restore_unreconciled())
        .await?;
    if paused {
        crate::operator::display::line(tr(
            "cli-order-sending-paused-restored-store-requires-machine-app-reconciliation",
        ))?;
    }
    let result = store
        .request::<_, CommandError>(deadline()?, move |store| match command {
            // Answered above, before the store opened; nothing to do here.
            Command::Cancelled { .. } => Ok(Vec::new()),
            Command::ResumeAfterRestore { .. } => {
                store.resume_after_restore(&audit)?;
                Ok(Vec::new())
            }
            Command::List { after, machine } => {
                store.audit(&audit)?;
                Ok(store.machine_orders(&MachineOrderFilter {
                    after,
                    machine,
                    state: None,
                })?)
            }
            Command::Reconcile { id, outcome, .. } => {
                let reason = audit.reason.as_str().to_owned();
                let order = store.reconcile_order(id, outcome.state(), &audit, move |record| {
                    super::build_event(record, Some(&reason)).map_err(super::EventError::into_store)
                })?;
                Ok(vec![order])
            }
        })
        .await;
    store.shutdown(deadline()?).await?;
    for order in result.map_err(Error::OrderCommand)? {
        crate::operator::display::line(&format!(
            "{} app={} order={} machine={} version={} state={} updated_at={}",
            order.id,
            order.key.destination.as_str(),
            order.key.order.as_str(),
            order.key.machine.as_str(),
            order.version,
            order.state,
            order.updated_at
        ))?;
        for item in order.items {
            crate::operator::display::line(&format!(
                "  item={} placer={} namespace={} coverage_known={} tests={} state={} reason={}",
                item.index,
                item.placer,
                crate::operator::display::optional_text(
                    item.placer_namespace.as_ref().map(Identifier::as_str)
                ),
                item.coverage_known,
                serde_json::to_string(&item.tests)?,
                item.state,
                item.source_code.as_str()
            ))?;
        }
    }
    Ok(())
}
