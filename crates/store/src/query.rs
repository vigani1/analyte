use crate::{Error, Interpretation, Store, key};
use analyte_model::{Identifier, ReceiptId, ReceiptMetadata};
use rusqlite::{OptionalExtension, params};

/// Protected receipt evidence, including the exact effective configuration.
pub struct Receipt {
    /// Typed metadata and digests captured at reception.
    pub metadata: ReceiptMetadata,
    /// Exact bytes received, without text decoding.
    pub raw: Vec<u8>,
    /// Exact configuration excluding secrets.
    pub config: Vec<u8>,
}

/// One retained frame, including its monotonically increasing sequence.
pub struct Frame {
    /// Database journal sequence.
    pub sequence: i64,
    /// Protocol frame number, independent of journal sequence.
    pub number: u8,
    /// Exact original frame.
    pub raw: Vec<u8>,
}

/// Protected operator evidence, intended only for authenticated local views.
pub struct ParkedItem {
    /// Stable local identity used for resolution.
    pub id: i64,
    /// Original reason and links as stored JSON.
    pub evidence: String,
    /// Actor, time and method of resolution, absent while open.
    pub resolution: Option<String>,
}

impl Store {
    /// Reads complete receipt evidence without altering its interpretation.
    /// # Errors
    /// Reports corrupt metadata, digest mismatches or database failures.
    pub fn receipt(&self, id: ReceiptId) -> Result<Option<Receipt>, Error> {
        let row = self
            .db
            .query_row(
                "SELECT metadata,raw,config FROM receipts WHERE id=?1",
                [key(&id)?],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, Vec<u8>>(1)?,
                        r.get::<_, Vec<u8>>(2)?,
                    ))
                },
            )
            .optional()?;
        row.map(|(metadata, raw, config)| {
            let metadata: ReceiptMetadata = serde_json::from_str(&metadata)?;
            if metadata.id != id
                || metadata.raw_digest != crate::digest(&raw)
                || metadata.config_digest != crate::digest(&config)
            {
                return Err(Error::Corruption);
            }
            Ok(Receipt {
                metadata,
                raw,
                config,
            })
        })
        .transpose()
    }

    /// Reads one bounded page of journal evidence after a global sequence.
    /// # Errors
    /// Reports invalid stored frame numbers or database failures.
    pub fn frames(&self, transfer: &Identifier, after: i64) -> Result<Vec<Frame>, Error> {
        let mut stmt=self.db.prepare("SELECT sequence,number,raw FROM frames WHERE transfer=?1 AND sequence>?2 ORDER BY sequence LIMIT ?3")?;
        Ok(stmt
            .query_map(
                params![
                    transfer.as_str(),
                    after,
                    i64::try_from(self.limits.page_rows).map_err(|_| Error::InvalidInput)?
                ],
                |r| {
                    Ok(Frame {
                        sequence: r.get(0)?,
                        number: r.get(1)?,
                        raw: r.get(2)?,
                    })
                },
            )?
            .collect::<Result<_, _>>()?)
    }

    /// Reads a particular interpretation version without changing it.
    /// # Errors
    /// Reports invalid stored model contents or database failures.
    pub fn interpretation(
        &self,
        receipt: ReceiptId,
        version: i64,
    ) -> Result<Option<Interpretation>, Error> {
        let encoded: Option<String> = self
            .db
            .query_row(
                "SELECT evidence FROM interpretations WHERE receipt=?1 AND version=?2",
                params![key(&receipt)?, version],
                |r| r.get(0),
            )
            .optional()?;
        encoded
            .map(|s| serde_json::from_str(&s).map_err(Error::from))
            .transpose()
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
                    i64::try_from(self.limits.page_rows).map_err(|_| Error::InvalidInput)?
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

    /// Returns the feed cursor committed with its last page, absent after reset.
    /// # Errors
    /// Reports database failures.
    pub fn cursor(&self, destination: &Identifier) -> Result<Option<String>, Error> {
        Ok(self
            .db
            .query_row(
                "SELECT cursor FROM feed WHERE destination=?1 AND trusted=1",
                [destination.as_str()],
                |r| r.get(0),
            )
            .optional()?)
    }
}
