//! Transactional observation history (ADR 0003).
mod status;
use crate::{Delivery, Error, Interpretation, Reading, Store, digest, key};
use analyte_model::{IdentityStatus, ReceiptId, ResultContent, ResultStatus, Revision};
use rusqlite::{OptionalExtension, params};
pub use status::StatusLifecycle;

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
    /// Append-only interpretation version within the receipt.
    pub version: i64,
    /// One decision per input result.
    pub outcomes: Vec<RevisionOutcome>,
}

fn clinical_digest(result: &ResultContent) -> Result<analyte_model::Digest, Error> {
    let mut parts = serde_json::to_value(&result.parts)?;
    if let Some(parts) = parts.as_array_mut() {
        for part in parts {
            if let Some(fields) = part.as_object_mut() {
                fields.remove("received_at");
                // Source offsets locate immutable evidence, not clinical content.
                // An optional resend field can shift every later XML/record offset.
                // Keep source kinds, sequences and statuses in the comparison.
                if let Some(path) = fields
                    .get_mut("source_path")
                    .and_then(serde_json::Value::as_array_mut)
                {
                    for node in path {
                        if let Some(node) = node.as_object_mut() {
                            node.remove("occurrence");
                        }
                    }
                }
            }
        }
    }
    // JSON objects use sorted keys. These are typed structures, not arbitrary source JSON.
    Ok(digest(&serde_json::to_vec(&(
        &result.sample,
        result.processing,
        parts,
    ))?))
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
            (corrections, findings, None),
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
            return Err(Error::InvalidInput);
        }
        self.reconcile(
            action.receipt,
            evidence,
            (corrections, findings, None),
            Some(action),
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
            (mapping.0, mapping.1, Some(lifecycle)),
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
            return Err(Error::InvalidInput);
        }
        self.reconcile(
            action.receipt,
            evidence,
            (mapping.0, mapping.1, Some(lifecycle)),
            Some(action),
            build,
        )
    }

    fn reconcile<F>(
        &mut self,
        receipt: ReceiptId,
        mut evidence: Interpretation,
        mapping: (&[bool], &[crate::Parked], Option<StatusLifecycle<'_>>),
        action: Option<&crate::Reprocess>,
        build: F,
    ) -> Result<InterpretationCommit, Error>
    where
        F: FnOnce(&Interpretation, &[RevisionOutcome]) -> Result<Vec<Delivery>, Error>,
    {
        let (corrections, findings, lifecycle) = mapping;
        let stored = self.receipt(receipt)?.ok_or(Error::UnknownReceipt)?;
        if action.is_none()
            && (evidence.config != stored.config
                || evidence.driver != stored.metadata.driver
                || evidence.config_version != stored.metadata.config_version)
        {
            return Err(Error::InvalidInput);
        }
        let count = match &evidence.outcome {
            Reading::Results(r) => r.len(),
            Reading::Failed(_) => 0,
        };
        if count != corrections.len() || count > self.limits.page_rows {
            return Err(Error::InvalidInput);
        }
        self.encode(&evidence)?;
        if findings.len() > self.limits.page_rows {
            return Err(Error::InvalidInput);
        }
        let findings = findings
            .iter()
            .map(|finding| {
                self.encode(&crate::parked::ParkingEvidence {
                    details: finding.clone(),
                    context: None,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
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
        let mut outcomes = Vec::new();
        let mut revisions: Vec<(String, i64, analyte_model::Digest)> = Vec::new();
        let mut staged = Vec::<(String, ResultContent)>::new();
        if let Reading::Results(results) = &mut evidence.outcome {
            for (result, correction) in results.iter_mut().zip(corrections) {
                if result.receipt != receipt {
                    return Err(Error::InvalidInput);
                }
                let candidate = result.clone();
                let content = clinical_digest(result)?;
                let IdentityStatus::Resolved(identity) = &mut result.identity else {
                    outcomes.push(RevisionOutcome::Unresolved);
                    continue;
                };
                let identity_key = key(&(&stored.metadata.machine, &identity.observation))?;
                if let Some(policy) = lifecycle
                    && status::other_final_run(
                        &tx,
                        &stored.metadata.machine,
                        &candidate,
                        &staged,
                        policy,
                        self.limits.page_rows,
                        limit,
                    )?
                {
                    result.identity = IdentityStatus::Unresolved(
                        analyte_model::Reason::new(
                            "multiple final runs under the same order and test",
                        )
                        .map_err(|_| Error::InvalidInput)?,
                    );
                    outcomes.push(RevisionOutcome::Conflict);
                    continue;
                }
                // A delayed retransmission may refer to any retained revision, not just the latest.
                let same: Option<i64> = tx.query_row("SELECT revision FROM observation_revisions WHERE identity_key=?1 AND content_digest=?2 ORDER BY revision DESC LIMIT 1", params![identity_key, content.0.as_slice()], |r| r.get(0)).optional()?;
                // Older digests included source offsets. Compare retained typed evidence
                // on a miss; do not rewrite history or silently treat old results as new.
                let same = match same {
                    Some(revision) => Some(revision),
                    None => same_retained_content(
                        &tx,
                        &identity_key,
                        &identity.observation,
                        &content,
                        self.limits.page_rows,
                        limit,
                    )?,
                };
                let same = revisions
                    .iter()
                    .rev()
                    .find(|(key, _, digest)| key == &identity_key && digest == &content)
                    .map(|(_, revision, _)| *revision)
                    .or(same);
                if let Some(revision) = same {
                    identity.revision =
                        Revision(u64::try_from(revision).map_err(|_| Error::Corruption)?);
                    outcomes.push(RevisionOutcome::Retransmission(identity.revision));
                    continue;
                }
                let previous: Option<i64> = tx.query_row(
                    "SELECT max(revision) FROM observation_revisions WHERE identity_key=?1",
                    [&identity_key],
                    |r| r.get(0),
                )?;
                let previous = revisions
                    .iter()
                    .rev()
                    .find(|(key, _, _)| key == &identity_key)
                    .map(|(_, revision, _)| *revision)
                    .or(previous);
                let withdrawn = result
                    .parts
                    .as_slice()
                    .iter()
                    .any(|p| p.status == ResultStatus::Withdrawn);
                let progression = if let Some(policy) = lifecycle.filter(|_| previous.is_some()) {
                    let prior = staged
                        .iter()
                        .rev()
                        .find(|(key, _)| key == &identity_key)
                        .map(|(_, result)| result.clone())
                        .map(Ok)
                        .unwrap_or_else(|| {
                            status::latest(&tx, &identity_key, limit)
                                .and_then(|result| result.ok_or(Error::Corruption))
                        })?;
                    policy.permits(&prior, &candidate)
                } else {
                    lifecycle.is_none() && (*correction || withdrawn)
                };
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
                        staged.push((identity_key.clone(), result.clone()));
                        revisions.push((
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
                            .map_err(|_| Error::InvalidInput)?,
                        );
                    }
                    RevisionOutcome::Retransmission(_) | RevisionOutcome::Unresolved => {
                        return Err(Error::InvalidInput);
                    }
                }
                outcomes.push(outcome);
            }
        }
        let deliveries = build(&evidence, &outcomes)?;
        if deliveries.len() > count {
            return Err(Error::InvalidInput);
        }
        let encoded = key(&evidence)?;
        if encoded.len() > limit {
            return Err(Error::InvalidInput);
        }
        let successful = findings.is_empty()
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
            };
        let audit = action
            .map(|action| crate::reprocess::audit_evidence(action, version, successful))
            .transpose()?;
        let mut required = encoded
            .len()
            .checked_add(
                audit
                    .as_ref()
                    .map_or(0, String::len)
                    .checked_mul(2)
                    .ok_or(Error::Capacity)?,
            )
            .ok_or(Error::Capacity)?;
        if audit.as_ref().is_some_and(|value| value.len() > limit) {
            return Err(Error::InvalidInput);
        }
        for finding in &findings {
            required = required.checked_add(finding.len()).ok_or(Error::Capacity)?;
        }
        for delivery in &deliveries {
            required = required
                .checked_add(delivery.payload.len())
                .ok_or(Error::Capacity)?;
        }
        for (identity, _, _) in &revisions {
            required = required
                .checked_add(identity.len())
                .and_then(|n| n.checked_add(128))
                .ok_or(Error::Capacity)?;
        }
        Self::capacity_on(&tx, &self.path, &self.limits, &*self.space, required)?;
        let findings_empty = findings.is_empty();
        for finding in findings {
            tx.execute("INSERT INTO parked(evidence) VALUES (?1)", [finding])?;
        }
        tx.execute(
            "INSERT INTO interpretations VALUES (?1,?2,?3)",
            params![receipt_key, version, encoded],
        )?;
        for (identity, revision, content) in revisions {
            tx.execute(
                "INSERT INTO observation_revisions VALUES (?1,?2,?3,?4,?5)",
                params![
                    identity,
                    revision,
                    receipt_key,
                    version,
                    content.0.as_slice()
                ],
            )?;
        }
        for delivery in deliveries {
            let eligible = match &evidence.outcome {
                Reading::Results(results) => results.iter().zip(&outcomes).any(|(r,o)| r.may_deliver_to_production() && lifecycle.is_none_or(|policy|policy.may_deliver(r)) && matches!(o, RevisionOutcome::New(_) | RevisionOutcome::Correction(_)) && matches!(&r.identity, IdentityStatus::Resolved(id) if id.observation == delivery.ordering_key.observation)),
                Reading::Failed(_) => false,
            };
            if !eligible
                || delivery.ordering_key.machine != stored.metadata.machine
                || delivery.payload.len() > limit
            {
                return Err(Error::InvalidInput);
            }
            tx.execute("INSERT INTO outbox(id,receipt,version,destination,ordering_key,payload,digest,contract,state,next_attempt,enqueued_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,0,unixepoch())", params![key(&delivery.id)?,receipt_key,version,delivery.destination.as_str(),key(&delivery.ordering_key)?,delivery.payload,digest(&delivery.payload).0.as_slice(),delivery.contract_version,crate::DeliveryState::Pending])?;
        }
        match &evidence.outcome {
            Reading::Failed(reason) => {
                tx.execute(
                    "INSERT INTO parked(evidence) VALUES (?1)",
                    [crate::parked::evidence(
                        "interpretation_failure",
                        &serde_json::from_str::<String>(&receipt_key)?,
                        reason.as_str(),
                        Some(crate::parked::ParkingContext::Interpretation { version }),
                    )?],
                )?;
            }
            Reading::Results(results) => {
                if action.is_some() && results.is_empty() && findings_empty {
                    tx.execute(
                        "INSERT INTO parked(evidence) VALUES (?1)",
                        [crate::parked::evidence(
                            "interpretation_failure",
                            &serde_json::from_str::<String>(&receipt_key)?,
                            "reprocessing produced no results",
                            Some(crate::parked::ParkingContext::Interpretation { version }),
                        )?],
                    )?;
                }
                for (index, (result, outcome)) in results.iter().zip(&outcomes).enumerate() {
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
                                &serde_json::from_str::<String>(&receipt_key)?,
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
        if let (Some(action), Some(audit)) = (action, audit) {
            if successful && let Some(parked) = action.parked {
                tx.execute(
                    "UPDATE parked SET resolution=?2 WHERE id=?1 AND resolution IS NULL",
                    params![parked, &audit],
                )?;
            }
            tx.execute("INSERT INTO audit(evidence) VALUES (?1)", [audit])?;
        }
        tx.commit()?;
        Ok(InterpretationCommit { version, outcomes })
    }
}

impl Store {
    pub(crate) fn migrate_v3(db: &mut rusqlite::Connection, limit: usize) -> Result<(), Error> {
        let tx = db.transaction()?;
        tx.execute_batch(include_str!("../upkeep/migration_v3.sql"))?;
        let mut after = 0_i64;
        loop {
            let mut statement = tx.prepare("SELECT i.rowid,i.receipt,i.version,i.evidence,r.metadata FROM interpretations i JOIN receipts r ON r.id=i.receipt WHERE i.rowid>?1 ORDER BY i.rowid LIMIT 64")?;
            let rows = statement
                .query_map([after], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            if rows.is_empty() {
                break;
            }
            for (rowid, receipt, version, evidence, metadata) in rows {
                if evidence.len() > limit || metadata.len() > limit {
                    return Err(Error::InvalidInput);
                }
                let evidence: Interpretation = serde_json::from_str(&evidence)?;
                let metadata: analyte_model::ReceiptMetadata = serde_json::from_str(&metadata)?;
                if let Reading::Results(results) = evidence.outcome {
                    for result in results {
                        if let IdentityStatus::Resolved(identity) = &result.identity {
                            let identity_key = key(&(&metadata.machine, &identity.observation))?;
                            let revision = i64::try_from(identity.revision.0)
                                .map_err(|_| Error::Corruption)?;
                            let digest = clinical_digest(&result)?;
                            let previous: Option<Vec<u8>> = tx.query_row("SELECT content_digest FROM observation_revisions WHERE identity_key=?1 AND revision=?2", params![identity_key,revision], |r| r.get(0)).optional()?;
                            match previous {
                                Some(previous) if previous != digest.0 => {
                                    return Err(Error::Conflict);
                                }
                                Some(_) => {}
                                None => {
                                    tx.execute(
                                        "INSERT INTO observation_revisions VALUES (?1,?2,?3,?4,?5)",
                                        params![
                                            identity_key,
                                            revision,
                                            receipt,
                                            version,
                                            digest.0.as_slice()
                                        ],
                                    )?;
                                }
                            }
                        }
                    }
                }
                after = rowid;
            }
        }
        tx.commit()?;
        Ok(())
    }
}

fn same_retained_content(
    tx: &rusqlite::Transaction<'_>,
    identity_key: &str,
    observation: &analyte_model::ObservationIdentity,
    content: &analyte_model::Digest,
    maximum_rows: usize,
    maximum_bytes: usize,
) -> Result<Option<i64>, Error> {
    let bound = maximum_rows
        .checked_add(1)
        .ok_or(Error::RevisionHistoryLimit)?;
    let mut statement = tx.prepare("SELECT r.revision,i.evidence FROM observation_revisions r JOIN interpretations i ON i.receipt=r.receipt AND i.version=r.interpretation_version WHERE r.identity_key=?1 ORDER BY r.revision DESC LIMIT ?2")?;
    let mut rows = statement.query(params![
        identity_key,
        i64::try_from(bound).map_err(|_| Error::RevisionHistoryLimit)?
    ])?;
    let mut count = 0usize;
    while let Some(row) = rows.next()? {
        if count >= maximum_rows {
            return Err(Error::RevisionHistoryLimit);
        }
        count = count.checked_add(1).ok_or(Error::RevisionHistoryLimit)?;
        let revision: i64 = row.get(0)?;
        let encoded: String = row.get(1)?;
        if encoded.len() > maximum_bytes {
            return Err(Error::Corruption);
        }
        let evidence: Interpretation = serde_json::from_str(&encoded)?;
        let Reading::Results(results) = evidence.outcome else {
            return Err(Error::Corruption);
        };
        let retained = results.iter().find(|result| matches!(&result.identity, IdentityStatus::Resolved(identity) if &identity.observation == observation && i64::try_from(identity.revision.0).ok() == Some(revision))).ok_or(Error::Corruption)?;
        if clinical_digest(retained)? == *content {
            return Ok(Some(revision));
        }
    }
    Ok(None)
}
