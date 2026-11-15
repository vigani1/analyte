//! Transfers storage responsibilities.
use crate::{Error, FrameProvenance, Store, key};
use analyte_model::{Identifier, ReceiptId};
use rusqlite::{OptionalExtension, params};

impl Store {
    /// Commits one ASTM frame before returning its monotonic journal sequence.
    /// # Errors
    /// Refuses closed transfers, oversized frames, invalid frame numbers or capacity failures.
    pub(super) fn frame(
        &mut self,
        transfer: &Identifier,
        number: u8,
        raw: &[u8],
    ) -> Result<i64, Error> {
        self.bound(raw)?;
        self.capacity(raw.len())?;
        if number > 7 {
            return Err(Error::InvalidInput);
        }
        let tx = self.db.transaction()?;
        tx.execute(
            "INSERT OR IGNORE INTO transfers VALUES (?1,'open',NULL)",
            [transfer.as_str()],
        )?;
        let state: String = tx.query_row(
            "SELECT state FROM transfers WHERE id=?1",
            [transfer.as_str()],
            |r| r.get(0),
        )?;
        if state != "open" {
            return Err(Error::Conflict);
        }
        tx.execute(
            "INSERT INTO frames(transfer,number,raw) VALUES (?1,?2,?3)",
            params![transfer.as_str(), number, raw],
        )?;
        let sequence = tx.last_insert_rowid();
        tx.commit()?;
        Ok(sequence)
    }

    /// Completes a transfer with a stored receipt, or preserves it as aborted evidence.
    /// # Errors
    /// Refuses missing receipts and already closed transfers.
    pub fn finish_transfer(
        &mut self,
        transfer: &Identifier,
        receipt: Option<ReceiptId>,
    ) -> Result<(), Error> {
        let state = if receipt.is_some() {
            "complete"
        } else {
            "aborted"
        };
        let receipt = receipt.map(|id| key(&id)).transpose()?;
        let tx = self.db.transaction()?;
        if tx.execute(
            "UPDATE transfers SET state=?2,receipt=?3 WHERE id=?1 AND state='open'",
            params![transfer.as_str(), state, receipt],
        )? != 1
        {
            return Err(Error::Conflict);
        }
        if receipt.is_none() {
            tx.execute(
                "INSERT INTO parked(evidence) VALUES (?1)",
                [crate::parked::evidence(
                    "incomplete_transfer",
                    transfer.as_str(),
                    "ASTM transfer ended before a complete receipt was retained",
                    None,
                )?],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Lists unfinished or aborted transfer IDs in a bounded page after an ID.
    /// # Errors
    /// Reports database failures.
    #[cfg(test)]
    pub(super) fn incomplete(&self, after: &str) -> Result<Vec<String>, Error> {
        let mut stmt = self.db.prepare(
            "SELECT id FROM transfers WHERE state!='complete' AND id>?1 ORDER BY id LIMIT ?2",
        )?;
        Ok(stmt
            .query_map(
                params![
                    after,
                    i64::try_from(self.limits.page_rows).map_err(|_| Error::InvalidInput)?
                ],
                |r| r.get(0),
            )?
            .collect::<Result<_, _>>()?)
    }

    /// Retains transfer provenance before journaling a frame; success permits its ACK.
    /// Provenance commits first, so a later frame failure never loses configuration evidence.
    /// # Errors
    /// Rejects changed provenance, bounds, capacity or database failures.
    pub fn journal_frame(
        &mut self,
        transfer: &Identifier,
        number: u8,
        raw: &[u8],
        provenance: &FrameProvenance,
    ) -> Result<i64, Error> {
        let evidence = self.encode(provenance)?;
        let retained: Option<String> = self
            .db
            .query_row(
                "SELECT evidence FROM frame_provenance WHERE transfer=?1",
                [transfer.as_str()],
                |row| row.get(0),
            )
            .optional()?;
        match retained {
            Some(retained) if retained != evidence => return Err(Error::Conflict),
            Some(_) => {}
            None => {
                self.capacity(evidence.len())?;
                self.db.execute(
                    "INSERT INTO frame_provenance VALUES (?1,?2)",
                    params![transfer.as_str(), evidence],
                )?;
            }
        }
        self.frame(transfer, number, raw)
    }
}
