//! Stores inbound transfer chunks and durable acknowledgment evidence.
use crate::{
    FrameProvenance, IntakeError as Error, OperatorError, StorageError, Store, digest, key,
};
use analyte_model::{Identifier, Reason, ReceiptId, ReceiptMetadata};
use rusqlite::{OptionalExtension, params};
#[cfg(test)]
mod tests;

/// A sequence value checked against the bound supplied by its protocol owner.
/// This is independent of the store's monotonic journal identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameSequence(u8);

impl FrameSequence {
    /// Admits a byte-sized sequence only within the caller's inclusive maximum.
    /// # Errors
    /// Refuses a value above that maximum without writing any evidence.
    pub fn new(number: u8, maximum: u8) -> Result<Self, Error> {
        if number > maximum {
            return Err(Error::InvalidReceiptInput);
        }
        Ok(Self(number))
    }
}

/// A closed transfer either names retained receipt evidence or an explicit reason.
pub enum TransferOutcome {
    /// Every frame belongs to this already stored receipt.
    Complete(ReceiptId),
    /// The caller could not complete the transfer; retain its exact reason.
    Incomplete(Reason),
}

impl Store {
    /// Commits one bounded frame before returning its monotonic journal sequence.
    /// # Errors
    /// Refuses closed transfers, oversized frames, invalid frame numbers or capacity failures.
    pub(crate) fn frame(
        &mut self,
        transfer: &Identifier,
        sequence: FrameSequence,
        raw: &[u8],
    ) -> Result<i64, Error> {
        self.bound(raw)?;
        self.capacity(raw.len())?;
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
            params![transfer.as_str(), sequence.0, raw],
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
        outcome: TransferOutcome,
    ) -> Result<(), Error> {
        let (state, receipt, reason) = match outcome {
            TransferOutcome::Complete(receipt) => ("complete", Some(receipt), None),
            TransferOutcome::Incomplete(reason) => ("aborted", None, Some(reason)),
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
        if let Some(reason) = reason {
            tx.execute(
                "INSERT INTO parked(evidence) VALUES (?1)",
                [crate::parked::evidence(
                    "incomplete_transfer",
                    transfer.as_str(),
                    reason.as_str(),
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
    pub(crate) fn incomplete(&self, after: &str) -> Result<Vec<String>, Error> {
        let mut stmt = self.db.prepare(
            "SELECT id FROM transfers WHERE state!='complete' AND id>?1 ORDER BY id LIMIT ?2",
        )?;
        Ok(stmt
            .query_map(
                params![
                    after,
                    i64::try_from(self.limits.page_rows).map_err(|_| Error::InvalidReceiptInput)?
                ],
                |r| r.get(0),
            )?
            .collect::<Result<_, _>>()?)
    }

    /// Gives an aborted transfer the receipt it never got, once, so an operator can
    /// reprocess its acknowledged frames. The receipt holds the frames in journal
    /// order with the provenance retained before the first ACK. Frames carry no time
    /// or connection, so `received_at` is the rebuild time and `connection` names the
    /// transfer. Returns `None` for an unknown or still open transfer.
    /// # Errors
    /// Refuses missing provenance, bounds, capacity and database failures.
    pub(crate) fn transfer_receipt(
        &mut self,
        transfer: &str,
    ) -> Result<Option<ReceiptId>, OperatorError> {
        let row: Option<(String, Option<String>)> = self
            .db
            .query_row(
                "SELECT state,receipt FROM transfers WHERE id=?1",
                [transfer],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        match row {
            Some((_, Some(receipt))) => return Ok(Some(serde_json::from_str(&receipt)?)),
            Some((state, None)) if state == "aborted" => {}
            Some(_) | None => return Ok(None),
        }
        let provenance: FrameProvenance = serde_json::from_str(&self.db.query_row(
            "SELECT evidence FROM frame_provenance WHERE transfer=?1",
            [transfer],
            |r| r.get::<_, String>(0),
        )?)?;
        let mut raw = Vec::new();
        let mut after = 0;
        loop {
            let page = self.frames(
                &Identifier::new(transfer).map_err(StorageError::Text)?,
                after,
            )?;
            let Some(last) = page.last() else { break };
            after = last.sequence;
            for frame in page {
                raw.extend_from_slice(&frame.raw);
                self.bound(&raw)?;
            }
        }
        let config_version: String = digest(&provenance.config)
            .0
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        let metadata = ReceiptMetadata {
            id: ReceiptId::new(),
            raw_digest: digest(&raw),
            connection: Identifier::new(transfer).map_err(StorageError::Text)?,
            machine: provenance.machine,
            driver: provenance.driver,
            config_version: Identifier::new(&config_version).map_err(StorageError::Text)?,
            config_digest: digest(&provenance.config),
            received_at: jiff::Timestamp::now(),
        };
        // The receipt and its transfer link commit together, so a retry never builds twice.
        self.db.execute_batch("BEGIN IMMEDIATE")?;
        let linked = self
            .receive(&metadata, &raw, &provenance.config)
            .map_err(OperatorError::from)
            .and_then(|()| {
                Ok(self.db.execute(
                    "UPDATE transfers SET receipt=?2 WHERE id=?1 AND receipt IS NULL",
                    params![transfer, key(&metadata.id)?],
                )?)
            });
        self.db
            .execute_batch(if linked.is_ok() { "COMMIT" } else { "ROLLBACK" })?;
        linked?;
        Ok(Some(metadata.id))
    }

    /// Retains transfer provenance before journaling a frame; success permits its ACK.
    /// Provenance commits first, so a later frame failure never loses configuration evidence.
    /// # Errors
    /// Rejects changed provenance, bounds, capacity or database failures.
    pub fn journal_frame(
        &mut self,
        transfer: &Identifier,
        sequence: FrameSequence,
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
        self.frame(transfer, sequence, raw)
    }
}
