//! Parks unresolved results and writes reprocessing audits in the caller transaction.
use super::*;
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
    context: (bool, bool),
) -> Result<(), Error> {
    let (reprocessing, findings_empty) = context;
    match &evidence.outcome {
        Reading::Failed(reason) => {
            tx.execute(
                "INSERT INTO parked(evidence) VALUES (?1)",
                [crate::parked::evidence(
                    "interpretation_failure",
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
                        "interpretation_failure",
                        &serde_json::from_str::<String>(receipt_key)?,
                        "reprocessing produced no results",
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
                            "result_reconciliation",
                            &serde_json::from_str::<String>(receipt_key)?,
                            &format!("result reconciliation: {outcome:?}"),
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
