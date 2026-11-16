//! Atomic operator reprocessing evidence (plan §11 and ADR 0003).
use crate::{Audit, Error, key};
use analyte_model::{Identifier, ReceiptId};
use rusqlite::{OptionalExtension, Transaction};

/// An explicit operator action on a receipt, optionally resolving one parked item.
pub struct Reprocess {
    /// Original receipt; reprocessing never changes its bytes or metadata.
    pub receipt: ReceiptId,
    /// Open parked item that must belong to this receipt.
    pub parked: Option<i64>,
    /// Actor, reason and time; action must be `receipt-reprocess`.
    pub audit: Audit,
}

pub(crate) fn check_parked(tx: &Transaction<'_>, id: i64, receipt: ReceiptId) -> Result<(), Error> {
    let evidence: Option<String> = tx
        .query_row(
            "SELECT evidence FROM parked WHERE id=?1 AND resolution IS NULL",
            [id],
            |r| r.get(0),
        )
        .optional()?;
    let details = crate::ParkedItem {
        id,
        evidence: evidence.ok_or(Error::ReprocessParked)?,
        resolution: None,
    }
    .details()?;
    let receipt_text: String = serde_json::from_str(&key(&receipt)?)?;
    let linked = details
        .targets
        .iter()
        .any(|target| target.as_str() == receipt_text);
    if !linked {
        return Err(Error::ReprocessParked);
    }
    Ok(())
}

pub(crate) fn audit_evidence(
    action: &Reprocess,
    version: i64,
    successful: bool,
) -> Result<String, Error> {
    let id: String = serde_json::from_str(&key(&action.receipt)?)?;
    let mut targets = vec![
        Identifier::new(&id).map_err(|_| Error::InvalidInput)?,
        Identifier::new(&format!("interpretation:{id}:{version}"))
            .map_err(|_| Error::InvalidInput)?,
        Identifier::new(if successful {
            "reprocess:succeeded"
        } else {
            "reprocess:parked"
        })
        .map_err(|_| Error::InvalidInput)?,
    ];
    if let Some(parked) = action.parked {
        targets
            .push(Identifier::new(&format!("parked:{parked}")).map_err(|_| Error::InvalidInput)?);
    }
    key(&Audit {
        actor: action.audit.actor.clone(),
        action: action.audit.action.clone(),
        targets,
        time: action.audit.time,
        reason: action.audit.reason.clone(),
    })
}

#[cfg(test)]
mod tests;
