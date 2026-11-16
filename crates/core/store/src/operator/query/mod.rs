//! Reads protected receipts, records and operator evidence.
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
    /// Canonical protected reason, links and context as stored JSON.
    pub evidence: String,
    /// Actor, time and method of resolution, absent while open.
    pub resolution: Option<String>,
}

impl ParkedItem {
    /// Decodes the canonical protected parking record.
    /// # Errors
    /// Refuses corrupt evidence instead of inventing receipt links.
    pub fn details(&self) -> Result<crate::Parked, Error> {
        Ok(serde_json::from_str::<crate::parked::ParkingEvidence>(&self.evidence)?.details)
    }
}

impl Store {
    /// Resolves an open parked item to exactly one retained receipt.
    /// # Errors
    /// Refuses missing/resolved items and absent or ambiguous receipt links.
    pub fn parked_receipt(&self, id: i64) -> Result<ReceiptId, Error> {
        let item = self.parked_item(id)?.ok_or(Error::ReprocessParked)?;
        if item.resolution.is_some() {
            return Err(Error::ReprocessParked);
        }
        let details = item.details()?;
        let mut receipt = None;
        for target in details.targets {
            // IDs share a representation; existence in receipts establishes the link.
            if let Ok(id) = serde_json::from_value::<ReceiptId>(serde_json::Value::String(
                target.as_str().to_owned(),
            )) && self.receipt_metadata(id)?.is_some()
                && receipt.replace(id).is_some()
            {
                return Err(Error::ReprocessParked);
            }
        }
        receipt.ok_or(Error::ReprocessParked)
    }

    /// Reads receipt metadata without loading raw payload or configuration bytes.
    /// # Errors
    /// Reports database failures or corrupt retained metadata.
    pub fn receipt_metadata(&self, id: ReceiptId) -> Result<Option<ReceiptMetadata>, Error> {
        let encoded: Option<String> = self
            .db
            .query_row(
                "SELECT metadata FROM receipts WHERE id=?1",
                [key(&id)?],
                |row| row.get(0),
            )
            .optional()?;
        if encoded.is_none() && self.receipt_was_purged(id)? {
            return Err(Error::Purged);
        }
        encoded
            .map(|text| {
                let metadata: ReceiptMetadata = serde_json::from_str(&text)?;
                if metadata.id != id {
                    return Err(Error::Corruption);
                }
                Ok(metadata)
            })
            .transpose()
    }

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

    /// Reads the retained cache entry for operator inspection, including stale work.
    /// This is not permission to send: check its version against the lifecycle
    /// record and use the normal reservation APIs before any machine I/O.
    /// # Errors
    /// Reports database failures or invalid retained cache evidence.
    pub fn cached_order(
        &self,
        destination: &Identifier,
        id: &Identifier,
    ) -> Result<Option<crate::CachedOrder>, Error> {
        let encoded: Option<String> = self
            .db
            .query_row(
                "SELECT evidence FROM orders WHERE destination=?1 AND id=?2",
                params![destination.as_str(), id.as_str()],
                |row| row.get(0),
            )
            .optional()?;
        encoded
            .map(|text| {
                let order: crate::CachedOrder = serde_json::from_str(&text)?;
                if order.id != *id {
                    return Err(Error::Corruption);
                }
                Ok(order)
            })
            .transpose()
    }

    /// Reads the first interpreted receipt for a machine connection, with digest checks.
    /// Connection context is evidence, never a substitute for a result identity.
    /// # Errors
    /// Reports corrupt metadata, digest mismatches or database failures.
    pub fn first_interpreted_connection_receipt(
        &self,
        machine: &Identifier,
        connection: &Identifier,
    ) -> Result<Option<Receipt>, Error> {
        let id: Option<String> = self.db.query_row(
            "SELECT r.id FROM receipts r WHERE json_extract(r.metadata,'$.machine')=?1 AND json_extract(r.metadata,'$.connection')=?2 AND EXISTS (SELECT 1 FROM interpretations i WHERE i.receipt=r.id) ORDER BY r.rowid LIMIT 1",
            params![machine.as_str(), connection.as_str()], |r| r.get(0),
        ).optional()?;
        match id {
            Some(id) => self.receipt(serde_json::from_str(&id)?),
            None => Ok(None),
        }
    }

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
        if row.is_none() && self.receipt_was_purged(id)? {
            return Err(Error::Purged);
        }
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

impl Store {
    /// Enqueues an order event atomically without creating result evidence.
    /// # Errors
    /// Refuses bounds, capacity failures, duplicate IDs and database errors.
    pub fn enqueue_order_event(
        &mut self,
        event: &crate::OrderEventDelivery,
        now: i64,
    ) -> Result<(), Error> {
        self.bound(&event.payload)?;
        self.capacity(event.payload.len())?;
        self.db.execute("INSERT INTO outbox(id,kind,order_id,destination,ordering_key,payload,digest,contract,state,next_attempt,enqueued_at) VALUES (?1,?8,?2,?3,?2,?4,?5,?6,?9,0,?7)", params![key(&event.id)?,event.order_id.as_str(),event.destination.as_str(),event.payload,crate::digest(&event.payload).0.as_slice(),event.contract_version,now,crate::DeliveryKind::OrderEvent,crate::DeliveryState::Pending])?;
        Ok(())
    }

    /// Reads one consistent destination snapshot without exposing clinical bytes.
    /// # Errors
    /// Returns database or aggregate overflow failures.
    pub fn health(
        &self,
        destination: &Identifier,
        now: i64,
        freshness: u64,
    ) -> Result<crate::Health, Error> {
        let (count, bytes, oldest, paused): (i64,i64,Option<i64>,bool) = self.db.query_row("SELECT count(*),coalesce(sum(length(payload)),0),min(enqueued_at),coalesce(max(state=?2),0) FROM outbox WHERE destination=?1 AND state!=?3",params![destination.as_str(),crate::DeliveryState::Paused,crate::DeliveryState::Done], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))?;
        let feed: Option<(i64, bool, bool)> = self
            .db
            .query_row(
                "SELECT polled,complete,trusted FROM feed WHERE destination=?1",
                [destination.as_str()],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        Ok(crate::Health {
            pending_count: u64::try_from(count).map_err(|_| Error::Corruption)?,
            pending_bytes: u64::try_from(bytes).map_err(|_| Error::Corruption)?,
            oldest_pending_age: oldest
                .and_then(|time| now.checked_sub(time))
                .and_then(|age| u64::try_from(age).ok()),
            paused,
            last_successful_poll: feed.map(|f| f.0),
            snapshot_complete: feed.is_some_and(|f| f.1),
            cache_trusted: self.order_trust(destination, now, freshness)?.is_trusted(),
        })
    }
}

impl Store {
    /// Lists receipt metadata in a bounded page ordered by stored UUID representation.
    /// Pass the previous page's final receipt UUID serialized as JSON as `after`, or
    /// an empty string initially. Payloads are read separately through `receipt`.
    /// # Errors
    /// Reports database errors or corrupt retained metadata.
    pub fn receipts(&self, after: &str) -> Result<Vec<ReceiptMetadata>, Error> {
        let mut statement = self
            .db
            .prepare("SELECT metadata FROM receipts WHERE id>?1 ORDER BY id LIMIT ?2")?;
        let rows = statement.query_map(
            params![
                after,
                i64::try_from(self.limits.page_rows).map_err(|_| Error::InvalidInput)?
            ],
            |row| row.get::<_, String>(0),
        )?;
        rows.map(|row| serde_json::from_str(&row?).map_err(Error::from))
            .collect()
    }
}

impl Store {
    /// Reads a bounded page of permanent operator actions, ordered by audit sequence.
    /// # Errors
    /// Reports database errors or corrupt audit evidence.
    pub fn audits(&self, after: i64) -> Result<Vec<(i64, crate::Audit)>, Error> {
        let mut statement = self.db.prepare(
            "SELECT sequence,evidence FROM audit WHERE sequence>?1 ORDER BY sequence LIMIT ?2",
        )?;
        let rows = statement.query_map(
            params![
                after,
                i64::try_from(self.limits.page_rows).map_err(|_| Error::InvalidInput)?
            ],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        )?;
        rows.map(|row| {
            let (sequence, evidence) = row?;
            Ok((sequence, serde_json::from_str(&evidence)?))
        })
        .collect()
    }
}

#[cfg(test)]
mod operator_read_tests {
    use super::*;
    use crate::test_support::{TestResult, open};
    use analyte_model::{Digest, NamespacedId, Reason, SampleId};

    // Plan section 11: exact metadata reads do not expose receipt bytes, and
    // stale cache inspection is distinct from permission to send machine work.
    #[test]
    fn plan11_operator_reads_preserve_identity_absence_and_stale_evidence() -> TestResult {
        let temp = tempfile::tempdir()?;
        let mut store = open(&temp.path().join("store.db"))?;
        let id = ReceiptId::try_from(uuid::Uuid::now_v7())?;
        assert!(store.receipt_metadata(id)?.is_none());
        assert!(store.parked_item(1)?.is_none());
        let destination = Identifier::new("app")?;
        let order = Identifier::new("order")?;
        assert!(store.cached_order(&destination, &order)?.is_none());
        let raw = b"protected payload";
        let config = b"config";
        let metadata = ReceiptMetadata {
            id,
            raw_digest: Digest(crate::digest(raw).0),
            connection: Identifier::new("connection")?,
            machine: Identifier::new("machine")?,
            driver: NamespacedId {
                namespace: Identifier::new("test")?,
                value: Identifier::new("1")?,
            },
            config_version: Identifier::new("1")?,
            config_digest: Digest(crate::digest(config).0),
            received_at: jiff::Timestamp::UNIX_EPOCH,
        };
        store.receive(&metadata, raw, config)?;
        assert_eq!(store.receipt_metadata(id)?.ok_or("metadata absent")?.id, id);
        let parked = store.park(&crate::Parked {
            kind: Identifier::new("parse")?,
            reason: Reason::new("<script>reason</script>")?,
            targets: vec![Identifier::new(&uuid::Uuid::from(id).to_string())?],
        })?;
        let item = store.parked_item(parked)?.ok_or("parked absent")?;
        let evidence: crate::Parked = serde_json::from_str(&item.evidence)?;
        assert_eq!(evidence.reason.as_str(), "<script>reason</script>");
        assert!(
            store
                .parked_item(parked.checked_add(1).ok_or("overflow")?)?
                .is_none()
        );
        store.order_page(&crate::OrderPage {
            destination: destination.clone(),
            cursor: Identifier::new("cursor")?,
            complete: true,
            polled: 100,
            orders: vec![crate::CachedOrder {
                id: order.clone(),
                sample: SampleId::new("001<script>")?,
                document: "retained document".into(),
                tombstone: true,
                expiry: 200,
            }],
        })?;
        store.reset_orders(&destination)?;
        let retained = store
            .cached_order(&destination, &order)?
            .ok_or("cache absent")?;
        assert_eq!(retained.sample.as_str(), "001<script>");
        assert!(retained.tombstone);
        assert_eq!(retained.document, "retained document");
        assert!(
            store
                .cached_order(&Identifier::new("different-app")?, &order)?
                .is_none()
        );
        assert_eq!(store.raw(id)?.ok_or("raw absent")?, raw);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
