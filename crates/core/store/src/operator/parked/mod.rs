//! Parks failed processing outcomes with retained reasons, and reads them back for Needs review.
use crate::{Audit, OperatorError as Error, Parked, Store};
use analyte_model::{Identifier, Reason};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
#[cfg(test)]
mod tests;

#[derive(Debug, Serialize, Deserialize)]
pub(crate) enum ParkingContext {
    Interpretation { version: i64 },
    Result { version: Option<i64>, index: usize },
}

#[derive(Serialize, Deserialize)]
pub(crate) struct ParkingEvidence {
    #[serde(flatten)]
    pub(crate) details: Parked,
    pub(crate) context: Option<ParkingContext>,
}

pub(crate) fn evidence(
    kind: &str,
    target: &str,
    reason: &str,
    context: Option<ParkingContext>,
) -> Result<String, crate::StorageError> {
    let details = Parked {
        kind: Identifier::new(kind).map_err(crate::StorageError::Text)?,
        reason: Reason::new(reason).map_err(crate::StorageError::Text)?,
        targets: vec![Identifier::new(target).map_err(crate::StorageError::Text)?],
    };
    crate::document(&ParkingEvidence { details, context })
}

/// Protected operator evidence, intended only for authenticated local views.
pub struct ParkedItem {
    /// Stable local identity used for resolution.
    pub id: i64,
    /// Canonical protected reason, links and context as stored JSON.
    pub evidence: String,
    /// Actor, time and method of resolution, absent while open.
    pub resolution: Option<String>,
}

/// Which parked items a page lists: those waiting for review or those resolved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParkedFilter {
    /// Not resolved yet.
    Open,
    /// Resolved, with their resolution kept.
    Resolved,
}

/// How many parked items are open and resolved; every parked item is one of the two.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct ParkedCounts {
    /// Items waiting for review.
    pub open: u64,
    /// Items resolved, kept with their resolution.
    pub resolved: u64,
}

impl ParkedItem {
    /// Decodes the canonical protected parking record.
    /// # Errors
    /// Refuses corrupt evidence instead of inventing receipt links.
    pub fn details(&self) -> Result<Parked, serde_json::Error> {
        Ok(serde_json::from_str::<ParkingEvidence>(&self.evidence)?.details)
    }
}

impl Store {
    /// Appends operator-visible evidence without a deletion API.
    /// # Errors
    /// Refuses oversized evidence or database failures.
    pub fn park(&mut self, item: &Parked) -> Result<i64, Error> {
        let encoded = self.encode(&ParkingEvidence {
            details: item.clone(),
            context: None,
        })?;
        self.bound(encoded.as_bytes())?;
        self.db
            .execute("INSERT INTO parked(evidence) VALUES (?1)", [encoded])?;
        Ok(self.db.last_insert_rowid())
    }

    /// Resolves once, recording who, when and how in both the item and audit trail.
    /// # Errors
    /// Refuses missing or already resolved items.
    pub fn resolve(&mut self, id: i64, resolution: &Audit) -> Result<(), Error> {
        let encoded = self.encode(resolution)?;
        self.bound(encoded.as_bytes())?;
        let tx = self.db.transaction()?;
        let required = crate::polls::resolution_bytes(
            &tx,
            id,
            encoded.len(),
            resolution.reason.as_str().len(),
        )?;
        Self::capacity_on(&tx, &self.path, &self.limits, &*self.space, required)?;
        if tx.execute(
            "UPDATE parked SET resolution=?2 WHERE id=?1 AND resolution IS NULL",
            params![id, encoded],
        )? != 1
        {
            return Err(Error::Conflict);
        }
        crate::polls::resolve_on(&tx, id, resolution, &encoded)?;
        tx.execute("INSERT INTO audit(evidence) VALUES (?1)", [encoded])?;
        tx.commit()?;
        Ok(())
    }

    /// Appends an audit entry permanently.
    /// # Errors
    /// Refuses oversized entries or database failures.
    pub fn audit(&mut self, entry: &Audit) -> Result<(), crate::StorageError> {
        let encoded = self.encode(entry)?;
        self.bound(encoded.as_bytes())?;
        self.db
            .execute("INSERT INTO audit(evidence) VALUES (?1)", [encoded])?;
        Ok(())
    }
}

impl Store {
    /// Reads one parked item by its exact identity without scanning another page.
    /// # Errors
    /// Reports database failures; an unknown identity returns absence.
    pub fn parked_item(&self, id: i64) -> Result<Option<ParkedItem>, Error> {
        Ok(self
            .db
            .query_row(
                "SELECT id,evidence,resolution FROM parked WHERE id=?1",
                [id],
                |row| {
                    Ok(ParkedItem {
                        id: row.get(0)?,
                        evidence: row.get(1)?,
                        resolution: row.get(2)?,
                    })
                },
            )
            .optional()?)
    }

    /// Lists a bounded page of parked items, including their resolution evidence.
    /// # Errors
    /// Reports database failures.
    pub fn parked(&self, after: i64) -> Result<Vec<ParkedItem>, Error> {
        let mut stmt = self.db.prepare(
            "SELECT id,evidence,resolution FROM parked WHERE id>?1 ORDER BY id LIMIT ?2",
        )?;
        Ok(stmt
            .query_map(
                params![
                    after,
                    i64::try_from(self.limits.page_rows)
                        .map_err(|_| Error::InvalidOperatorInput)?
                ],
                |r| {
                    Ok(ParkedItem {
                        id: r.get(0)?,
                        evidence: r.get(1)?,
                        resolution: r.get(2)?,
                    })
                },
            )?
            .collect::<Result<_, _>>()?)
    }

    /// How many parked items are open and how many are resolved. Counting changes nothing.
    /// # Errors
    /// Reports database failures or corrupt counts.
    pub fn parked_counts(&self) -> Result<ParkedCounts, Error> {
        let (open, resolved): (i64, i64) = self.db.query_row(
            concat!(
                "SELECT count(*) FILTER (WHERE resolution IS NULL),",
                "count(*) FILTER (WHERE resolution IS NOT NULL) FROM parked",
            ),
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        Ok(ParkedCounts {
            open: u64::try_from(open).map_err(|_| Error::Corruption)?,
            resolved: u64::try_from(resolved).map_err(|_| Error::Corruption)?,
        })
    }

    /// Lists a bounded page of the open or the resolved parked items after `after`,
    /// in item order, like [`Store::parked`].
    /// # Errors
    /// Reports database failures.
    pub fn parked_filtered(
        &self,
        after: i64,
        filter: ParkedFilter,
    ) -> Result<Vec<ParkedItem>, Error> {
        self.parked_filtered_up_to(after, filter, self.limits.page_rows)
    }

    /// Like [`Store::parked_filtered`], with at most `limit` items.
    /// # Errors
    /// Reports database failures.
    pub fn parked_filtered_up_to(
        &self,
        after: i64,
        filter: ParkedFilter,
        limit: usize,
    ) -> Result<Vec<ParkedItem>, Error> {
        let mut stmt = self.db.prepare(
            "SELECT id,evidence,resolution FROM parked WHERE id>?1 AND (resolution IS NULL)=?2 ORDER BY id LIMIT ?3",
        )?;
        Ok(stmt
            .query_map(
                params![
                    after,
                    filter == ParkedFilter::Open,
                    i64::try_from(limit.min(self.limits.page_rows))
                        .map_err(|_| Error::InvalidOperatorInput)?
                ],
                |r| {
                    Ok(ParkedItem {
                        id: r.get(0)?,
                        evidence: r.get(1)?,
                        resolution: r.get(2)?,
                    })
                },
            )?
            .collect::<Result<_, _>>()?)
    }

    /// The parked items resolved at a time in `from..until` (Unix seconds, as
    /// their resolution records it), newest first, at most `limit` and at most
    /// one page. A resolution that is not a readable audit entry has no time and
    /// is left out. Reading changes nothing.
    /// # Errors
    /// Reports database failures.
    pub fn parked_resolved_between(
        &self,
        from: i64,
        until: i64,
        limit: usize,
    ) -> Result<Vec<ParkedItem>, Error> {
        let limit = i64::try_from(limit.min(self.limits.page_rows))
            .map_err(|_| Error::InvalidOperatorInput)?;
        // json_extract refuses malformed JSON, so only valid resolutions are read.
        let mut stmt = self.db.prepare(
            "SELECT id,evidence,resolution FROM (SELECT id,evidence,resolution,CASE WHEN json_valid(resolution) THEN json_extract(resolution,'$.time') END AS at FROM parked WHERE resolution IS NOT NULL) WHERE at>=?1 AND at<?2 ORDER BY at DESC,id DESC LIMIT ?3",
        )?;
        Ok(stmt
            .query_map(params![from, until, limit], |r| {
                Ok(ParkedItem {
                    id: r.get(0)?,
                    evidence: r.get(1)?,
                    resolution: r.get(2)?,
                })
            })?
            .collect::<Result<_, _>>()?)
    }
}
