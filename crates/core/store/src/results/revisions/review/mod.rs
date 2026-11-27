//! Compares protected result evidence and commits explicit operator decisions.
use super::{Error, Reading, Reconciliation, RevisionOutcome, clinical_digest, key, params};
mod suggestion;

use crate::parked::{ParkingContext, ParkingEvidence};
use crate::{Audit, Delivery, Interpretation, StorageError, Store};
use analyte_model::{
    Identifier, IdentityStatus, Reason, ReceiptMetadata, ResultContent, ResultIdentity, Revision,
    SourceText,
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior};
use serde::Serialize;

/// Audit action for an asserted resend.
pub const RESEND_MEASUREMENT_ACTION: &str = "parked-measurement-resend";
/// Audit action for an asserted rerun.
pub const RERUN_MEASUREMENT_ACTION: &str = "parked-measurement-rerun";
/// Audit action for an asserted correction.
pub const CORRECTION_MEASUREMENT_ACTION: &str = "parked-measurement-correction";
/// Audit action for a superseded missing-ID receipt.
pub const SUPERSEDE_SAMPLE_ACTION: &str = "parked-sample-superseded";

/// An explicit operator decision about two retained measurements.
#[derive(Clone, Copy)]
pub enum MeasurementDecision {
    /// Link the held result to the earlier one without another delivery.
    Resend,
    /// Assign an audited host measurement identity and queue one delivery.
    Rerun,
    /// Deliver the held reading as the next revision of the earlier result.
    Correction,
    /// Close the missing-sample finding in favor of the later receipt.
    Supersede,
}

impl MeasurementDecision {
    fn action(self) -> &'static str {
        match self {
            Self::Resend => RESEND_MEASUREMENT_ACTION,
            Self::Rerun => RERUN_MEASUREMENT_ACTION,
            Self::Correction => CORRECTION_MEASUREMENT_ACTION,
            Self::Supersede => SUPERSEDE_SAMPLE_ACTION,
        }
    }
}

/// Snapshot and audit evidence supplied by one explicit confirmation.
pub struct MeasurementConfirmation<'a> {
    /// Open item the operator reviewed.
    pub item: i64,
    /// Snapshot printed or displayed alongside the comparison.
    pub snapshot: &'a str,
    /// Which decision the operator confirmed.
    pub decision: MeasurementDecision,
    /// Signed-in actor or explicit command-line actor.
    pub actor: Identifier,
    /// Unix second when the action was submitted.
    pub time: i64,
    /// Required reason supplied by the operator.
    pub reason: Reason,
}

/// Protected comparison for one open review item.
#[derive(Serialize)]
pub struct MeasurementReview {
    /// Results covered by the open item.
    pub held: Vec<ResultContent>,
    /// Retained comparison results in matching source order.
    pub other: Vec<ResultContent>,
    /// Whether the comparison is a later receipt with a sample ID.
    pub suggestion: bool,
    /// Whether the held reading differs clinically from the retained one, so
    /// it can be confirmed as that result's next revision.
    pub correctable: bool,
    /// Exact snapshot the operator must confirm.
    pub snapshot: String,
}

struct Comparison {
    view: MeasurementReview,
    version: i64,
    index: usize,
    evidence: Interpretation,
    metadata: ReceiptMetadata,
}

impl Store {
    /// Reads a protected comparison and audits disclosure before returning it.
    /// # Errors
    /// Reports retained-evidence, capacity and audit failures.
    pub fn measurement_review(
        &mut self,
        id: i64,
        actor: Identifier,
        time: i64,
    ) -> Result<Option<MeasurementReview>, Error> {
        let tx = rusqlite::Transaction::new_unchecked(&self.db, TransactionBehavior::Immediate)?;
        let Some(comparison) =
            comparison(&tx, id, self.limits.page_rows, self.limits.record_bytes)?
        else {
            return Ok(None);
        };
        let encoded = crate::document(&audit(
            id,
            &comparison.view,
            actor,
            time,
            "parked-measurement-view",
            Reason::new("operator inspected measurement comparison").map_err(StorageError::Text)?,
        )?)?;
        if encoded.len() > self.limits.record_bytes {
            return Err(Error::InvalidInterpretationInput);
        }
        Self::capacity_on(&tx, &self.path, &self.limits, &*self.space, encoded.len())?;
        tx.execute("INSERT INTO audit(evidence) VALUES (?1)", [encoded])?;
        tx.commit()?;
        Ok(Some(comparison.view))
    }

    /// Rechecks the displayed snapshot and records the decision atomically.
    /// Cancellation can leave a committed decision and queued delivery.
    /// # Errors
    /// Refuses stale or ineligible decisions and rolls back on construction or storage failure.
    pub fn decide_measurement<F>(
        &mut self,
        confirmation: MeasurementConfirmation<'_>,
        delivery: F,
    ) -> Result<(), Error>
    where
        F: FnOnce(&ResultContent, &ReceiptMetadata) -> Result<Delivery, Error>,
    {
        let MeasurementConfirmation {
            item: id,
            snapshot,
            decision,
            actor,
            time,
            reason,
        } = confirmation;
        let tx = rusqlite::Transaction::new_unchecked(&self.db, TransactionBehavior::Immediate)?;
        let comparison = comparison(&tx, id, self.limits.page_rows, self.limits.record_bytes)?
            .ok_or(Error::ReviewChanged)?;
        if comparison.view.snapshot != snapshot
            || comparison.view.suggestion != matches!(decision, MeasurementDecision::Supersede)
            || (matches!(decision, MeasurementDecision::Correction) && !comparison.view.correctable)
            || reason.as_str().trim().is_empty()
        {
            return Err(Error::ReviewChanged);
        }
        let encoded = crate::document(&audit(
            id,
            &comparison.view,
            actor,
            time,
            decision.action(),
            reason,
        )?)?;
        if encoded.len() > self.limits.record_bytes {
            return Err(Error::InvalidInterpretationInput);
        }
        match decision {
            MeasurementDecision::Rerun | MeasurementDecision::Correction => {
                write_decided(&tx, self, comparison, decision, &encoded, delivery)?;
            }
            MeasurementDecision::Resend | MeasurementDecision::Supersede => {
                Self::capacity_on(
                    &tx,
                    &self.path,
                    &self.limits,
                    &*self.space,
                    encoded.len().saturating_mul(2),
                )?;
            }
        }
        if tx.execute(
            "UPDATE parked SET resolution=?2 WHERE id=?1 AND resolution IS NULL",
            params![id, &encoded],
        )? != 1
        {
            return Err(Error::ReviewChanged);
        }
        tx.execute("INSERT INTO audit(evidence) VALUES (?1)", [encoded])?;
        tx.commit()?;
        Ok(())
    }
}

pub(super) fn first(results: &[ResultContent]) -> Result<&ResultContent, Error> {
    results.first().ok_or(Error::Corruption)
}

fn target(value: &str) -> Result<Identifier, Error> {
    Ok(Identifier::new(value).map_err(StorageError::Text)?)
}

fn audit(
    id: i64,
    view: &MeasurementReview,
    actor: Identifier,
    time: i64,
    action: &str,
    reason: Reason,
) -> Result<Audit, Error> {
    Ok(Audit {
        actor,
        time,
        reason,
        action: target(action)?,
        targets: vec![
            target(&format!("parked:{id}"))?,
            target(&uuid::Uuid::from(first(&view.held)?.receipt).to_string())?,
            target(&uuid::Uuid::from(first(&view.other)?.receipt).to_string())?,
            target(&view.snapshot)?,
        ],
    })
}

fn latest(db: &Connection, receipt: &str) -> Result<Option<(i64, Interpretation)>, Error> {
    let row: Option<(i64, String)> = db.query_row(
        "SELECT version,evidence FROM interpretations WHERE receipt=?1 ORDER BY version DESC LIMIT 1",
        [receipt], |r| Ok((r.get(0)?, r.get(1)?)),
    ).optional()?;
    row.map(|(version, evidence)| Ok((version, serde_json::from_str(&evidence)?)))
        .transpose()
}

fn comparison(
    db: &Connection,
    id: i64,
    rows: usize,
    limit: usize,
) -> Result<Option<Comparison>, Error> {
    let Some(encoded): Option<String> = db
        .query_row(
            "SELECT evidence FROM parked WHERE id=?1 AND resolution IS NULL",
            [id],
            |r| r.get(0),
        )
        .optional()?
    else {
        return Ok(None);
    };
    let item: ParkingEvidence = serde_json::from_str(&encoded)?;
    let [receipt] = item.details.targets.as_slice() else {
        return Ok(None);
    };
    let receipt = crate::document(receipt)?;
    let Some((version, evidence)) = latest(db, &receipt)? else {
        return Ok(None);
    };
    let Reading::Results(results) = &evidence.outcome else {
        return Ok(None);
    };
    let metadata: String = db.query_row(
        "SELECT metadata FROM receipts WHERE id=?1",
        [&receipt],
        |r| r.get(0),
    )?;
    let metadata: ReceiptMetadata = serde_json::from_str(&metadata)?;
    let (index, held, other, suggestion, correctable) = match item.context {
        Some(ParkingContext::Result {
            version: Some(parked_version),
            index,
            claimed: Some(claimed),
        }) if claimed
            .run
            .as_str()
            .starts_with(|c: char| c.is_ascii_digit()) =>
        {
            if parked_version != version
                && !unchanged_result(db, &receipt, parked_version, index, &evidence, results)?
            {
                return Ok(None);
            }
            let observation = key(&(&metadata.machine, &claimed))?;
            let Some(other) = super::status_policy::latest(db, &observation, limit)? else {
                return Ok(None);
            };
            let held = results.get(index).ok_or(Error::Corruption)?;
            if held.sample.as_ref().map(|sample| &sample.id)
                != other.sample.as_ref().map(|sample| &sample.id)
                || held.sample.is_none()
                || held.receipt == other.receipt
            {
                return Ok(None);
            }
            let correctable = clinical_digest(held)? != clinical_digest(&other)?;
            (index, vec![held.clone()], vec![other], false, correctable)
        }
        None if item.details.kind.as_str() == crate::ParkedKind::MappingFinding.as_str()
            && item.details.reason.as_str().ends_with("}: MissingSample")
            && !results.is_empty()
            && results.iter().all(|r| r.sample.is_none()) =>
        {
            let findings: i64 = db.query_row(
                "SELECT count(*) FROM parked WHERE json_extract(evidence,'$.targets[0]')=json_extract(?1,'$')
                 AND json_extract(evidence,'$.reason') LIKE '%}: MissingSample'",
                [&receipt], |row| row.get(0),
            )?;
            if findings != 1 {
                return Ok(None);
            }
            let Some(other) = suggestion::suggested(db, &receipt, &metadata, results, rows)? else {
                return Ok(None);
            };
            (0, results.clone(), other, true, false)
        }
        Some(ParkingContext::Result { .. } | ParkingContext::Interpretation { .. }) | None => {
            return Ok(None);
        }
    };
    let other_version: i64 = db.query_row(
        "SELECT max(version) FROM interpretations WHERE receipt=?1",
        [key(&first(&other)?.receipt)?],
        |row| row.get(0),
    )?;
    let snapshot = snapshot(id, version, other_version, index, &held, &other)?;
    Ok(Some(Comparison {
        view: MeasurementReview {
            held,
            other,
            suggestion,
            correctable,
            snapshot,
        },
        version,
        index,
        evidence,
        metadata,
    }))
}

fn snapshot(
    id: i64,
    version: i64,
    other_version: i64,
    index: usize,
    held: &[ResultContent],
    other: &[ResultContent],
) -> Result<String, Error> {
    let evidence = crate::document(&(id, version, other_version, index, held, other))?;
    Ok(super::digest(evidence.as_bytes())
        .0
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join(""))
}

fn unchanged_result(
    db: &Connection,
    receipt: &str,
    parked_version: i64,
    index: usize,
    latest: &Interpretation,
    results: &[ResultContent],
) -> Result<bool, Error> {
    let original: String = db.query_row(
        "SELECT evidence FROM interpretations WHERE receipt=?1 AND version=?2",
        params![receipt, parked_version],
        |row| row.get(0),
    )?;
    let original: Interpretation = serde_json::from_str(&original)?;
    let Reading::Results(original_results) = original.outcome else {
        return Err(Error::Corruption);
    };
    Ok(original.driver == latest.driver
        && original.config == latest.config
        && original_results.get(index) == results.get(index))
}

/// Writes the new reading and its one delivery for a rerun or a correction.
///
/// A rerun gets a host-assigned run, so it is a new result at revision zero.
/// A correction keeps the machine's identity key, so the reading becomes the
/// next revision of the retained result and the lab app replaces that value.
/// The audit passed in marks either as an operator's decision.
fn write_decided<F>(
    tx: &rusqlite::Transaction<'_>,
    store: &Store,
    mut comparison: Comparison,
    decision: MeasurementDecision,
    audit: &str,
    delivery: F,
) -> Result<(), Error>
where
    F: FnOnce(&ResultContent, &ReceiptMetadata) -> Result<Delivery, Error>,
{
    if super::digest(&comparison.evidence.config) != comparison.metadata.config_digest {
        return Err(Error::ReviewChanged);
    }
    let IdentityStatus::Resolved(prior) = &first(&comparison.view.other)?.identity else {
        return Err(Error::Corruption);
    };
    let mut observation = prior.observation.clone();
    let receipt = first(&comparison.view.held)?.receipt;
    let expected = match decision {
        MeasurementDecision::Rerun => {
            // Source recipes encode length-prefixed fields. This prefix marks a
            // host-assigned identity and cannot collide with a recipe's source run.
            observation.run = SourceText::new(&format!(
                "operator:{}:{}",
                uuid::Uuid::from(receipt),
                comparison.index,
            ))
            .map_err(StorageError::Text)?;
            RevisionOutcome::New(Revision(0))
        }
        MeasurementDecision::Correction => RevisionOutcome::Correction(Revision(
            prior.revision.0.checked_add(1).ok_or(Error::Conflict)?,
        )),
        MeasurementDecision::Resend | MeasurementDecision::Supersede => {
            return Err(Error::Corruption);
        }
    };
    let correction = matches!(decision, MeasurementDecision::Correction);
    let Reading::Results(results) = &mut comparison.evidence.outcome else {
        return Err(Error::Corruption);
    };
    let result = results.get_mut(comparison.index).ok_or(Error::Corruption)?;
    result.identity = IdentityStatus::Resolved(ResultIdentity {
        observation,
        revision: Revision(0),
    });
    let reconciler = Reconciliation {
        retention_key: store.retention_key.as_ref(),
        tx,
        machine: &comparison.metadata.machine,
        limit: store.limits.record_bytes,
        rows: store.limits.page_rows,
        lifecycle: None,
        edit_guard: None,
        // The operator already decided; asking again would only hold it again.
        repeats: super::RepeatPolicy::Automatic,
    };
    let mut batch = super::assignment::RevisionBatch::default();
    reconciler.assign_result(result, receipt, correction, &mut batch)?;
    if batch.outcomes != [expected] || !result.may_deliver_to_production() {
        return Err(Error::ReviewChanged);
    }
    let queued = delivery(result, &comparison.metadata)?;
    let delivery_evidence = Interpretation {
        outcome: Reading::Results(vec![result.clone()]),
        ..comparison.evidence.clone()
    };
    let encoded = crate::document(&comparison.evidence)?;
    if encoded.len() > store.limits.record_bytes {
        return Err(Error::InvalidInterpretationInput);
    }
    let deliveries = vec![queued];
    let required = super::required_capacity(
        &encoded,
        Some(audit),
        &[],
        &deliveries,
        &batch.revisions,
        store.limits.record_bytes,
    )?;
    Store::capacity_on(tx, &store.path, &store.limits, &*store.space, required)?;
    let receipt = key(&receipt)?;
    let version = comparison.version.checked_add(1).ok_or(Error::Conflict)?;
    super::write_history(tx, &receipt, version, &encoded, &batch.revisions)?;
    reconciler.write_deliveries(
        &receipt,
        version,
        &delivery_evidence,
        &batch.outcomes,
        deliveries,
    )
}
