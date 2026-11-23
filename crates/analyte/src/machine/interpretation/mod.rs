//! Maps stored receipts without transport handles, orders or acknowledgments.
mod mapping_reading;
pub(crate) use mapping_reading::{DERIVED_STATUS_REASON, departure_conflict_at};
pub(crate) mod policy;
pub(crate) mod retained;
use crate::{deadline, installation::config::Machine, intake};
use analyte_model::Revision;
use analyte_store::{Handle, InterpretationCommit, Reprocess};
pub(crate) use mapping_reading::map_reading;
pub(crate) mod departure_evidence;
#[cfg(test)]
mod tests;

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
            s.receipt(id)?.ok_or(intake::Error::UnknownReceipt)
        })
        .await?;
    if receipt.metadata.machine != *machine.id() {
        return Err(intake::Error::WrongMachine);
    }
    // A record-only receipt is a journaled chunk, never a message (ADR 0034);
    // and a record-only machine reads nothing.
    if is_recorded(&receipt.metadata) {
        return Err(intake::Error::RecordedOnly);
    }
    if machine.snapshot.effective().records_only() {
        return Err(intake::Error::RecordOnly);
    }
    Ok(
        retained::process(store, machine, &receipt, None, Some(action))
            .await?
            .1,
    )
}

/// Whether a record-only connection kept this receipt: its driver namespace
/// says so (ADR 0034). Such a receipt is never interpreted, parked or delivered.
pub(crate) fn is_recorded(metadata: &analyte_model::ReceiptMetadata) -> bool {
    metadata.driver.namespace.as_str() == analyte_config::recording::RECORD_DRIVER
}

// Read source context from durable evidence; every stack then uses the same layer-owned reader.
pub(crate) async fn map_retained(
    store: &Handle,
    machine: &Machine,
    receipt: &analyte_store::Receipt,
) -> Result<analyte_mapping::Output, ReadError> {
    let hello = if machine
        .snapshot
        .effective()
        .stack()
        .choice()
        .connection_context()
    {
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
    Ok(crate::machine::protocols::read(
        &machine.snapshot,
        analyte_mapping::Input {
            device: None,
            receipt: receipt.metadata.id,
            received_at: receipt.metadata.received_at,
            revision: Revision(0),
        },
        &receipt.raw,
        hello.as_ref().map(|receipt| receipt.raw.as_slice()),
    )?)
}

/// Failures reading retained receipt context, separate from a stored parked interpretation.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ReadError {
    /// The host could not represent the storage request deadline.
    #[error(transparent)]
    Deadline(#[from] crate::DeadlineError),
    #[error(transparent)]
    Store(#[from] analyte_store::OperatorError),
    #[error(transparent)]
    Mapping(#[from] crate::reading::Error),
}

impl From<analyte_store::WorkerError> for ReadError {
    fn from(cause: analyte_store::WorkerError) -> Self {
        Self::Store(analyte_store::OperatorError::from(cause))
    }
}
