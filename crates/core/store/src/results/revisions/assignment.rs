//! Assigns result revisions within one transaction and bounded batch state.
use super::*;
pub(super) type AssignedRevision = (String, i64, analyte_model::Digest);

#[derive(Default)]
pub(super) struct RevisionBatch {
    pub(super) outcomes: Vec<RevisionOutcome>,
    pub(super) revisions: Vec<AssignedRevision>,
    pub(super) staged: Vec<(String, ResultContent)>,
}

pub(super) struct Reconciliation<'a, 'db> {
    pub(super) retention_key: Option<&'a crate::DigestKey>,
    pub(super) tx: &'a rusqlite::Transaction<'db>,
    pub(super) machine: &'a analyte_model::Identifier,
    pub(super) limit: usize,
    pub(super) rows: usize,
    pub(super) lifecycle: Option<StatusLifecycle<'a>>,
    pub(super) edit_guard: Option<&'a str>,
}

impl Reconciliation<'_, '_> {
    pub(super) fn assign_revisions(
        &self,
        evidence: &mut Interpretation,
        receipt: ReceiptId,
        corrections: &[bool],
    ) -> Result<RevisionBatch, Error> {
        let mut batch = RevisionBatch::default();
        if let Reading::Results(results) = &mut evidence.outcome {
            for (result, correction) in results.iter_mut().zip(corrections) {
                self.assign_result(result, receipt, *correction, &mut batch)?;
            }
        }
        Ok(batch)
    }
    pub(super) fn assign_result(
        &self,
        result: &mut ResultContent,
        receipt: ReceiptId,
        correction: bool,
        batch: &mut RevisionBatch,
    ) -> Result<(), Error> {
        if result.receipt != receipt {
            return Err(Error::InvalidInterpretationInput);
        }
        let candidate = result.clone();
        let content = clinical_digest(result)?;
        let IdentityStatus::Resolved(identity) = &mut result.identity else {
            batch.outcomes.push(RevisionOutcome::Unresolved);
            return Ok(());
        };
        let identity_key = key(&(self.machine, &identity.observation))?;
        if let Some(policy) = self.lifecycle
            && status_policy::other_final_run(
                self.tx,
                self.machine,
                &candidate,
                &batch.staged,
                policy,
                self.rows,
                self.limit,
            )?
        {
            result.identity = IdentityStatus::Unresolved(
                analyte_model::Reason::new("multiple final runs under the same order and test")
                    .map_err(Error::Text)?,
            );
            batch.outcomes.push(RevisionOutcome::Conflict);
            return Ok(());
        }
        let same = self.same_revision(
            &identity_key,
            &identity.observation,
            &content,
            &batch.revisions,
        )?;
        let previous = self.previous_revision(&identity_key, &batch.revisions)?;
        if correction && let Some(reason) = self.edit_guard {
            if same.is_some() && same == previous {
                result.identity = IdentityStatus::Unresolved(
                    analyte_model::Reason::new(reason).map_err(Error::Text)?,
                );
                batch.outcomes.push(RevisionOutcome::Unresolved);
                return Ok(());
            }
        } else if let Some(revision) = same {
            identity.revision = Revision(u64::try_from(revision).map_err(|_| Error::Corruption)?);
            batch
                .outcomes
                .push(RevisionOutcome::Retransmission(identity.revision));
            return Ok(());
        }
        let progression = self.permits_progression(
            &identity_key,
            &candidate,
            previous,
            correction,
            &batch.staged,
        )?;
        let outcome = match previous {
            None => RevisionOutcome::New(Revision(0)),
            Some(previous) if progression => RevisionOutcome::Correction(Revision(
                u64::try_from(previous.checked_add(1).ok_or(Error::Conflict)?)
                    .map_err(|_| Error::Corruption)?,
            )),
            Some(_) => RevisionOutcome::Conflict,
        };
        match outcome {
            RevisionOutcome::New(revision) | RevisionOutcome::Correction(revision) => {
                identity.revision = revision;
                batch.staged.push((identity_key.clone(), result.clone()));
                batch.revisions.push((
                    identity_key,
                    i64::try_from(revision.0).map_err(|_| Error::Conflict)?,
                    content,
                ));
            }
            RevisionOutcome::Conflict => {
                result.identity = IdentityStatus::Unresolved(
                    analyte_model::Reason::new(
                        "conflicting content under retained observation identity",
                    )
                    .map_err(Error::Text)?,
                );
            }
            RevisionOutcome::Retransmission(_) | RevisionOutcome::Unresolved => {
                return Err(Error::InvalidInterpretationInput);
            }
        }
        batch.outcomes.push(outcome);
        Ok(())
    }
}
