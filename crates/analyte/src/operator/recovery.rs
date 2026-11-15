//! Operator recovery changes app delivery or interpretation, never machine orders.
use crate::{Error, deadline, installation::config::Program, invalid};
use analyte_model::{DeliveryId, Identifier, Reason, ReceiptId};
use analyte_store::{Audit, Handle, InterpretationCommit, Reprocess};

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
                .request(deadline(), move |s| s.parked_receipt(id))
                .await?,
            Some(id),
        ),
    };
    let metadata = store
        .request(deadline(), move |s| {
            s.receipt_metadata(receipt)?
                .ok_or(analyte_store::Error::UnknownReceipt)
        })
        .await?;
    let machine = program
        .machines()
        .iter()
        .find(|m| m.id() == &metadata.machine)
        .ok_or_else(|| invalid("receipt machine is absent from current configuration"))?;
    crate::machine::reprocess(
        store,
        machine,
        Reprocess {
            receipt,
            parked,
            audit: Audit {
                actor,
                action: Identifier::new("receipt-reprocess")?,
                targets: Vec::new(),
                time: jiff::Timestamp::now().as_second(),
                reason,
            },
        },
    )
    .await
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
        .request(deadline(), move |s| {
            s.resend_delivery(id, acknowledge_superseded, &audit)
        })
        .await?;
    Ok(())
}
