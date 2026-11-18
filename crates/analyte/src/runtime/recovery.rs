//! Finishes or parks receipts and ASTM transfers left unfinished before any machine connection opens.
use crate::runtime::Error;
use crate::{deadline, installation::config::Program};
use analyte_store::{Handle, Interpretation, Reading};

/// Store boundaries combined while recovering unfinished intake at startup.
#[derive(Debug, thiserror::Error)]
pub enum RecoveryError {
    /// Protected receipt lookup failed.
    #[error(transparent)]
    Receipt(#[from] analyte_store::OperatorError),
    /// Reconciliation failed; a submitted write may have committed.
    #[error(transparent)]
    Reconciliation(#[from] analyte_store::ReconciliationError),
    /// Closing an open transfer failed; its frames stay journaled and open.
    #[error(transparent)]
    Transfer(#[from] analyte_store::IntakeError),
    /// Failure evidence could not fit model bounds.
    #[error(transparent)]
    Text(#[from] analyte_model::TextError),
    /// Pending receipt does not exist.
    #[error("receipt does not exist")]
    UnknownReceipt,
}
impl From<analyte_store::WorkerError> for RecoveryError {
    fn from(error: analyte_store::WorkerError) -> Self {
        Self::Receipt(error.into())
    }
}

/// Bounds every startup scan; more pages than this refuse startup visibly.
const PAGES: usize = 128;

/// Runs before any machine connection opens, so no transfer or receipt is still live.
pub(crate) async fn run(program: &Program, store: &Handle) -> Result<(), Error> {
    park_open_transfers(store).await?;
    for machine in program.machines() {
        if crate::machine::protocols::journals_chunks(machine.snapshot.effective().stack()) {
            continue;
        }
        let mut after = 0;
        let mut complete = false;
        for _ in 0..PAGES {
            let target = machine.id.clone();
            let pending = store
                .request(deadline()?, move |s| {
                    s.pending_interpretations(&target, after)
                })
                .await?;
            if pending.is_empty() {
                complete = true;
                break;
            }
            for (sequence, metadata) in pending {
                after = sequence;
                if metadata.config_digest.0 == machine.snapshot.digest() {
                    crate::intake::interpret_with_location(store, machine, metadata.id).await?;
                } else {
                    park_changed_config(store, metadata).await?;
                }
            }
        }
        if !complete {
            return Err(Error::RecoveryPageLimit);
        }
    }
    Ok(())
}

/// ASTM frames are acknowledged one by one (ADR 0003), so a transfer still open at
/// startup holds acknowledged data that no receipt names. Close it as incomplete:
/// the store keeps every frame and parks the transfer for an operator.
async fn park_open_transfers(store: &Handle) -> Result<(), Error> {
    for _ in 0..PAGES {
        let open = store
            .request::<_, RecoveryError>(deadline()?, |s| Ok(s.open_transfers()?))
            .await?;
        if open.is_empty() {
            return Ok(());
        }
        for transfer in open {
            let reason = analyte_model::Reason::new(
                "ASTM transfer was still open when Analyte stopped; its acknowledged frames are kept",
            )
            .map_err(RecoveryError::Text)?;
            store
                .request::<_, RecoveryError>(deadline()?, move |s| {
                    Ok(s.finish_transfer(
                        &transfer,
                        analyte_store::TransferOutcome::Incomplete(reason),
                    )?)
                })
                .await?;
        }
    }
    Err(Error::RecoveryPageLimit)
}

async fn park_changed_config(
    store: &Handle,
    metadata: analyte_model::ReceiptMetadata,
) -> Result<(), Error> {
    let id = metadata.id;
    store
        .request::<_, RecoveryError>(deadline()?, move |s| {
            let receipt = s.receipt(id)?.ok_or(RecoveryError::UnknownReceipt)?;
            let evidence = Interpretation {
                driver: metadata.driver,
                config_version: metadata.config_version,
                config: receipt.config,
                outcome: Reading::Failed(analyte_model::Reason::new(
                    "pending receipt has a different retained configuration; explicit audited reprocessing required",
                )?),
                departures: None,
            };
            Ok(s.reconcile(
                analyte_store::ReconcileAction::Interpret(id),
                evidence,
                analyte_store::ReconcileInput::standard(&[], &[]),
                |_, _| Ok(Vec::new()),
            )?)
        })
        .await?;
    Ok(())
}
