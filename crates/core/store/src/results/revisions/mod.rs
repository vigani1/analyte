//! Records transactional observation revision history (ADR 0003).
mod admission;
mod assignment;
pub(crate) mod content;
mod history;
pub mod law;
mod outbox;
mod parking;
mod progression;
mod status_policy;
use crate::{Delivery, Interpretation, Reading, ReconciliationError as Error, Store, digest, key};
use admission::required_capacity;
use analyte_model::{IdentityStatus, ReceiptId, ResultContent, ResultStatus, Revision};
use assignment::{AssignedRevision, Reconciliation};
use content::clinical_digest;
use history::write_history;
use parking::{park_outcomes, reconciliation_succeeded, write_reprocess_audit};
use rusqlite::params;
pub use status_policy::StatusLifecycle;

/// Store decision for one result, in input order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevisionOutcome {
    /// First observed content under this identity.
    New(Revision),
    /// Already retained content; never enqueue another delivery.
    Retransmission(Revision),
    /// Changed content with permitted source progression, correction or withdrawal evidence.
    Correction(Revision),
    /// Changed content without an allowed source transition or correction evidence; parked.
    Conflict,
    /// Source did not provide a usable identity; parked.
    Unresolved,
}

/// Durable interpretation version and decisions for its results.
pub struct InterpretationCommit {
    /// Durable LAW disposition when this is a LAB-29 custody transaction.
    pub law: Option<law::Disposition>,
    /// Append-only interpretation version within the receipt.
    pub version: i64,
    /// One decision per input result.
    pub outcomes: Vec<RevisionOutcome>,
}

/// Reconciliation policy selected by the host.
pub enum ReconcileMode<'a> {
    /// Ordinary revision reconciliation.
    Standard,
    /// Apply the protocol's explicit status progression.
    Lifecycle(StatusLifecycle<'a>),
    /// Park otherwise indistinguishable edits with this protected reason.
    EditGuard(&'a str),
    /// Reconcile LAW custody and status progression in the same transaction.
    Law {
        /// Explicit LAW progression policy.
        lifecycle: StatusLifecycle<'a>,
        /// Retained work and message identity evidence.
        custody: &'a law::Custody,
    },
}

/// Mapping evidence retained together with revision assignment.
pub struct ReconcileInput<'a> {
    /// Config-declared correction evidence in result order.
    pub corrections: &'a [bool],
    /// Protected mapping findings.
    pub findings: &'a [crate::Parked],
    /// Protocol reconciliation policy.
    pub mode: ReconcileMode<'a>,
}

impl<'a> ReconcileInput<'a> {
    /// Selects ordinary reconciliation for the supplied mapping evidence.
    #[must_use]
    pub fn standard(corrections: &'a [bool], findings: &'a [crate::Parked]) -> Self {
        Self {
            corrections,
            findings,
            mode: ReconcileMode::Standard,
        }
    }
}

/// Identifies initial interpretation or an audited operator reprocessing.
#[derive(Clone, Copy)]
pub enum ReconcileAction<'a> {
    /// Interpret the immutable receipt.
    Interpret(ReceiptId),
    /// Append a version with the supplied audit evidence.
    Reprocess(&'a crate::Reprocess),
}

impl Store {
    #[expect(
        clippy::too_many_lines,
        reason = "Keep the single interpretation commit and rollback ordering visible together"
    )]
    /// Assigns revisions, findings, deliveries, optional LAW custody and reprocess audit atomically.
    /// Cancellation after submission can leave a committed transaction; reconcile the receipt.
    /// # Errors
    /// Rejects invalid audit, provenance, bounds, delivery selection and storage failures.
    pub fn reconcile<F>(
        &mut self,
        action: ReconcileAction<'_>,
        mut evidence: Interpretation,
        input: ReconcileInput<'_>,
        build: F,
    ) -> Result<InterpretationCommit, Error>
    where
        F: FnOnce(&Interpretation, &[RevisionOutcome]) -> Result<Vec<Delivery>, Error>,
    {
        let (receipt, action) = match action {
            ReconcileAction::Interpret(receipt) => (receipt, None),
            ReconcileAction::Reprocess(action) => {
                if action.audit.action.as_str() != "receipt-reprocess" {
                    return Err(Error::AuditMismatch);
                }
                (action.receipt, Some(action))
            }
        };
        let ReconcileInput {
            corrections,
            findings,
            mode,
        } = input;
        let (lifecycle, edit_guard, law) = match mode {
            ReconcileMode::Standard => (None, None, None),
            ReconcileMode::Lifecycle(policy) => (Some(policy), None, None),
            ReconcileMode::EditGuard(reason) => (None, Some(reason), None),
            ReconcileMode::Law { lifecycle, custody } => (Some(lifecycle), None, Some(custody)),
        };
        if let Some(custody) = law {
            self.encode(custody)?;
        }
        let stored = self.receipt(receipt)?.ok_or(Error::UnknownReceipt)?;
        let (count, findings) = self.validate_reconciliation(
            &stored,
            &evidence,
            corrections,
            findings,
            action.is_some(),
        )?;
        let limit = self.limits.record_bytes;
        let receipt_key = key(&receipt)?;
        let tx = self
            .db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        if let Some(action) = action
            && let Some(parked) = action.parked
        {
            crate::reprocess::check_parked(&tx, parked, receipt)?;
        }
        let version: i64 = tx.query_row(
            "SELECT coalesce(max(version),0)+1 FROM interpretations WHERE receipt=?1",
            [&receipt_key],
            |r| r.get(0),
        )?;
        let admission = law
            .map(|custody| {
                law::admit(
                    &tx,
                    &stored,
                    custody,
                    self.limits.page_rows,
                    action.is_some(),
                )
            })
            .transpose()?;
        if let Some(admission) = &admission {
            if let law::Disposition::Rejected(reason) = admission.disposition {
                evidence.outcome = Reading::Failed(
                    analyte_model::Reason::new(&format!(
                        "LAW {} at OBR occurrence {}",
                        reason.reason, reason.occurrence
                    ))
                    .map_err(Error::Text)?,
                );
            } else if admission.replay {
                evidence.outcome = Reading::Results(Vec::new());
            }
        }
        let reconciliation = Reconciliation {
            retention_key: self.retention_key.as_ref(),
            tx: &tx,
            machine: &stored.metadata.machine,
            limit,
            rows: self.limits.page_rows,
            lifecycle,
            edit_guard,
        };
        let batch = reconciliation.assign_revisions(&mut evidence, receipt, corrections)?;
        let deliveries = build(&evidence, &batch.outcomes)?;
        if deliveries.len() > count {
            return Err(Error::InvalidInterpretationInput);
        }
        let encoded = crate::document(&evidence)?;
        if encoded.len() > limit {
            return Err(Error::LimitExceeded {
                what: "record_bytes",
            });
        }
        let successful = admission
            .as_ref()
            .is_some_and(|a| matches!(a.disposition, law::Disposition::Accepted))
            || reconciliation_succeeded(&evidence, &batch.outcomes, findings.is_empty());
        let audit = action
            .map(|action| crate::reprocess::audit_evidence(action, version, successful))
            .transpose()?;
        let required = required_capacity(
            &encoded,
            audit.as_deref(),
            &findings,
            &deliveries,
            &batch.revisions,
            limit,
        )?;
        let required = required
            .checked_add(
                law.map(|c| crate::document(c).map(|s| s.len().saturating_mul(3)))
                    .transpose()?
                    .unwrap_or(0),
            )
            .ok_or(Error::Capacity)?;
        Self::capacity_on(&tx, &self.path, &self.limits, &*self.space, required)?;
        write_history(
            &tx,
            &receipt_key,
            version,
            &encoded,
            &findings,
            &batch.revisions,
        )?;
        reconciliation.write_deliveries(
            &receipt_key,
            version,
            &evidence,
            &batch.outcomes,
            deliveries,
        )?;
        if !admission.as_ref().is_some_and(|a| {
            a.replay
                || (matches!(a.disposition, law::Disposition::Accepted)
                    && matches!(&evidence.outcome, Reading::Results(results) if results.is_empty()))
        }) {
            park_outcomes(
                &tx,
                &receipt_key,
                version,
                &evidence,
                &batch.outcomes,
                (action.is_some(), findings.is_empty(), edit_guard),
            )?;
        }
        if let Some((custody, admission)) = law.zip(admission.as_ref()) {
            law::write(&tx, &stored, version, custody, admission)?;
        }
        write_reprocess_audit(&tx, action, audit, successful)?;
        tx.commit()?;
        Ok(InterpretationCommit {
            law: admission.map(|a| a.disposition),
            version,
            outcomes: batch.outcomes,
        })
    }
}

#[cfg(test)]
mod tests;

mod evidence;
