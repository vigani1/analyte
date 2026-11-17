//! Parks failed processing outcomes with retained reasons.
use crate::{Audit, OperatorError as Error, Parked, Store};
use analyte_model::{Identifier, Reason};
use rusqlite::params;
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
