//! Parks unresolved results and writes reprocessing audits in the caller transaction.
use super::{Error, IdentityStatus, Interpretation, Reading, RevisionOutcome, params};

/// The reason parked when processing a message again found no results at all.
pub const NO_RESULTS_REASON: &str = "reprocessing produced no results";

/// A parked result's reason is `result reconciliation: ` and its outcome.
const RECONCILIATION_OPEN: &str = "result reconciliation: ";

/// The reason a parked result is stored with: its reconciliation outcome.
fn reconciliation_reason(outcome: &RevisionOutcome) -> String {
    format!("{RECONCILIATION_OPEN}{outcome}")
}

/// Reads back the outcome a parked result's reason names, the exact reverse
/// of how it was written; any other text gives `None`.
#[must_use]
pub fn reconciled_outcome(stored: &str) -> Option<RevisionOutcome> {
    RevisionOutcome::from_stored(stored.strip_prefix(RECONCILIATION_OPEN)?)
}
pub(super) fn reconciliation_succeeded(
    evidence: &Interpretation,
    outcomes: &[RevisionOutcome],
    findings_empty: bool,
) -> bool {
    findings_empty
        && match &evidence.outcome {
            Reading::Failed(_) => false,
            Reading::Results(results) => {
                !results.is_empty()
                    && results.iter().all(|r| !r.requires_parking())
                    && outcomes.iter().all(|o| {
                        matches!(
                            o,
                            RevisionOutcome::New(_)
                                | RevisionOutcome::Correction(_)
                                | RevisionOutcome::Retransmission(_)
                        )
                    })
            }
        }
}
pub(super) fn park_outcomes(
    tx: &rusqlite::Transaction<'_>,
    receipt_key: &str,
    version: i64,
    evidence: &Interpretation,
    outcomes: &[RevisionOutcome],
    context: (bool, bool, Option<&str>),
) -> Result<(), Error> {
    let (reprocessing, findings_empty, edit_guard) = context;
    match &evidence.outcome {
        Reading::Failed(reason) => {
            tx.execute(
                "INSERT INTO parked(evidence) VALUES (?1)",
                [crate::parked::evidence(
                    crate::ParkedKind::InterpretationFailure.as_str(),
                    &serde_json::from_str::<String>(receipt_key)?,
                    reason.as_str(),
                    Some(crate::parked::ParkingContext::Interpretation { version }),
                )?],
            )?;
        }
        Reading::Results(results) => {
            if reprocessing && results.is_empty() && findings_empty {
                tx.execute(
                    "INSERT INTO parked(evidence) VALUES (?1)",
                    [crate::parked::evidence(
                        crate::ParkedKind::InterpretationFailure.as_str(),
                        &serde_json::from_str::<String>(receipt_key)?,
                        NO_RESULTS_REASON,
                        Some(crate::parked::ParkingContext::Interpretation { version }),
                    )?],
                )?;
            }
            for (index, (result, outcome)) in results.iter().zip(outcomes).enumerate() {
                if result.requires_parking()
                    || matches!(
                        outcome,
                        RevisionOutcome::Conflict | RevisionOutcome::Unresolved
                    )
                {
                    tx.execute(
                        "INSERT INTO parked(evidence) VALUES (?1)",
                        [crate::parked::evidence(
                            crate::ParkedKind::ResultReconciliation.as_str(),
                            &serde_json::from_str::<String>(receipt_key)?,
                            &match (&result.identity, edit_guard) {
                                (IdentityStatus::Unresolved(reason), Some(guard))
                                    if reason.as_str() == guard =>
                                {
                                    guard.to_owned()
                                }
                                _ => reconciliation_reason(outcome),
                            },
                            Some(crate::parked::ParkingContext::Result {
                                version: Some(version),
                                index,
                            }),
                        )?],
                    )?;
                }
            }
        }
    }

    Ok(())
}
pub(super) fn write_reprocess_audit(
    tx: &rusqlite::Transaction<'_>,
    action: Option<&crate::Reprocess>,
    audit: Option<String>,
    successful: bool,
) -> Result<(), Error> {
    if let (Some(action), Some(audit)) = (action, audit) {
        if successful && let Some(parked) = action.parked {
            tx.execute(
                "UPDATE parked SET resolution=?2 WHERE id=?1 AND resolution IS NULL",
                params![parked, &audit],
            )?;
        }
        tx.execute("INSERT INTO audit(evidence) VALUES (?1)", [audit])?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use analyte_model::Revision;

    // A parked result's reason names its reconciliation outcome; it reads back
    // exactly, and any other spelling does not.
    #[test]
    fn reconciliation_reasons_read_back_only_their_exact_spelling() {
        for outcome in [
            RevisionOutcome::New(Revision(0)),
            RevisionOutcome::Retransmission(Revision(7)),
            RevisionOutcome::Correction(Revision(12)),
            RevisionOutcome::Conflict,
            RevisionOutcome::Unresolved,
        ] {
            let stored = reconciliation_reason(&outcome);
            assert_eq!(stored, format!("result reconciliation: {outcome}"));
            assert_eq!(reconciled_outcome(&stored), Some(outcome));
        }
        for text in [
            "",
            "Conflict",
            "result reconciliation: conflict",
            "result reconciliation: New(Revision(01))",
            "result reconciliation: New(Revision(-1))",
            "result reconciliation: New(Revision(1)) ",
            NO_RESULTS_REASON,
        ] {
            assert_eq!(reconciled_outcome(text), None, "{text}");
        }
    }
}
