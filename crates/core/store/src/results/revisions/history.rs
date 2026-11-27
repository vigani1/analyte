//! Reads and writes revision history within the caller transaction.
use super::{
    AssignedRevision, Error, IdentityStatus, Interpretation, Reading, Reconciliation,
    clinical_digest, params,
};
use rusqlite::OptionalExtension as _;
pub(super) fn same_retained_content(
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
        let retained = results
            .iter()
            .find(|result| {
                matches!(&result.identity,
            IdentityStatus::Resolved(identity) if &identity.observation == observation
                && i64::try_from(identity.revision.0).ok() == Some(revision))
            })
            .ok_or(Error::Corruption)?;
        if clinical_digest(retained)? == *content {
            return Ok(Some(revision));
        }
    }
    Ok(None)
}

impl Reconciliation<'_, '_> {
    pub(super) fn same_revision(
        &self,
        identity_key: &str,
        observation: &analyte_model::ObservationIdentity,
        content: &analyte_model::Digest,
        revisions: &[AssignedRevision],
    ) -> Result<Option<i64>, Error> {
        if let Some(key) = self.retention_key {
            let identity = key.digest("identity", identity_key.as_bytes())?;
            let content = key.digest("content", &content.0)?;
            let purged: Option<i64> = self.tx.query_row("SELECT revision FROM retention_results WHERE identity_digest=?1 AND content_digest=?2 ORDER BY revision DESC LIMIT 1",
                params![identity.as_slice(),
                content.as_slice()],
                |r| r.get(0)).optional()?;
            if purged.is_some() {
                return Ok(purged);
            }
        }
        // A delayed retransmission may refer to any retained revision, not just the latest.
        let same: Option<i64> = self.tx.query_row("SELECT revision FROM observation_revisions WHERE identity_key=?1 AND content_digest=?2 ORDER BY revision DESC LIMIT 1",
            params![identity_key,
            content.0.as_slice()],
            |r| r.get(0)).optional()?;
        // Older digests included source offsets. Compare retained typed evidence
        // on a miss; do not rewrite history or silently treat old results as new.
        let same = match same {
            Some(revision) => Some(revision),
            None => same_retained_content(
                self.tx,
                identity_key,
                observation,
                content,
                self.rows,
                self.limit,
            )?,
        };
        let same = revisions
            .iter()
            .rev()
            .find(|(key, _, digest)| key == identity_key && digest == content)
            .map(|(_, revision, _)| *revision)
            .or(same);

        Ok(same)
    }
    /// Whether an upload other than `receipt` retained this key, including
    /// one data keeping has since purged; this receipt's own earlier readings
    /// are not a repeat.
    pub(super) fn retained_by_other_upload(
        &self,
        identity_key: &str,
        receipt: analyte_model::ReceiptId,
    ) -> Result<bool, Error> {
        let receipt = crate::key(&receipt)?;
        let retained: bool = self.tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM observation_revisions WHERE identity_key=?1 AND receipt<>?2)",
            params![identity_key, receipt],
            |r| r.get(0),
        )?;
        if retained {
            return Ok(true);
        }
        let Some(key) = self.retention_key else {
            return Ok(false);
        };
        let identity = key.digest("identity", identity_key.as_bytes())?;
        Ok(self.tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM retention_results WHERE identity_digest=?1)",
            [identity.as_slice()],
            |r| r.get(0),
        )?)
    }
    pub(super) fn previous_revision(
        &self,
        identity_key: &str,
        revisions: &[AssignedRevision],
    ) -> Result<Option<i64>, Error> {
        let previous: Option<i64> = self.tx.query_row(
            "SELECT max(revision) FROM observation_revisions WHERE identity_key=?1",
            [&identity_key],
            |r| r.get(0),
        )?;
        let purged = if let Some(key) = self.retention_key {
            let identity = key.digest("identity", identity_key.as_bytes())?;
            self.tx.query_row(
                "SELECT max(revision) FROM retention_results WHERE identity_digest=?1",
                [identity.as_slice()],
                |r| r.get::<_, Option<i64>>(0),
            )?
        } else {
            None
        };
        let previous = previous.into_iter().chain(purged).max();
        let previous = revisions
            .iter()
            .rev()
            .find(|(key, _, _)| key == identity_key)
            .map(|(_, revision, _)| *revision)
            .or(previous);

        Ok(previous)
    }
}

pub(super) fn write_history(
    tx: &rusqlite::Transaction<'_>,
    receipt_key: &str,
    version: i64,
    encoded: &str,
    revisions: &[AssignedRevision],
) -> Result<(), Error> {
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

    Ok(())
}
