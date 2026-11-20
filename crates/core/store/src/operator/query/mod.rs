//! Reads protected receipts, records and operator evidence.
use crate::delivery::outbox::outbox_owners;
use crate::sql_types::SqlOrderState;
use crate::{DeliveryState, Interpretation, OperatorError as Error, Store, key};
use analyte_model::{Identifier, OrderState, ReceiptId, ReceiptMetadata};
use rusqlite::{OptionalExtension, ToSql, params};
#[cfg(test)]
mod tests;

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
    pub fn details(&self) -> Result<crate::Parked, serde_json::Error> {
        Ok(serde_json::from_str::<crate::parked::ParkingEvidence>(&self.evidence)?.details)
    }
}

impl Store {
    /// Resolves an open parked item to exactly one retained receipt. An aborted ASTM
    /// transfer is parked by its transfer identity; its receipt is rebuilt from the
    /// acknowledged frames the first time it is asked for.
    /// # Errors
    /// Refuses missing/resolved items and absent or ambiguous receipt links.
    pub fn parked_receipt(&mut self, id: i64) -> Result<ReceiptId, Error> {
        let item = self.parked_item(id)?.ok_or(Error::ReprocessParked)?;
        if item.resolution.is_some() {
            return Err(Error::ReprocessParked);
        }
        let details = item.details()?;
        let mut receipt = None;
        for target in &details.targets {
            // IDs share a representation; existence in receipts establishes the link.
            if let Ok(id) = serde_json::from_value::<ReceiptId>(serde_json::Value::String(
                target.as_str().to_owned(),
            )) && self.receipt_metadata(id)?.is_some()
                && receipt.replace(id).is_some()
            {
                return Err(Error::ReprocessParked);
            }
        }
        if receipt.is_none() {
            for target in &details.targets {
                if let Some(id) = self.transfer_receipt(target.as_str())?
                    && receipt.replace(id).is_some()
                {
                    return Err(Error::ReprocessParked);
                }
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
                    i64::try_from(self.limits.page_rows)
                        .map_err(|_| Error::InvalidOperatorInput)?
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

    /// The newest audit entries recording `action` on `target`, oldest first: at
    /// most one page, and whether older entries exist beyond it.
    /// # Errors
    /// Reports database failures or corrupt audit entries.
    pub fn audits_naming(
        &self,
        target: &Identifier,
        action: &Identifier,
    ) -> Result<(Vec<crate::Audit>, bool), Error> {
        let page = self.limits.page_rows;
        let limit = i64::try_from(page.checked_add(1).ok_or(Error::InvalidOperatorInput)?)
            .map_err(|_| Error::InvalidOperatorInput)?;
        // The quoted target narrows the scan before each entry's targets are read.
        let quoted = serde_json::to_string(target.as_str())?;
        let mut stmt = self.db.prepare(
            "SELECT evidence FROM audit WHERE instr(evidence,?1)>0 AND json_extract(evidence,'$.action')=?2 AND EXISTS(SELECT 1 FROM json_each(evidence,'$.targets') t WHERE t.value=?3) ORDER BY sequence DESC LIMIT ?4",
        )?;
        let mut rows = stmt
            .query_map(
                params![quoted, action.as_str(), target.as_str(), limit],
                |r| r.get::<_, String>(0),
            )?
            .map(|row| Ok(serde_json::from_str::<crate::Audit>(&row?)?))
            .collect::<Result<Vec<_>, Error>>()?;
        let older = rows.len() > page;
        rows.truncate(page);
        rows.reverse();
        Ok((rows, older))
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
        self.db.execute("INSERT INTO outbox(id,kind,order_id,destination,ordering_key,payload,digest,contract,state,next_attempt,enqueued_at) VALUES (?1,?8,?2,?3,?2,?4,?5,?6,?9,0,?7)", params![key(&event.id)?,event.order_id.as_str(),event.destination.as_str(),event.payload,crate::digest(&event.payload).0.as_slice(),event.contract_version,now,crate::DeliveryKind::OrderEvent,DeliveryState::Pending])?;
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
        let (count, bytes, oldest, paused): (i64,i64,Option<i64>,bool) = self.db.query_row("SELECT count(*),coalesce(sum(length(payload)),0),min(enqueued_at),coalesce(max(state=?2),0) FROM outbox WHERE destination=?1 AND state!=?3",params![destination.as_str(),DeliveryState::Paused,DeliveryState::Done], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))?;
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

/// When each lab app last accepted a delivery, as far as the store still knows.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct LastDelivered {
    /// Each destination with at least one accepted delivery still stored, and
    /// the Unix second of its latest accepted attempt.
    pub at: Vec<(Identifier, i64)>,
    /// Data keeping has purged delivered items, so a destination missing from
    /// `at` may still have delivered before: the purged records keep no
    /// destination. False means a missing destination never delivered.
    pub purged: bool,
}

/// Every lab app with a stored delivery, as `lab(destination)` in name order,
/// ending with one NULL row. Each step is one index lookup, so the cost grows
/// with the number of lab apps, not deliveries.
macro_rules! lab_apps {
    () => {
        concat!(
            "WITH RECURSIVE lab(destination) AS (SELECT (SELECT min(destination) FROM outbox) ",
            "UNION ALL SELECT (SELECT min(destination) FROM outbox WHERE destination>lab.destination) ",
            "FROM lab WHERE lab.destination IS NOT NULL) "
        )
    };
}

/// Every delivery state but Done. A delivery in Done holds no attempt after
/// its own latest accepted one, since every attempt moves a delivery out of
/// Done unless it is itself accepted; so a try after a lab app's last accepted
/// delivery always belongs to a delivery in one of these.
const NOT_DONE: [DeliveryState; 5] = [
    DeliveryState::Pending,
    DeliveryState::InFlight,
    DeliveryState::Parked,
    DeliveryState::Paused,
    DeliveryState::Conflict,
];

impl Store {
    /// Reads, in one query, the latest accepted ("done") attempt time per
    /// destination, results and order messages alike; read-only. Each lab
    /// app's time is one index lookup of its deliveries' `delivered_at`, which
    /// the store keeps as the latest time of each delivery's accepted attempts.
    /// # Errors
    /// Reports database errors or a stored destination that is not an identifier.
    pub fn last_delivered(&self) -> Result<LastDelivered, Error> {
        let mut statement = self.db.prepare(concat!(
            lab_apps!(),
            "SELECT destination,(SELECT max(o.delivered_at) FROM outbox o WHERE o.destination=lab.destination) AS at ",
            "FROM lab WHERE at IS NOT NULL ORDER BY destination"
        ))?;
        let at = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?
            .map(|row| {
                let (destination, time) = row?;
                Ok((
                    Identifier::new(&destination).map_err(|_| Error::Corruption)?,
                    time,
                ))
            })
            .collect::<Result<Vec<_>, Error>>()?;
        let purged = self.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM retention_deliveries) OR EXISTS(SELECT 1 FROM retention_order_deliveries)",
            [],
            |row| row.get(0),
        )?;
        Ok(LastDelivered { at, purged })
    }
}

/// A lab app whose deliveries keep failing: items wait for it, and at least
/// one try failed after its last accepted delivery (or ever, with none).
/// A failed try is one the sink scheduled again (`RetryAt`): no answer, a
/// timeout, or an answer that settles nothing. Restart recovery
/// (`Interrupted`) is not a failed try. Only stored attempts count, so once
/// data keeping purged a lab app's delivered items, `since` may predate an
/// accepted delivery that is no longer kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailingDestination {
    /// The lab app.
    pub destination: Identifier,
    /// Unix second of the first failed try after the last accepted delivery.
    pub since: i64,
    /// Unix second of the latest failed try.
    pub last: i64,
    /// Failed tries from `since` to `last`, both included.
    pub tries: u64,
    /// Unix second the earliest waiting item is due to be tried again; none
    /// while every waiting item is being tried.
    pub next: Option<i64>,
}

impl Store {
    /// Reads every lab app whose deliveries keep failing, by the rule on
    /// [`FailingDestination`], ordered by name; read-only. Attempts are
    /// ordered by their stored sequence, so a failure and a success in the same
    /// second keep their real order. Only lab apps with items waiting are
    /// read, and for each only the tries of its deliveries not in Done that
    /// come after its last accepted attempt (`delivered_attempt`, see
    /// [`NOT_DONE`]), so the cost follows what is waiting, not what was kept.
    /// # Errors
    /// Reports database errors, a stored destination that is not an
    /// identifier, or a negative count.
    pub fn failing_destinations(&self) -> Result<Vec<FailingDestination>, Error> {
        let mut statement = self.db.prepare(concat!(
            lab_apps!(),
            ", waiting(destination, accepted) AS (SELECT lab.destination,",
            "(SELECT max(o.delivered_attempt) FROM outbox o WHERE o.destination=lab.destination) ",
            "FROM lab WHERE lab.destination IS NOT NULL ",
            "AND EXISTS(SELECT 1 FROM outbox w WHERE w.destination=lab.destination AND w.state IN (?1,?2))) ",
            "SELECT w.destination,min(a.time),max(a.time),count(*),",
            "(SELECT min(p.next_attempt) FROM outbox p WHERE p.destination=w.destination AND p.state=?1) ",
            "FROM waiting w CROSS JOIN outbox o CROSS JOIN attempts a ",
            "WHERE o.destination=w.destination AND o.state IN (?3,?4,?5,?6,?7) AND a.delivery=o.id ",
            "AND a.sequence>coalesce(w.accepted,0) AND json_type(a.outcome,'$.RetryAt')='integer' ",
            "GROUP BY w.destination ORDER BY w.destination",
        ))?;
        let [pending, in_flight, parked, paused, conflict] = NOT_DONE;
        statement
            .query_map(
                params![
                    DeliveryState::Pending,
                    DeliveryState::InFlight,
                    pending,
                    in_flight,
                    parked,
                    paused,
                    conflict
                ],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, Option<i64>>(4)?,
                    ))
                },
            )?
            .map(|row| {
                let (destination, since, last, tries, next) = row?;
                Ok(FailingDestination {
                    destination: Identifier::new(&destination).map_err(|_| Error::Corruption)?,
                    since,
                    last,
                    tries: u64::try_from(tries).map_err(|_| Error::Corruption)?,
                    next,
                })
            })
            .collect()
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
                i64::try_from(self.limits.page_rows).map_err(|_| Error::InvalidOperatorInput)?
            ],
            |row| row.get::<_, String>(0),
        )?;
        rows.map(|row| serde_json::from_str(&row?).map_err(Error::from))
            .collect()
    }

    /// When the latest stored message from each of `machines` was received, for
    /// those with any, in the order given; read-only. Each machine costs one
    /// index lookup: its messages in the latest received second (and any whose
    /// time SQLite cannot read, which sort apart) are compared in full, so
    /// fractions of a second decide as they would over every message.
    /// # Errors
    /// Reports database errors or a received time that is not a timestamp.
    pub fn newest_receipts(
        &self,
        machines: &[Identifier],
    ) -> Result<Vec<(Identifier, jiff::Timestamp)>, Error> {
        let mut statement = self.db.prepare(concat!(
            "SELECT json_extract(metadata,'$.received_at') FROM receipts ",
            "WHERE json_extract(metadata,'$.machine')=?1 AND unixepoch(json_extract(metadata,'$.received_at'))=",
            "(SELECT max(unixepoch(json_extract(metadata,'$.received_at'))) FROM receipts WHERE json_extract(metadata,'$.machine')=?1) ",
            "UNION ALL SELECT json_extract(metadata,'$.received_at') FROM receipts ",
            "WHERE json_extract(metadata,'$.machine')=?1 AND unixepoch(json_extract(metadata,'$.received_at')) IS NULL"
        ))?;
        let mut newest = Vec::new();
        for machine in machines {
            let mut latest: Option<jiff::Timestamp> = None;
            for received in
                statement.query_map([machine.as_str()], |row| row.get::<_, String>(0))?
            {
                let received: jiff::Timestamp =
                    serde_json::from_value(serde_json::Value::String(received?))?;
                latest = latest.max(Some(received));
            }
            if let Some(latest) = latest {
                newest.push((machine.clone(), latest));
            }
        }
        Ok(newest)
    }

    /// The newest stored messages from `machine`, newest first in store order: at
    /// most `limit` and at most one page. Each says what was stored about it: its
    /// latest reading, how its deliveries stand and the first open Needs review
    /// item naming it. No bytes, values or test codes leave the store; sample IDs
    /// do, as stored. Reading changes nothing.
    /// # Errors
    /// Reports database failures, bounds or corrupt stored evidence.
    pub fn recent_receipts(
        &self,
        machine: &Identifier,
        limit: usize,
    ) -> Result<Vec<RecentReceipt>, Error> {
        let limit = i64::try_from(limit.min(self.limits.page_rows))
            .map_err(|_| Error::InvalidOperatorInput)?;
        let mut statement = self.db.prepare(
            "SELECT id,metadata,(SELECT evidence FROM interpretations i WHERE i.receipt=r.id ORDER BY version DESC LIMIT 1),(SELECT max(version) FROM interpretations i WHERE i.receipt=r.id) FROM receipts r WHERE json_extract(metadata,'$.machine')=?1 ORDER BY rowid DESC LIMIT ?2",
        )?;
        let rows = statement
            .query_map(params![machine.as_str(), limit], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<i64>>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        // One pass each over the deliveries and the open items for the whole page.
        let bare = rows
            .iter()
            .map(|(id, ..)| serde_json::from_str::<String>(id))
            .collect::<Result<Vec<_>, _>>()?;
        let page = serde_json::to_string(&bare)?;
        let mut deliveries = self.db.prepare(
            "SELECT receipt,state,count(*) FROM outbox WHERE receipt IN (SELECT json_quote(value) FROM json_each(?1)) GROUP BY receipt,state ORDER BY receipt,min(sequence)",
        )?;
        let deliveries = deliveries
            .query_map([&page], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, DeliveryState>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut review = self.db.prepare(
            "SELECT json_quote(t.value),min(p.id) FROM parked p,json_each(p.evidence,'$.targets') t WHERE p.resolution IS NULL AND t.value IN (SELECT value FROM json_each(?1)) GROUP BY t.value",
        )?;
        let review = review
            .query_map([&page], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(|(id, metadata, evidence, version)| {
                let metadata: ReceiptMetadata = serde_json::from_str(&metadata)?;
                let reading = match (evidence, version) {
                    (Some(evidence), Some(version)) => Some(RecentReading {
                        version,
                        outcome: RecentOutcome::of(&serde_json::from_str(&evidence)?),
                    }),
                    (None, None) => None,
                    _ => return Err(Error::Corruption),
                };
                Ok(RecentReceipt {
                    metadata,
                    reading,
                    deliveries: deliveries
                        .iter()
                        .filter(|(receipt, ..)| *receipt == id)
                        .map(|&(_, state, count)| {
                            u64::try_from(count)
                                .map(|count| (state, count))
                                .map_err(|_| Error::Corruption)
                        })
                        .collect::<Result<_, _>>()?,
                    review: review
                        .iter()
                        .find(|(receipt, _)| *receipt == id)
                        .map(|&(_, item)| item),
                })
            })
            .collect()
    }
}

/// One stored message as a machine's page lists it: what was stored about it,
/// never its bytes, values or test codes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentReceipt {
    /// Its metadata, captured at reception.
    pub metadata: ReceiptMetadata,
    /// Its latest reading; none while it was never read.
    pub reading: Option<RecentReading>,
    /// How many of its deliveries, over all its readings, are in each state, in
    /// the order the first of each was queued; empty when none was queued.
    pub deliveries: Vec<(DeliveryState, u64)>,
    /// The first open Needs review item that names it.
    pub review: Option<i64>,
}

/// A message's latest reading, in short.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentReading {
    /// Its number; the first reading is 1.
    pub version: i64,
    /// What it produced.
    pub outcome: RecentOutcome,
}

/// What a reading produced, without values or test codes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecentOutcome {
    /// Results: how many, and each sample ID the machine sent, once, in order.
    Results {
        /// How many results it held.
        results: usize,
        /// The sample IDs, exactly as stored.
        samples: Vec<analyte_model::SampleId>,
    },
    /// It could not be read, and was kept for review.
    Failed,
}

impl RecentOutcome {
    /// What `interpretation` produced, without values or test codes.
    pub(crate) fn of(interpretation: &Interpretation) -> Self {
        match &interpretation.outcome {
            crate::Reading::Results(results) => {
                let mut samples: Vec<analyte_model::SampleId> = Vec::new();
                for sample in results.iter().filter_map(|result| result.sample.as_ref()) {
                    if !samples.contains(&sample.id) {
                        samples.push(sample.id.clone());
                    }
                }
                Self::Results {
                    results: results.len(),
                    samples,
                }
            }
            crate::Reading::Failed(_) => Self::Failed,
        }
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
                i64::try_from(self.limits.page_rows).map_err(|_| Error::InvalidOperatorInput)?
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

/// Work the store still holds open for one configured machine. Counting changes nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Unfinished {
    /// Result deliveries from this machine's receipts, and order events for its
    /// orders, that the lab app has not yet accepted.
    pub undelivered: u64,
    /// Machine orders sending, sent, accepted, cancel requested or uncertain.
    pub open_orders: u64,
    /// Receipts stored but never interpreted; only this machine's configuration reads them.
    pub unread: u64,
    /// Unresolved parked items whose targets link to this machine: its name, one of its
    /// receipts, deliveries, completed transfers or machine orders. Items that link to
    /// no single machine (an aborted transfer, a refused lab app order version) are not counted.
    pub flagged: u64,
}

impl Unfinished {
    /// True when nothing is open.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Matches a parked target to machine `?1`. Receipt and delivery keys are stored as
/// JSON strings, while parked targets hold the bare identity.
const PARKED_FOR_MACHINE: &str = concat!(
    "SELECT count(*) FROM parked p WHERE p.resolution IS NULL AND EXISTS(",
    "SELECT 1 FROM json_each(p.evidence,'$.targets') t WHERE t.value=?1 ",
    "OR EXISTS(SELECT 1 FROM receipts r WHERE r.id=json_quote(t.value) ",
    "AND json_extract(r.metadata,'$.machine')=?1) ",
    "OR EXISTS(SELECT 1 FROM outbox o JOIN receipts r ON r.id=o.receipt ",
    "WHERE o.id=json_quote(t.value) AND json_extract(r.metadata,'$.machine')=?1) ",
    "OR EXISTS(SELECT 1 FROM transfers x JOIN receipts r ON r.id=x.receipt ",
    "WHERE x.id=t.value AND json_extract(r.metadata,'$.machine')=?1) ",
    "OR EXISTS(SELECT 1 FROM machine_orders m WHERE CAST(m.id AS TEXT)=t.value AND m.machine=?1))"
);

impl Store {
    /// Counts what is still open for `machine` before an operator stops serving it.
    /// # Errors
    /// Reports database failures or corrupt counts.
    pub fn unfinished(&self, machine: &Identifier) -> Result<Unfinished, Error> {
        let count = |sql: &str, values: &[&dyn ToSql]| -> Result<u64, Error> {
            let value: i64 = self.db.query_row(sql, values, |r| r.get(0))?;
            u64::try_from(value).map_err(|_| Error::Corruption)
        };
        let machine = machine.as_str();
        Ok(Unfinished {
            undelivered: count(
                concat!(
                    "SELECT count(DISTINCT item) FROM (",
                    outbox_owners!("o.state!=?2"),
                    ") WHERE machine=?1"
                ),
                params![machine, DeliveryState::Done],
            )?,
            open_orders: count(
                "SELECT count(*) FROM machine_orders WHERE machine=?1 AND state IN (?2,?3,?4,?5,?6)",
                params![
                    machine,
                    SqlOrderState(OrderState::Sending),
                    SqlOrderState(OrderState::Sent),
                    SqlOrderState(OrderState::Accepted),
                    SqlOrderState(OrderState::CancelRequested),
                    SqlOrderState(OrderState::Uncertain)
                ],
            )?,
            unread: count(
                concat!(
                    "SELECT count(*) FROM receipts r WHERE json_extract(r.metadata,'$.machine')=?1 ",
                    "AND NOT EXISTS(SELECT 1 FROM interpretations i WHERE i.receipt=r.id)"
                ),
                params![machine],
            )?,
            flagged: count(PARKED_FOR_MACHINE, params![machine])?,
        })
    }

    /// True when the store holds any receipt, order, activity or retention record
    /// for `machine`, so a machine added under that name joins that history.
    /// # Errors
    /// Reports database failures.
    pub fn has_history(&self, machine: &Identifier) -> Result<bool, Error> {
        Ok(self.db.query_row(
            concat!(
                "SELECT EXISTS(SELECT 1 FROM machine_activity WHERE machine=?1) ",
                "OR EXISTS(SELECT 1 FROM machine_orders WHERE machine=?1) ",
                "OR EXISTS(SELECT 1 FROM poll_intents WHERE machine=?1) ",
                "OR EXISTS(SELECT 1 FROM retention_tombstones WHERE machine=?1) ",
                "OR EXISTS(SELECT 1 FROM receipts WHERE json_extract(metadata,'$.machine')=?1)"
            ),
            [machine.as_str()],
            |r| r.get(0),
        )?)
    }
}

impl Store {
    /// Lists a bounded page of raw receipts awaiting their first interpretation.
    /// Recovery owns no transport and cannot resend work.
    /// # Errors
    /// Reports invalid stored metadata, bounds and database failures.
    pub fn pending_interpretations(
        &self,
        machine: &Identifier,
        after: i64,
    ) -> Result<Vec<(i64, ReceiptMetadata)>, Error> {
        let mut query=self.db.prepare("SELECT r.rowid,r.metadata FROM receipts r WHERE r.rowid>?1 AND json_extract(r.metadata,'$.machine')=?2 AND NOT EXISTS(SELECT 1 FROM interpretations i WHERE i.receipt=r.id) ORDER BY r.rowid LIMIT ?3")?;
        let rows = query.query_map(
            params![
                after,
                machine.as_str(),
                i64::try_from(self.limits.page_rows).map_err(|_| Error::InvalidOperatorInput)?
            ],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)),
        )?;
        rows.map(|r| {
            let (sequence, metadata) = r?;
            Ok((sequence, serde_json::from_str(&metadata)?))
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
    // Plan §11
    #[test]
    fn operator_reads_keep_absence_and_stale_evidence() -> TestResult {
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
