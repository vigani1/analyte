//! Commits prepared mapping evidence and retains rejected LAW batches after rollback.
use super::{
    Identifier, Interpretation, InterpretationOutcome, Reading, ReceiptId, ReceiptMetadata,
    RevisionOutcome, park_findings,
};
pub(super) struct PreparedMapping {
    pub(super) id: ReceiptId,
    pub(super) findings: Vec<String>,
    pub(super) corrections: Vec<bool>,
    pub(super) interpretation: Interpretation,
    pub(super) destination: Identifier,
    pub(super) conflict: Option<Interpretation>,
    pub(super) custody: Option<analyte_store::law::Custody>,
    pub(super) action: Option<analyte_store::Reprocess>,
    pub(super) lifecycle: Option<analyte_store::StatusLifecycle<'static>>,
    pub(super) atomic: bool,
    pub(super) accepted: bool,
    pub(super) poct_edit_guard: bool,
}

impl PreparedMapping {
    pub(super) fn commit(
        self,
        store: &mut analyte_store::Store,
    ) -> Result<(analyte_store::InterpretationCommit, InterpretationOutcome), analyte_store::Error>
    {
        let Self {
            id,
            findings,
            corrections,
            interpretation,
            destination,
            conflict,
            custody,
            action,
            lifecycle,
            atomic,
            accepted,
            poct_edit_guard,
        } = self;
        let findings = park_findings(id, findings)?;
        let receipt = store
            .receipt(id)?
            .ok_or(analyte_store::Error::Conflict)?
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
                return Err(analyte_store::Error::DeliveryBuild(Box::new(
                    LawBatchConflict,
                )));
            }
            if !accepted {
                return Ok(Vec::new());
            }
            build_deliveries(assigned, outcomes, &receipt, &destination, lifecycle)
        };
        let commit = if let Some(custody) = &custody {
            store.reconcile(
                action.as_ref().map_or(
                    analyte_store::ReconcileAction::Interpret(id),
                    analyte_store::ReconcileAction::Reprocess,
                ),
                interpretation.clone(),
                analyte_store::ReconcileInput {
                    corrections: &corrections,
                    findings: &findings,
                    mode: analyte_store::ReconcileMode::Law {
                        lifecycle: lifecycle
                            .ok_or(analyte_store::Error::LifecyclePolicyRequired)?,
                        custody,
                    },
                },
                build,
            )
        } else {
            store.reconcile(
                reconciliation_action(id, action.as_ref()),
                interpretation,
                analyte_store::ReconcileInput {
                    corrections: &corrections,
                    findings: &findings,
                    mode: reconciliation_mode(lifecycle, poct_edit_guard),
                },
                build,
            )
        };
        match commit {
            // The builder refusal rolls back revisions, audit and deliveries together.
            // Keep a failed version (and its audit for operator actions) afterwards.
            Err(analyte_store::Error::DeliveryBuild(error)) if error.is::<LawBatchConflict>() => {
                let rejected = reject_batch(
                    store,
                    id,
                    conflict,
                    custody.as_ref(),
                    action.as_ref(),
                    &findings,
                    lifecycle,
                )?;
                Ok((rejected, InterpretationOutcome::Conflict))
            }
            result => result.map(|commit| {
                (
                    commit,
                    if accepted {
                        InterpretationOutcome::Accepted
                    } else {
                        InterpretationOutcome::Failed
                    },
                )
            }),
        }
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
    custody: Option<&analyte_store::law::Custody>,
    action: Option<&analyte_store::Reprocess>,
    findings: &[analyte_store::Parked],
    lifecycle: Option<analyte_store::StatusLifecycle<'_>>,
) -> Result<analyte_store::InterpretationCommit, analyte_store::Error> {
    let rejected_evidence = conflict.ok_or(analyte_store::Error::ConflictEvidenceRequired)?;
    let rejected = if let Some(mut custody) = custody.cloned() {
        custody.rejection = Some(analyte_store::law::Rejection {
            occurrence: 1,
            reason: analyte_store::law::Conflict::Progression,
        });
        store.reconcile(
            action.map_or(
                analyte_store::ReconcileAction::Interpret(id),
                analyte_store::ReconcileAction::Reprocess,
            ),
            rejected_evidence,
            analyte_store::ReconcileInput {
                corrections: &[],
                findings,
                mode: analyte_store::ReconcileMode::Law {
                    lifecycle: lifecycle.ok_or(analyte_store::Error::LifecyclePolicyRequired)?,
                    custody: &custody,
                },
            },
            |_, _| Ok(Vec::new()),
        )?
    } else {
        store.reconcile(
            reconciliation_action(id, action),
            rejected_evidence,
            analyte_store::ReconcileInput {
                corrections: &[],
                findings,
                mode: reconciliation_mode(lifecycle, false),
            },
            |_, _| Ok(Vec::new()),
        )?
    };

    Ok(rejected)
}

fn reconciliation_mode(
    lifecycle: Option<analyte_store::StatusLifecycle<'_>>,
    edit_guard: bool,
) -> analyte_store::ReconcileMode<'_> {
    if edit_guard {
        analyte_store::ReconcileMode::EditGuard(
            "edited POCT observation with no detectable common-content change; source context may have changed",
        )
    } else {
        lifecycle.map_or(
            analyte_store::ReconcileMode::Standard,
            analyte_store::ReconcileMode::Lifecycle,
        )
    }
}

#[derive(Debug, thiserror::Error)]
#[error(
    "LAW result batch conflicts with retained observation history; no result revisions committed"
)]
pub(super) struct LawBatchConflict;

// The store assigns revisions before this callback freezes bytes. Only new and
// corrected production results can cross this boundary; replay never enqueues.
fn build_deliveries(
    assigned: &Interpretation,
    outcomes: &[RevisionOutcome],
    receipt: &ReceiptMetadata,
    destination: &Identifier,
    lifecycle: Option<analyte_store::StatusLifecycle<'_>>,
) -> Result<Vec<analyte_store::Delivery>, analyte_store::Error> {
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
                    .map_err(|error| analyte_store::Error::DeliveryBuild(Box::new(error)))?,
                );
            }
        }
    }
    Ok(deliveries)
}
