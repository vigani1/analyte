//! Records transactional observation revision history (ADR 0003).
mod admission;
mod assignment;
pub(crate) mod content;
mod history;
pub(crate) mod law;
mod outbox;
mod parking;
mod progression;
mod status_policy;
use crate::{Delivery, Error, Interpretation, Reading, Store, digest, key};
use admission::required_capacity;
use analyte_model::{IdentityStatus, ReceiptId, ResultContent, ResultStatus, Revision};
use assignment::{AssignedRevision, Reconciliation};
use content::clinical_digest;
use history::write_history;
use parking::{park_outcomes, reconciliation_succeeded, write_reprocess_audit};
use rusqlite::{OptionalExtension, params};
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
    pub law: Option<law::LawDisposition>,
    /// Append-only interpretation version within the receipt.
    pub version: i64,
    /// One decision per input result.
    pub outcomes: Vec<RevisionOutcome>,
}

impl Store {
    /// Assigns revisions and stores interpretation, conflicts and eligible deliveries atomically.
    /// Mapping `findings` are parked in this same transaction, including on retransmission.
    /// `corrections` supplies the config-declared correction evidence per result.
    /// The bounded builder sees assigned revisions; it must produce deliveries only for new
    /// or corrected results. Retransmissions and conflicts cannot be enqueued.
    /// Cancellation after submission can leave a committed transaction; reconcile the receipt.
    /// # Errors
    /// Rejects bounds, provenance mismatches, invalid delivery selection and database failures.
    pub fn interpret_reconciled<F>(
        &mut self,
        receipt: ReceiptId,
        evidence: Interpretation,
        corrections: &[bool],
        findings: &[crate::Parked],
        build: F,
    ) -> Result<InterpretationCommit, Error>
    where
        F: FnOnce(&Interpretation, &[RevisionOutcome]) -> Result<Vec<Delivery>, Error>,
    {
        self.reconcile(
            receipt,
            evidence,
            (corrections, findings, None, None),
            None,
            None,
            build,
        )
    }

    /// Reinterprets retained bytes under current provenance, appending a version.
    /// Reconciliation, parking, resolution and exactly one audit commit together.
    /// No machine I/O or order lifecycle operation is performed.
    /// # Errors
    /// Rejects missing receipts, unrelated/resolved parked items, invalid audit evidence,
    /// bounds, delivery construction and storage failures without partial effects.
    pub fn reprocess_reconciled<F>(
        &mut self,
        action: &crate::Reprocess,
        evidence: Interpretation,
        corrections: &[bool],
        findings: &[crate::Parked],
        build: F,
    ) -> Result<InterpretationCommit, Error>
    where
        F: FnOnce(&Interpretation, &[RevisionOutcome]) -> Result<Vec<Delivery>, Error>,
    {
        if action.audit.action.as_str() != "receipt-reprocess" {
            return Err(Error::AuditMismatch);
        }
        self.reconcile(
            action.receipt,
            evidence,
            (corrections, findings, None, None),
            Some(action),
            None,
            build,
        )
    }

    /// Reconciles using a caller-owned status transition/reporting table (ADR 0011).
    /// Identical retained content is still a retransmission; a table cannot authorize delivery of it.
    /// # Errors
    /// Reports the same atomic validation, capacity and storage failures as `interpret_reconciled`.
    pub fn interpret_with_lifecycle<F>(
        &mut self,
        receipt: ReceiptId,
        evidence: Interpretation,
        mapping: (&[bool], &[crate::Parked]),
        lifecycle: StatusLifecycle<'_>,
        build: F,
    ) -> Result<InterpretationCommit, Error>
    where
        F: FnOnce(&Interpretation, &[RevisionOutcome]) -> Result<Vec<Delivery>, Error>,
    {
        self.reconcile(
            receipt,
            evidence,
            (mapping.0, mapping.1, Some(lifecycle), None),
            None,
            None,
            build,
        )
    }
    /// Reprocesses with caller-owned lifecycle rules, keeping the same atomic audit boundary.
    /// # Errors
    /// Reports invalid actions and the same atomic failures as `reprocess_reconciled`.
    pub fn reprocess_with_lifecycle<F>(
        &mut self,
        action: &crate::Reprocess,
        evidence: Interpretation,
        mapping: (&[bool], &[crate::Parked]),
        lifecycle: StatusLifecycle<'_>,
        build: F,
    ) -> Result<InterpretationCommit, Error>
    where
        F: FnOnce(&Interpretation, &[RevisionOutcome]) -> Result<Vec<Delivery>, Error>,
    {
        if action.audit.action.as_str() != "receipt-reprocess" {
            return Err(Error::AuditMismatch);
        }
        self.reconcile(
            action.receipt,
            evidence,
            (mapping.0, mapping.1, Some(lifecycle), None),
            Some(action),
            None,
            build,
        )
    }

    /// Parks corrections matching the latest common content under a caller-owned reason.
    /// The guard runs inside revision reconciliation; it changes no stored format.
    /// Reprocessing keeps the usual audit and custody boundary.
    /// # Errors
    /// Rejects invalid reprocess actions, bounds and transactional storage failures.
    pub fn interpret_with_edit_guard<F>(
        &mut self,
        receipt: ReceiptId,
        evidence: Interpretation,
        mapping: (&[bool], &[crate::Parked], &str),
        action: Option<&crate::Reprocess>,
        build: F,
    ) -> Result<InterpretationCommit, Error>
    where
        F: FnOnce(&Interpretation, &[RevisionOutcome]) -> Result<Vec<Delivery>, Error>,
    {
        if action
            .is_some_and(|a| a.receipt != receipt || a.audit.action.as_str() != "receipt-reprocess")
        {
            return Err(Error::AuditMismatch);
        }
        self.reconcile(
            receipt,
            evidence,
            (mapping.0, mapping.1, None, Some(mapping.2)),
            action,
            None,
            build,
        )
    }

    /// Commits LAW receipt disposition, item execution, revisions and deliveries atomically.
    /// No positive disposition exists until commit; cancellation can leave a committed
    /// transaction, so retries use the same trusted message identity.
    /// # Errors
    /// Refuses invalid provenance, bounds, audit and storage failures without partial writes.
    pub fn interpret_law<F>(
        &mut self,
        receipt: ReceiptId,
        evidence: Interpretation,
        mapping: (&[bool], &[crate::Parked]),
        lifecycle: StatusLifecycle<'_>,
        context: (&law::LawCustody, Option<&crate::Reprocess>),
        build: F,
    ) -> Result<InterpretationCommit, Error>
    where
        F: FnOnce(&Interpretation, &[RevisionOutcome]) -> Result<Vec<Delivery>, Error>,
    {
        let (custody, action) = context;
        if action
            .is_some_and(|a| a.receipt != receipt || a.audit.action.as_str() != "receipt-reprocess")
        {
            return Err(Error::AuditMismatch);
        }
        self.reconcile(
            receipt,
            evidence,
            (mapping.0, mapping.1, Some(lifecycle), None),
            action,
            Some(custody),
            build,
        )
    }

    #[expect(
        clippy::too_many_lines,
        reason = "Keep the single interpretation commit and rollback ordering visible together"
    )]
    fn reconcile<F>(
        &mut self,
        receipt: ReceiptId,
        mut evidence: Interpretation,
        mapping: (
            &[bool],
            &[crate::Parked],
            Option<StatusLifecycle<'_>>,
            Option<&str>,
        ),
        action: Option<&crate::Reprocess>,
        law: Option<&law::LawCustody>,
        build: F,
    ) -> Result<InterpretationCommit, Error>
    where
        F: FnOnce(&Interpretation, &[RevisionOutcome]) -> Result<Vec<Delivery>, Error>,
    {
        if let Some(custody) = law {
            self.encode(custody)?;
        }
        let (corrections, findings, lifecycle, edit_guard) = mapping;
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
            if let law::LawDisposition::Rejected(reason) = admission.disposition {
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
            .is_some_and(|a| matches!(a.disposition, law::LawDisposition::Accepted))
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
                || (matches!(a.disposition, law::LawDisposition::Accepted)
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
