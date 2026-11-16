//! Stores raw receipts before machine acknowledgment.
use crate::{Error, Store, digest, key};
use analyte_model::{ReceiptId, ReceiptMetadata};
use rusqlite::{OptionalExtension, params};

impl Store {
    /// Commits exact bytes and effective config; supplied digests must match.
    /// # Errors
    /// Refuses insufficient capacity, oversized evidence, digest mismatches or duplicate IDs.
    pub fn receive(
        &mut self,
        metadata: &ReceiptMetadata,
        raw: &[u8],
        config: &[u8],
    ) -> Result<(), Error> {
        self.bound(raw)?;
        self.bound(config)?;
        let encoded = self.encode(metadata)?;
        self.bound(encoded.as_bytes())?;
        self.capacity(raw.len().checked_add(config.len()).ok_or(Error::Capacity)?)?;
        if digest(raw) != metadata.raw_digest || digest(config) != metadata.config_digest {
            return Err(Error::InvalidInput);
        }
        self.db.execute(
            "INSERT INTO receipts VALUES (?1,?2,?3,?4)",
            params![key(&metadata.id)?, encoded, raw, config],
        )?;
        Ok(())
    }

    /// Returns exact protected raw bytes for a receipt, or absence; never logs them.
    /// # Errors
    /// Reports database failures.
    pub fn raw(&self, id: ReceiptId) -> Result<Option<Vec<u8>>, Error> {
        Ok(self
            .db
            .query_row("SELECT raw FROM receipts WHERE id=?1", [key(&id)?], |r| {
                r.get(0)
            })
            .optional()?)
    }
}

#[cfg(test)]
mod tests;
