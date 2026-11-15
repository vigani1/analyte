//! Parked storage responsibilities.
use crate::{Audit, Error, Parked, Store};
use rusqlite::params;

impl Store {
    /// Appends operator-visible evidence without a deletion API.
    /// # Errors
    /// Refuses oversized evidence or database failures.
    pub fn park(&mut self, item: &Parked) -> Result<i64, Error> {
        let encoded = self.encode(item)?;
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
        if tx.execute(
            "UPDATE parked SET resolution=?2 WHERE id=?1 AND resolution IS NULL",
            params![id, encoded],
        )? != 1
        {
            return Err(Error::Conflict);
        }
        tx.execute("INSERT INTO audit(evidence) VALUES (?1)", [encoded])?;
        tx.commit()?;
        Ok(())
    }

    /// Appends an audit entry permanently.
    /// # Errors
    /// Refuses oversized entries or database failures.
    pub fn audit(&mut self, entry: &Audit) -> Result<(), Error> {
        let encoded = self.encode(entry)?;
        self.bound(encoded.as_bytes())?;
        self.db
            .execute("INSERT INTO audit(evidence) VALUES (?1)", [encoded])?;
        Ok(())
    }
}
