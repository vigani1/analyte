//! Replaces eligible protected receipt content with nonclinical tombstones inside its purge transaction.
use super::DigestKey;
use crate::{Error, Interpretation, Reading, key};
use analyte_model::{IdentityStatus, ReceiptId, ReceiptMetadata};
use rusqlite::{Connection, params};

pub(super) fn remove_delete_guards(db: &Connection) -> Result<Vec<String>, Error> {
    let names = [
        "immutable_receipts_delete",
        "immutable_frames_delete",
        "immutable_interpretations_delete",
        "immutable_attempts_delete",
        "observation_revisions_immutable_delete",
        "frame_provenance_immutable_delete",
        "outbox_retained",
    ];
    let mut guards = Vec::new();
    for name in names {
        let sql: String = db.query_row(
            "SELECT sql FROM sqlite_master WHERE type='trigger' AND name=?1",
            [name],
            |r| r.get(0),
        )?;
        guards.push(sql);
        db.execute_batch(&format!("DROP TRIGGER {name}"))?;
    }
    Ok(guards)
}
pub(super) fn purge(
    db: &Connection,
    credential: &DigestKey,
    receipt: ReceiptId,
    now: i64,
    limit: usize,
) -> Result<(usize, usize), Error> {
    let receipt_key = key(&receipt)?;
    let metadata: String = db.query_row("SELECT CASE WHEN length(CAST(metadata AS BLOB))<=?2 THEN metadata END FROM receipts WHERE id=?1", params![receipt_key,i64::try_from(limit).map_err(|_|Error::InvalidMaintenanceInput)?], |r|r.get(0))?;
    let metadata: ReceiptMetadata = serde_json::from_str(&metadata)?;
    let acknowledged: i64 = db.query_row("SELECT max(a.time) FROM attempts a JOIN outbox o ON o.id=a.delivery WHERE o.receipt=?1 AND a.outcome='\"Done\"'", [&receipt_key], |r|r.get(0))?;
    db.execute(
        "INSERT INTO retention_key VALUES (1,?1) ON CONFLICT(singleton) DO NOTHING",
        [credential.fingerprint().as_slice()],
    )?;
    db.execute(
        "INSERT INTO retention_tombstones VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
        params![
            receipt_key,
            metadata.machine.as_str(),
            key(&metadata.driver)?,
            metadata.config_version.as_str(),
            metadata.raw_digest.0.as_slice(),
            metadata.received_at.as_second(),
            acknowledged,
            now
        ],
    )?;
    db.execute("INSERT INTO retention_deliveries SELECT o.id,o.receipt,max(a.time) FROM outbox o JOIN attempts a ON a.delivery=o.id WHERE o.receipt=?1 AND a.outcome='\"Done\"' GROUP BY o.id", [&receipt_key])?;
    retain_results(db, credential, &receipt_key, &metadata, limit)?;
    let deliveries: i64 = db.query_row(
        "SELECT count(*) FROM retention_deliveries WHERE receipt=?1",
        [&receipt_key],
        |r| r.get(0),
    )?;
    let results: i64 = db.query_row(
        "SELECT count(*) FROM retention_results WHERE receipt=?1",
        [&receipt_key],
        |r| r.get(0),
    )?;
    for sql in [
        "DELETE FROM attempts WHERE delivery IN (SELECT id FROM outbox WHERE receipt=?1)",
        "DELETE FROM outbox WHERE receipt=?1",
        "DELETE FROM observation_revisions WHERE receipt=?1",
        "DELETE FROM interpretations WHERE receipt=?1",
        "DELETE FROM frames WHERE transfer IN (SELECT id FROM transfers WHERE receipt=?1)",
        "DELETE FROM frame_provenance WHERE transfer IN (SELECT id FROM transfers WHERE receipt=?1)",
        "DELETE FROM transfers WHERE receipt=?1",
        "DELETE FROM receipts WHERE id=?1",
    ] {
        db.execute(sql, [&receipt_key])?;
    }
    Ok((
        usize::try_from(deliveries).map_err(|_| Error::Corruption)?,
        usize::try_from(results).map_err(|_| Error::Corruption)?,
    ))
}
fn retain_results(
    db: &Connection,
    credential: &DigestKey,
    receipt: &str,
    metadata: &ReceiptMetadata,
    limit: usize,
) -> Result<(), Error> {
    let mut statement = db.prepare("SELECT CASE WHEN length(CAST(evidence AS BLOB))<=?2 THEN evidence END FROM interpretations WHERE receipt=?1 ORDER BY version")?;
    let mut rows = statement.query(params![
        receipt,
        i64::try_from(limit).map_err(|_| Error::InvalidMaintenanceInput)?
    ])?;
    while let Some(row) = rows.next()? {
        let encoded: String = row.get(0)?;
        let evidence: Interpretation = serde_json::from_str(&encoded)?;
        let Reading::Results(results) = evidence.outcome else {
            return Err(Error::Corruption);
        };
        for result in results {
            let IdentityStatus::Resolved(identity) = &result.identity else {
                return Err(Error::Corruption);
            };
            let observation = &identity.observation;
            let identity_digest = credential.digest(
                "identity",
                key(&(&metadata.machine, observation))?.as_bytes(),
            )?;
            let order = credential.digest("order", key(&observation.order)?.as_bytes())?;
            let sample = credential.digest(
                "sample",
                key(&result.sample.as_ref().map(|sample| &sample.id))?.as_bytes(),
            )?;
            let run = credential.digest("run", key(&observation.run)?.as_bytes())?;
            let content = crate::results::revisions::content::clinical_digest(&result)?;
            let content = credential.digest("content", &content.0)?;
            db.execute(
                "INSERT INTO retention_results VALUES (?1,?2,?3,?4,?5,?6,?7)",
                params![
                    identity_digest.as_slice(),
                    i64::try_from(identity.revision.0).map_err(|_| Error::Corruption)?,
                    receipt,
                    order.as_slice(),
                    sample.as_slice(),
                    run.as_slice(),
                    content.as_slice()
                ],
            )?;
        }
    }
    Ok(())
}
