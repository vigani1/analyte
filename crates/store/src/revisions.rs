//! Transactional observation history (ADR 0003).
use crate::{Delivery, Error, Interpretation, Reading, Store, digest, key};
use analyte_model::{IdentityStatus, ReceiptId, ResultContent, ResultStatus, Revision};
use rusqlite::{OptionalExtension, params};

/// Store decision for one result, in input order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevisionOutcome {
    /// First observed content under this identity.
    New(Revision),
    /// Already retained content; never enqueue another delivery.
    Retransmission(Revision),
    /// Changed content with explicit correction or withdrawal evidence.
    Correction(Revision),
    /// Changed content without correction evidence; parked.
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
        mut evidence: Interpretation,
        corrections: &[bool],
        findings: &[crate::Parked],
        build: F,
    ) -> Result<InterpretationCommit, Error>
    where
        F: FnOnce(&Interpretation, &[RevisionOutcome]) -> Result<Vec<Delivery>, Error>,
    {
        let stored = self.receipt(receipt)?.ok_or(Error::Conflict)?;
        if evidence.config != stored.config
            || evidence.driver != stored.metadata.driver
            || evidence.config_version != stored.metadata.config_version
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
            .map(|finding| self.encode(finding))
            .collect::<Result<Vec<_>, _>>()?;
        let limit = self.limits.record_bytes;
        let receipt_key = key(&receipt)?;
        let tx = self.db.transaction()?;
        let version: i64 = tx.query_row(
            "SELECT coalesce(max(version),0)+1 FROM interpretations WHERE receipt=?1",
            [&receipt_key],
            |r| r.get(0),
        )?;
        let mut outcomes = Vec::new();
        let mut revisions: Vec<(String, i64, analyte_model::Digest)> = Vec::new();
        if let Reading::Results(results) = &mut evidence.outcome {
            for (result, correction) in results.iter_mut().zip(corrections) {
                if result.receipt != receipt {
                    return Err(Error::InvalidInput);
                }
                let content = clinical_digest(result)?;
                let IdentityStatus::Resolved(identity) = &mut result.identity else {
                    outcomes.push(RevisionOutcome::Unresolved);
                    continue;
                };
                let identity_key = key(&(&stored.metadata.machine, &identity.observation))?;
                // A delayed retransmission may refer to any retained revision, not just the latest.
                let same: Option<i64> = tx.query_row("SELECT revision FROM observation_revisions WHERE identity_key=?1 AND content_digest=?2 ORDER BY revision DESC LIMIT 1", params![identity_key, content.0.as_slice()], |r| r.get(0)).optional()?;
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
                let outcome = match previous {
                    None => RevisionOutcome::New(Revision(0)),
                    Some(previous) if *correction || withdrawn => {
                        RevisionOutcome::Correction(Revision(
                            u64::try_from(previous.checked_add(1).ok_or(Error::Conflict)?)
                                .map_err(|_| Error::Corruption)?,
                        ))
                    }
                    Some(_) => RevisionOutcome::Conflict,
                };
                match outcome {
                    RevisionOutcome::New(revision) | RevisionOutcome::Correction(revision) => {
                        identity.revision = revision;
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
        let mut required = encoded.len();
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
                Reading::Results(results) => results.iter().zip(&outcomes).any(|(r,o)| r.may_deliver_to_production() && matches!(o, RevisionOutcome::New(_) | RevisionOutcome::Correction(_)) && matches!(&r.identity, IdentityStatus::Resolved(id) if id.observation == delivery.ordering_key.observation)),
                Reading::Failed(_) => false,
            };
            if !eligible
                || delivery.ordering_key.machine != stored.metadata.machine
                || delivery.payload.len() > limit
            {
                return Err(Error::InvalidInput);
            }
            tx.execute("INSERT INTO outbox(id,receipt,version,destination,ordering_key,payload,digest,contract,state,next_attempt,enqueued_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,'pending',0,unixepoch())", params![key(&delivery.id)?,receipt_key,version,delivery.destination.as_str(),key(&delivery.ordering_key)?,delivery.payload,digest(&delivery.payload).0.as_slice(),delivery.contract_version])?;
        }
        match &evidence.outcome {
            Reading::Failed(reason) => {
                tx.execute(
                    "INSERT INTO parked(evidence) VALUES (?1)",
                    [key(&("interpretation_failure", &receipt_key, reason))?],
                )?;
            }
            Reading::Results(results) => {
                for (index, (result, outcome)) in results.iter().zip(&outcomes).enumerate() {
                    if result.requires_parking()
                        || matches!(
                            outcome,
                            RevisionOutcome::Conflict | RevisionOutcome::Unresolved
                        )
                    {
                        tx.execute(
                            "INSERT INTO parked(evidence) VALUES (?1)",
                            [key(&(
                                "result_reconciliation",
                                &receipt_key,
                                index,
                                format!("{outcome:?}"),
                            ))?],
                        )?;
                    }
                }
            }
        }
        tx.commit()?;
        Ok(InterpretationCommit { version, outcomes })
    }
}

impl Store {
    pub(super) fn migrate_v3(db: &mut rusqlite::Connection, limit: usize) -> Result<(), Error> {
        let tx = db.transaction()?;
        tx.execute_batch(include_str!("migration_v3.sql"))?;
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
