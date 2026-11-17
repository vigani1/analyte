//! Maps stored receipts without transport handles, orders or acknowledgments.
#[cfg(test)]
use crate::Error;
use crate::{deadline, installation::config::Machine, intake};
use analyte_config::Stack;
#[cfg(test)]
use analyte_model::ReceiptId;
use analyte_model::Revision;
use analyte_store::{Handle, InterpretationCommit, Reprocess};

/// Reinterprets only the retained original bytes using the supplied current machine.
/// Stores an append-only version and exactly one audit, atomically with revision
/// reconciliation and optional parked resolution. No machine connection is opened.
/// Cancellation after submission may leave a committed version; inspect its audit.
/// # Errors
/// Reports missing receipts, machine mismatch, invalid operator evidence and storage
/// failures. Syntax/mapping failures are retained as a new parked interpretation.
pub async fn reprocess(
    store: &Handle,
    machine: &Machine,
    action: Reprocess,
) -> Result<InterpretationCommit, intake::Error> {
    let id = action.receipt;
    let receipt = store
        .request(deadline()?, move |s| {
            s.receipt(id)?.ok_or(analyte_store::Error::UnknownReceipt)
        })
        .await?;
    if receipt.metadata.machine != *machine.id() {
        return Err(intake::Error::WrongMachine);
    }
    let mapping = map_retained(store, machine, &receipt).await;
    Ok(
        intake::persist_mapping(store, machine, id, mapping.into(), Some(action))
            .await?
            .1,
    )
}

// Read source context from durable evidence; every stack then uses the same layer-owned reader.
pub(crate) async fn map_retained(
    store: &Handle,
    machine: &Machine,
    receipt: &analyte_store::Receipt,
) -> Result<analyte_mapping::Output, ReadError> {
    let hello = if matches!(machine.snapshot.effective().stack(), Stack::Poct(_)) {
        let machine_id = receipt.metadata.machine.clone();
        let connection = receipt.metadata.connection.clone();
        store
            .request(deadline()?, move |s| {
                s.first_interpreted_connection_receipt(&machine_id, &connection)
            })
            .await?
    } else {
        None
    };
    Ok(analyte_mapping::read::read(
        &machine.snapshot,
        analyte_mapping::Input {
            receipt: receipt.metadata.id,
            received_at: receipt.metadata.received_at,
            revision: Revision(0),
        },
        &receipt.raw,
        hello.as_ref().map(|receipt| receipt.raw.as_slice()),
    )?)
}

pub(super) use analyte_mapping::read::poct_limits;

#[cfg(test)]
fn vidas_config(machine: &Machine) -> Result<analyte_framing::vidas::Config, ReadError> {
    Ok(analyte_mapping::read::vidas_config(&machine.snapshot)?)
}
pub(crate) mod departure_evidence;
#[cfg(test)]
mod departure_tests;
#[cfg(test)]
mod tests;

/// Failures reading retained receipt context, separate from a stored parked interpretation.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ReadError {
    #[error(transparent)]
    Store(#[from] analyte_store::Error),
    #[error(transparent)]
    Mapping(#[from] analyte_mapping::read::Error),
}
