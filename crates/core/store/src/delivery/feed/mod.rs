//! Stores and pages order-change feed entries.
use crate::{CachedOrder, Lookup, OrderError as Error, OrderPage, OrderTrust, Store};
use analyte_model::{Identifier, SampleId};
use rusqlite::{Connection, OptionalExtension, params};
#[cfg(test)]
mod tests;

#[derive(serde::Deserialize)]
struct Versioned {
    version: std::num::NonZeroU64,
}

impl Store {
    /// Stores a bounded page with its cursor in the same commit.
    /// Only the final page of a chain moves `polled`, so an unfinished chain leaves
    /// the cache as fresh as the last chain that reached the feed head.
    /// # Errors
    /// Rejects oversized pages and database failures.
    pub fn order_page(&mut self, page: &OrderPage) -> Result<(), Error> {
        if page.orders.len() > self.limits.page_rows {
            return Err(Error::InvalidOrderInput);
        }
        self.bound(self.encode(page)?.as_bytes())?;
        let tx = self.db.transaction()?;
        tx.execute(
            concat!(
                "INSERT INTO feed(destination,cursor,complete,trusted,polled) VALUES (?1,?2,?3,1,?4) ",
                "ON CONFLICT(destination) DO UPDATE SET cursor=excluded.cursor,",
                "complete=max(feed.complete,excluded.complete),trusted=1,",
                "polled=CASE WHEN excluded.complete THEN excluded.polled ELSE feed.polled END"
            ),
            params![page.destination.as_str(), page.cursor.as_str(), page.complete, page.polled],
        )?;
        for order in &page.orders {
            if let Some(reason) = Self::refused_version(&tx, &page.destination, order)? {
                tx.execute(
                    "INSERT INTO parked(evidence) VALUES (?1)",
                    [crate::parked::evidence(
                        "order_version_refused",
                        order.id.as_str(),
                        &reason,
                        None,
                    )?],
                )?;
                // The newer cached version stays part of a reloaded snapshot (after 410).
                tx.execute(
                    "UPDATE orders SET generation=(SELECT generation FROM feed WHERE destination=?1) WHERE destination=?1 AND id=?2",
                    params![page.destination.as_str(), order.id.as_str()],
                )?;
                continue;
            }
            tx.execute("INSERT INTO orders VALUES (?1,?2,?3,?4,?5,?6,(SELECT generation FROM feed WHERE destination=?1)) ON CONFLICT(destination,id) DO UPDATE SET sample=excluded.sample,evidence=excluded.evidence,tombstone=excluded.tombstone,expiry=excluded.expiry,generation=excluded.generation", params![page.destination.as_str(),order.id.as_str(),order.sample.as_str(),crate::document(order)?,order.tombstone,order.expiry])?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Versions only move forward (contract v1 `GET /orders`): an older version, or the same
    /// version with different content, keeps the cached order and returns the operator reason.
    fn refused_version(
        tx: &Connection,
        destination: &Identifier,
        order: &CachedOrder,
    ) -> Result<Option<String>, Error> {
        let cached: Option<String> = tx
            .query_row(
                "SELECT evidence FROM orders WHERE destination=?1 AND id=?2",
                params![destination.as_str(), order.id.as_str()],
                |r| r.get(0),
            )
            .optional()?;
        let Some(cached) = cached else {
            return Ok(None);
        };
        let cached: CachedOrder = serde_json::from_str(&cached)?;
        let version = |document: &str| {
            serde_json::from_str::<Versioned>(document).map(|parsed| parsed.version)
        };
        let (kept, offered) = (version(&cached.document)?, version(&order.document)?);
        Ok(if offered < kept {
            Some(format!(
                "app sent order version {offered} after cached version {kept}; kept {kept}"
            ))
        } else if offered == kept && cached.document != order.document {
            Some(format!(
                "app sent different content for order version {kept}; kept the cached one"
            ))
        } else {
            None
        })
    }

    /// Invalidates the cursor and snapshot after 410; stale orders remain as evidence.
    /// # Errors
    /// Reports database failures.
    pub fn reset_orders(&mut self, destination: &Identifier) -> Result<(), Error> {
        self.db.execute(
            "UPDATE feed SET trusted=0,complete=0,cursor='',generation=generation+1 WHERE destination=?1",
            [destination.as_str()],
        )?;
        Ok(())
    }

    /// Returns three-way trust, refusing stale, incomplete, future-clock or oversized answers.
    /// # Errors
    /// Reports database or encoded evidence failures.
    pub fn orders(
        &self,
        destination: &Identifier,
        sample: &SampleId,
        now: i64,
        freshness: u64,
    ) -> Result<Lookup, Error> {
        if !self.order_trust(destination, now, freshness)?.is_trusted() {
            return Ok(Lookup::NotTrusted);
        }
        let mut stmt=self.db.prepare("SELECT evidence FROM orders WHERE destination=?1 AND sample=?2 AND tombstone=0 AND expiry>?3 AND generation=(SELECT generation FROM feed WHERE destination=?1) LIMIT ?4")?;
        let limit = self
            .limits
            .page_rows
            .checked_add(1)
            .ok_or(Error::InvalidOrderInput)?;
        let encoded = stmt
            .query_map(
                params![
                    destination.as_str(),
                    sample.as_str(),
                    now,
                    i64::try_from(limit).map_err(|_| Error::InvalidOrderInput)?
                ],
                |r| r.get::<_, String>(0),
            )?
            .collect::<Result<Vec<_>, _>>()?;
        if encoded.len() > self.limits.page_rows {
            return Ok(Lookup::NotTrusted);
        }
        let orders = encoded
            .iter()
            .map(|s| serde_json::from_str(s))
            .collect::<Result<Vec<_>, _>>()?;
        if orders.is_empty() {
            Ok(Lookup::NoOrders)
        } else {
            Ok(Lookup::Orders(orders))
        }
    }

    /// Evaluates the destination cache without conflating unavailable data with no orders.
    /// The transaction helper uses exactly this rule for atomic reservation checks.
    /// # Errors
    /// Reports database failures; never substitutes an empty lookup.
    pub fn order_trust(
        &self,
        destination: &Identifier,
        now: i64,
        freshness: u64,
    ) -> Result<OrderTrust, Error> {
        Self::order_trust_on(&self.db, destination, now, freshness)
    }

    pub(crate) fn order_trust_on(
        db: &Connection,
        destination: &Identifier,
        now: i64,
        freshness: u64,
    ) -> Result<OrderTrust, Error> {
        if Self::restore_paused(db)? {
            return Ok(OrderTrust::RestorePaused);
        }
        let feed: Option<(i64, bool, bool)> = db
            .query_row(
                "SELECT polled,complete,trusted FROM feed WHERE destination=?1",
                [destination.as_str()],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        let Some((polled, complete, trusted)) = feed else {
            return Ok(OrderTrust::Unknown);
        };
        if !complete || !trusted {
            return Ok(OrderTrust::Incomplete);
        }
        let Some(age) = now
            .checked_sub(polled)
            .and_then(|age| u64::try_from(age).ok())
        else {
            return Ok(OrderTrust::Unknown);
        };
        if age >= freshness {
            Ok(OrderTrust::Stale { age })
        } else {
            Ok(OrderTrust::Trusted { age })
        }
    }
}
