//! Runs operator recovery for app deliveries, receipt interpretation and parked items without resending machine orders.
pub mod measurement;

use crate::operator::Error;
use crate::{deadline, installation::config::Program};
use analyte_model::{DeliveryId, Identifier, Reason, ReceiptId};
use analyte_store::{Audit, Handle, InterpretationCommit, Reprocess};

/// The audit action recorded when retained evidence is read again.
pub const REPROCESS_ACTION: &str = "receipt-reprocess";
/// The audit action recorded when a parked item is marked resolved.
pub const RESOLVE_ACTION: &str = "parked-resolve";

/// Selects retained evidence without accepting unvalidated textual IDs inward.
#[derive(Clone, Copy)]
pub enum ReprocessTarget {
    /// One original receipt, whether previously interpreted or not.
    Receipt(ReceiptId),
    /// One open parked item linked to exactly one receipt.
    Parked(i64),
}

/// Reinterprets retained evidence using the current validated program settings.
/// The store commits one audit together with its effects. Cancellation can leave
/// a committed version; inspect the audit before repeating an uncertain action.
/// # Errors
/// Refuses unknown, ambiguous or resolved evidence, absent machine config and
/// underlying parsing-provenance/storage failures. Mapping failures stay parked.
pub async fn reprocess(
    store: &Handle,
    program: &Program,
    target: ReprocessTarget,
    actor: Identifier,
    reason: Reason,
) -> Result<InterpretationCommit, Error> {
    let (receipt, parked) = match target {
        ReprocessTarget::Receipt(id) => (id, None),
        ReprocessTarget::Parked(id) => (
            store
                .request(deadline()?, move |s| s.parked_receipt(id))
                .await?,
            Some(id),
        ),
    };
    let metadata = store
        .request(deadline()?, move |s| {
            s.receipt_metadata(receipt)?.ok_or(Error::UnknownReceipt)
        })
        .await?;
    let machine = program
        .machines()
        .iter()
        .find(|m| m.id() == &metadata.machine)
        .ok_or_else(|| Error::ReceiptMachineMissing)?;
    crate::machine::reprocess(
        store,
        machine,
        Reprocess {
            receipt,
            parked,
            audit: Audit {
                actor,
                action: Identifier::new(REPROCESS_ACTION)?,
                targets: Vec::new(),
                time: jiff::Timestamp::now().as_second(),
                reason,
            },
        },
    )
    .await
    .map_err(Into::into)
}

/// Marks one parked item resolved with who did it and why, for the command line
/// and the admin pages alike. The item is never deleted (ADR 0003 rule 2): it
/// keeps its evidence and gains its resolution, and the same audit row is
/// written in the same transaction. Linked machine exchanges of unknown outcome
/// are settled with it.
/// # Errors
/// Reports invalid audit fields, an unknown or already resolved item, and store
/// failures. A timeout leaves the outcome uncertain; inspect the item before retrying.
pub async fn resolve(
    store: &Handle,
    id: i64,
    actor: Identifier,
    reason: Reason,
) -> Result<(), Error> {
    let audit = Audit {
        actor,
        action: Identifier::new(RESOLVE_ACTION)?,
        targets: vec![Identifier::new(&id.to_string())?],
        time: jiff::Timestamp::now().as_second(),
        reason,
    };
    store
        .request(deadline()?, move |s| s.resolve(id, &audit))
        .await?;
    Ok(())
}

/// Requeues exactly the named delivery, preserving its ID and stored bytes.
/// Explicitly naming a superseded ID acknowledges selecting that old revision.
/// # Errors
/// Reports invalid audit fields, unknown/ineligible deliveries and store failures.
/// A timeout leaves the outcome uncertain; inspect the audit before retrying.
pub async fn resend(
    store: &Handle,
    id: DeliveryId,
    acknowledge_superseded: bool,
    actor: Identifier,
    reason: Reason,
) -> Result<(), Error> {
    let audit = Audit {
        actor,
        action: Identifier::new("delivery-resend")?,
        targets: vec![Identifier::new(&format!(
            "delivery:{}",
            uuid::Uuid::from(id)
        ))?],
        time: jiff::Timestamp::now().as_second(),
        reason,
    };
    store
        .request(deadline()?, move |s| {
            s.resend_delivery(id, acknowledge_superseded, &audit)
        })
        .await?;
    Ok(())
}
