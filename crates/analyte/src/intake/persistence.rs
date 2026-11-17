//! Commits prepared mapping evidence and retains rejected atomic batches after rollback.
use super::{
    BatchConflict, Identifier, Interpretation, InterpretationOutcome, Policy, Reading, ReceiptId,
    ReceiptMetadata, RevisionOutcome, park_findings,
};
/// Boundaries combined while committing prepared mapping evidence.
#[derive(Debug, thiserror::Error)]
pub enum CommitError {
    /// Reading the retained receipt failed.
    #[error(transparent)]
    Receipt(#[from] analyte_store::OperatorError),
    /// Atomic interpretation and outbox reconciliation failed.
    #[error(transparent)]
    Reconciliation(#[from] analyte_store::ReconciliationError),
    /// Finding evidence could not fit the model bounds.
    #[error(transparent)]
    Text(#[from] analyte_model::TextError),
}
impl From<analyte_store::WorkerError> for CommitError {
    fn from(error: analyte_store::WorkerError) -> Self {
        Self::Reconciliation(error.into())
    }
}
pub(super) struct PreparedMapping {
    pub(super) id: ReceiptId,
    pub(super) findings: Vec<String>,
    pub(super) corrections: Vec<bool>,
    pub(super) interpretation: Interpretation,
    pub(super) destination: Identifier,
    pub(super) conflict: Option<Interpretation>,
    pub(super) policy: Policy,
    pub(super) action: Option<analyte_store::Reprocess>,
    pub(super) accepted: bool,
}

impl PreparedMapping {
    pub(super) fn commit(
        self,
        store: &mut analyte_store::Store,
    ) -> Result<(analyte_store::InterpretationCommit, InterpretationOutcome), CommitError> {
        let Self {
            id,
            findings,
            corrections,
            interpretation,
            destination,
            conflict,
            policy,
            action,
            accepted,
        } = self;
        let atomic = policy.atomic();
        let lifecycle = policy.lifecycle();
        let findings = park_findings(id, findings)?;
        let receipt = store
            .receipt(id)?
            .ok_or(analyte_store::ReconciliationError::Conflict)?
            .metadata;
        let build = |assigned: &Interpretation, outcomes: &[RevisionOutcome]| {
            if atomic
                && outcomes.iter().any(|outcome| {
                    matches!(
                        outcome,
                        RevisionOutcome::Conflict | RevisionOutcome::Unresolved
                    )
                })
            {
                return Err(analyte_store::ReconciliationError::DeliveryBuild(Box::new(
                    BatchConflict,
                )));
            }
            if !accepted {
                return Ok(Vec::new());
            }
            build_deliveries(assigned, outcomes, &receipt, &destination, lifecycle)
        };
        let commit = store.reconcile(
            reconciliation_action(id, action.as_ref()),
            interpretation,
            analyte_store::ReconcileInput {
                corrections: &corrections,
                findings: &findings,
                mode: policy.mode(),
            },
            build,
        );
        let (commit, successful) = match commit {
            // Builder refusal rolls back revisions, audit and deliveries together.
            // Retain a failed version and its operator audit in a separate transaction.
            Err(analyte_store::ReconciliationError::DeliveryBuild(error))
                if error.is::<BatchConflict>() =>
            {
                (
                    reject_batch(store, id, conflict, &policy, action.as_ref(), &findings)?,
                    InterpretationOutcome::Conflict,
                )
            }
            result => (
                result?,
                if accepted {
                    InterpretationOutcome::Accepted
                } else {
                    InterpretationOutcome::Failed
                },
            ),
        };
        let successful = policy.outcome(&commit, successful);
        Ok((commit, successful))
    }
}

fn reconciliation_action(
    id: ReceiptId,
    action: Option<&analyte_store::Reprocess>,
) -> analyte_store::ReconcileAction<'_> {
    action.map_or(
        analyte_store::ReconcileAction::Interpret(id),
        analyte_store::ReconcileAction::Reprocess,
    )
}

fn reject_batch(
    store: &mut analyte_store::Store,
    id: ReceiptId,
    conflict: Option<Interpretation>,
    policy: &Policy,
    action: Option<&analyte_store::Reprocess>,
    findings: &[analyte_store::Parked],
) -> Result<analyte_store::InterpretationCommit, analyte_store::ReconciliationError> {
    let rejected_evidence =
        conflict.ok_or(analyte_store::ReconciliationError::ConflictEvidenceRequired)?;
    let rejected_policy = policy.rejected();
    store.reconcile(
        reconciliation_action(id, action),
        rejected_evidence,
        analyte_store::ReconcileInput {
            corrections: &[],
            findings,
            mode: rejected_policy.mode(),
        },
        |_, _| Ok(Vec::new()),
    )
}

// The store assigns revisions before this callback freezes bytes. Only new and
// corrected production results can cross this boundary; replay never enqueues.
fn build_deliveries(
    assigned: &Interpretation,
    outcomes: &[RevisionOutcome],
    receipt: &ReceiptMetadata,
    destination: &Identifier,
    lifecycle: Option<analyte_store::StatusLifecycle<'_>>,
) -> Result<Vec<analyte_store::Delivery>, analyte_store::ReconciliationError> {
    let mut deliveries = Vec::new();
    if let Reading::Results(results) = &assigned.outcome {
        for (result, outcome) in results.iter().zip(outcomes) {
            let fresh = match outcome {
                RevisionOutcome::New(_) | RevisionOutcome::Correction(_) => true,
                RevisionOutcome::Retransmission(_)
                | RevisionOutcome::Conflict
                | RevisionOutcome::Unresolved => false,
            };
            if fresh
                && lifecycle.is_none_or(|policy| policy.may_deliver(result))
                && result.processing.may_deliver_to_production()
                && !result.requires_parking()
            {
                deliveries.push(
                    analyte_sink::encode_delivery(
                        result,
                        receipt,
                        destination.clone(),
                        jiff::Timestamp::now(),
                    )
                    .map_err(|error| {
                        analyte_store::ReconciliationError::DeliveryBuild(Box::new(error))
                    })?,
                );
            }
        }
    }
    Ok(deliveries)
}
