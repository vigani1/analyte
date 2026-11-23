//! Finds a unique later receipt covering the missing tube ID.
use super::{
    Error, Reading, ReceiptMetadata, ResultContent, clinical_digest, first, latest, params,
};
use rusqlite::Connection;

pub(super) fn suggested(
    db: &Connection,
    receipt: &str,
    metadata: &ReceiptMetadata,
    held: &[ResultContent],
    rows: usize,
) -> Result<Option<Vec<ResultContent>>, Error> {
    // A candidate needs a machine-supplied measurement time and identical
    // retained result content apart from the missing sample and receipt offsets.
    if held
        .iter()
        .flat_map(|result| result.parts.as_slice())
        .any(|p| {
            !p.times.as_slice().iter().any(|time| {
                matches!(time.role.as_str(), "R.12" | "R.13" | "OBX.14" | "OBX.19")
                    && matches!(
                        time.time.precision,
                        analyte_model::TimePrecision::Second
                            | analyte_model::TimePrecision::FractionalSecond
                    )
            })
        })
    {
        return Ok(None);
    }
    let mut statement = db.prepare(
        "SELECT r.id,r.metadata FROM receipts r WHERE r.rowid>(SELECT rowid FROM receipts WHERE id=?1)
         ORDER BY r.rowid LIMIT ?2",
    )?;
    let mut receipts = statement.query(params![
        receipt,
        i64::try_from(rows.saturating_add(1)).map_err(|_| Error::InvalidInterpretationInput)?
    ])?;
    let mut found = None;
    for count in 0..=rows {
        let Some(row) = receipts.next()? else { break };
        if count == rows {
            return Ok(None);
        }
        let candidate: String = row.get(0)?;
        let candidate_metadata: ReceiptMetadata = serde_json::from_str(&row.get::<_, String>(1)?)?;
        if candidate_metadata.machine != metadata.machine
            || candidate_metadata.config_digest != metadata.config_digest
        {
            continue;
        }
        let Some((version, evidence)) = latest(db, &candidate)? else {
            continue;
        };
        let Reading::Results(results) = evidence.outcome else {
            continue;
        };
        if !matches_missing_sample(held, &results)? {
            continue;
        }
        let deliveries: i64 = db.query_row(
            "SELECT count(*) FROM outbox WHERE receipt=?1 AND version=?2",
            params![candidate, version],
            |r| r.get(0),
        )?;
        if usize::try_from(deliveries).ok() == Some(results.len()) {
            if found.is_some() {
                return Ok(None);
            }
            found = Some(results);
        }
    }
    Ok(found)
}

fn matches_missing_sample(held: &[ResultContent], later: &[ResultContent]) -> Result<bool, Error> {
    if held.len() != later.len() {
        return Ok(false);
    }
    let sample = &first(later)?.sample;
    for (before, after) in held.iter().zip(later) {
        if !after.may_deliver_to_production() || after.sample != *sample {
            return Ok(false);
        }
        if measurement_digest(before)? != measurement_digest(after)? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn measurement_digest(result: &ResultContent) -> Result<analyte_model::Digest, Error> {
    use analyte_model::{EvidenceList, NonEmptyEvidenceList};
    let mut result = result.clone();
    result.sample = None;
    let mut parts = result.parts.as_slice().to_vec();
    // The analyzer may stamp the enclosing transmission again when it resends.
    for part in &mut parts {
        part.times = EvidenceList::new(
            part.times
                .as_slice()
                .iter()
                .filter(|time| !matches!(time.role.as_str(), "H.14" | "MSH.7"))
                .cloned()
                .collect(),
        )
        .map_err(|_| Error::Corruption)?;
    }
    result.parts =
        NonEmptyEvidenceList::new(EvidenceList::new(parts).map_err(|_| Error::Corruption)?)
            .map_err(|_| Error::Corruption)?;
    Ok(clinical_digest(&result)?)
}
