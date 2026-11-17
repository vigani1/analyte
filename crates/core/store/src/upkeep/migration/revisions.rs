//! Migrates legacy revision history in a separate startup transaction.
use crate::results::revisions::content::clinical_digest;
use crate::{Interpretation, MaintenanceError as Error, Reading, Store, key};
use analyte_model::IdentityStatus;
use rusqlite::{OptionalExtension, params};
impl Store {
    pub(crate) fn migrate_v3(db: &mut rusqlite::Connection, limit: usize) -> Result<(), Error> {
        let tx = db.transaction()?;
        tx.execute_batch(include_str!("../migration_v3.sql"))?;
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
                    return Err(Error::Storage(crate::StorageError::LimitExceeded {
                        what: "record_bytes",
                    }));
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
        Self::integrity(&tx)?;
        tx.commit()?;
        Ok(())
    }
}
