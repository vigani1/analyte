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
/// What makes two open items the same problem: their kind, reason and linked
/// records, and for a result its place in the reading. Which reading version
/// found it does not count, so reading a message again finds the same problem.
#[derive(PartialEq, Eq, Hash)]
struct Sameness {
    kind: String,
    reason: String,
    targets: Vec<String>,
    result: Option<usize>,
}

impl Sameness {
    fn of(evidence: &crate::parked::ParkingEvidence) -> Self {
        let details = &evidence.details;
        Self {
            kind: details.kind.as_str().to_owned(),
            reason: details.reason.as_str().to_owned(),
            targets: details
                .targets
                .iter()
                .map(|target| target.as_str().to_owned())
                .collect(),
            result: match evidence.context {
                Some(crate::parked::ParkingContext::Result { index, .. }) => Some(index),
                Some(crate::parked::ParkingContext::Interpretation { .. }) | None => None,
            },
        }
    }
}

/// Parks the items one reading of a message needs, one open item per problem
/// (owner report 2026-11-21): an item the message already has open, or one
/// parked earlier in the same reading, is not parked again. The reading
/// itself, and any audit, are still kept, so no evidence is lost.
pub(super) struct Parking<'a, 'db> {
    tx: &'a rusqlite::Transaction<'db>,
    receipt: String,
    open: std::collections::HashSet<Sameness>,
}

impl<'a, 'db> Parking<'a, 'db> {
    /// Reads the open items linked to the message, at most `rows` of them; an
    /// item beyond that bound, or one another version wrote in a shape this
    /// one cannot read, is only not compared, so at worst a copy is parked.
    pub(super) fn open(
        tx: &'a rusqlite::Transaction<'db>,
        receipt_key: &str,
        rows: usize,
    ) -> Result<Self, Error> {
        let receipt = serde_json::from_str::<String>(receipt_key)?;
        let mut stmt = tx.prepare(concat!(
            "SELECT p.evidence FROM parked p WHERE p.resolution IS NULL AND EXISTS(SELECT 1 FROM ",
            "json_each(p.evidence,'$.targets') t WHERE t.value=?1) ORDER BY p.id LIMIT ?2",
        ))?;
        let open = stmt
            .query_map(
                params![
                    receipt,
                    i64::try_from(rows).map_err(|_| Error::InvalidInterpretationInput)?
                ],
                |row| row.get::<_, String>(0),
            )?
            .collect::<Result<Vec<_>, _>>()?
            .iter()
            .filter_map(|stored| serde_json::from_str(stored).ok())
            .map(|evidence| Sameness::of(&evidence))
            .collect();
        Ok(Self { tx, receipt, open })
    }

    /// Parks one encoded item unless the same problem is already open.
    pub(super) fn park(&mut self, encoded: &str) -> Result<(), Error> {
        let evidence: crate::parked::ParkingEvidence = serde_json::from_str(encoded)?;
        if self.open.insert(Sameness::of(&evidence)) {
            self.tx
                .execute("INSERT INTO parked(evidence) VALUES (?1)", [encoded])?;
        }
        Ok(())
    }

    fn park_new(
        &mut self,
        kind: crate::ParkedKind,
        reason: &str,
        context: crate::parked::ParkingContext,
    ) -> Result<(), Error> {
        let encoded = crate::parked::evidence(kind.as_str(), &self.receipt, reason, Some(context))?;
        self.park(&encoded)
    }
}

pub(super) fn park_outcomes(
    parking: &mut Parking<'_, '_>,
    version: i64,
    evidence: &Interpretation,
    outcomes: &[RevisionOutcome],
    claimed: &[Option<analyte_model::ObservationIdentity>],
    context: (bool, bool, Option<&str>, Option<&str>),
) -> Result<(), Error> {
    let (reprocessing, findings_empty, edit_guard, held_back) = context;
    match &evidence.outcome {
        // The findings already say why the results were held back.
        Reading::Failed(reason) if !findings_empty && held_back == Some(reason.as_str()) => {}
        Reading::Failed(reason) => {
            parking.park_new(
                crate::ParkedKind::InterpretationFailure,
                reason.as_str(),
                crate::parked::ParkingContext::Interpretation { version },
            )?;
        }
        Reading::Results(results) => {
            if reprocessing && results.is_empty() && findings_empty {
                parking.park_new(
                    crate::ParkedKind::InterpretationFailure,
                    NO_RESULTS_REASON,
                    crate::parked::ParkingContext::Interpretation { version },
                )?;
            }
            for (index, (result, outcome)) in results.iter().zip(outcomes).enumerate() {
                let reason = match (&result.identity, edit_guard) {
                    (IdentityStatus::Unresolved(reason), Some(guard))
                        if reason.as_str() == guard =>
                    {
                        guard.to_owned()
                    }
                    // A result that arrived without an identity is explained by
                    // the reading's findings, which name why; it gets an item of
                    // its own only when no finding was given, never none.
                    (IdentityStatus::Unresolved(_), _)
                        if *outcome == RevisionOutcome::Unresolved && !findings_empty =>
                    {
                        continue;
                    }
                    _ => reconciliation_reason(outcome),
                };
                if result.requires_parking()
                    || matches!(
                        outcome,
                        RevisionOutcome::Conflict | RevisionOutcome::Unresolved
                    )
                {
                    parking.park_new(
                        crate::ParkedKind::ResultReconciliation,
                        &reason,
                        crate::parked::ParkingContext::Result {
                            version: Some(version),
                            index,
                            claimed: (*outcome == RevisionOutcome::Conflict)
                                .then(|| claimed.get(index).cloned().flatten())
                                .flatten(),
                        },
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
