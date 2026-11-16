//! Runs audited placement and release of indefinite retention holds.
use crate::{Error, deadline, installation::config::Program};
use analyte_model::{Identifier, Reason};
use analyte_store::{Audit, HoldTarget};
use clap::Subcommand;

/// Explicit targets for the local retention-hold command.
#[derive(Subcommand)]
pub enum Command {
    /// Places or releases a hold on one receipt.
    Receipt {
        /// Host receipt UUID.
        id: String,
        /// Release an existing hold instead of placing one.
        #[arg(long)]
        release: bool,
        /// Required operator explanation.
        #[arg(long)]
        reason: String,
    },
    /// Places or releases a hold covering an app order and its linked receipts.
    Order {
        /// Destination namespace.
        destination: String,
        /// App order identity.
        id: String,
        /// Release an existing hold instead of placing one.
        #[arg(long)]
        release: bool,
        /// Required operator explanation.
        #[arg(long)]
        reason: String,
    },
}
/// Applies one explicit hold transition with an actor and reason.
/// # Errors
/// Refuses missing targets, repeated transitions, invalid evidence and storage failures.
pub async fn run(program: &Program, actor: Identifier, command: Command) -> Result<(), Error> {
    let (target, release, reason) = match command {
        Command::Receipt {
            id,
            release,
            reason,
        } => (
            HoldTarget::Receipt(serde_json::from_value(serde_json::Value::String(id))?),
            release,
            reason,
        ),
        Command::Order {
            destination,
            id,
            release,
            reason,
        } => (
            HoldTarget::Order {
                destination: Identifier::new(&destination)?,
                order: Identifier::new(&id)?,
            },
            release,
            reason,
        ),
    };
    let audit = Audit {
        actor,
        action: Identifier::new(if release {
            "retention-hold-release"
        } else {
            "retention-hold-place"
        })?,
        targets: Vec::new(),
        time: jiff::Timestamp::now().as_second(),
        reason: Reason::new(&reason)?,
    };
    let mut store = crate::open_system_store(program).await?;
    let result = store
        .request(deadline(), move |store| {
            store.set_retention_hold(&target, !release, &audit)
        })
        .await;
    store.shutdown(deadline()).await?;
    result?;
    super::output::line("Retention hold updated.")?;
    Ok(())
}
