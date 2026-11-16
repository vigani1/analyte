//! Stores and pages order-change feed entries.
use crate::{Error, Lookup, OrderPage, OrderTrust, Store};
use analyte_model::{Identifier, SampleId};
use rusqlite::{Connection, OptionalExtension, params};

impl Store {
    /// Stores a bounded page with its cursor in the same commit.
    /// # Errors
    /// Rejects oversized pages and database failures.
    pub fn order_page(&mut self, page: &OrderPage) -> Result<(), Error> {
        if page.orders.len() > self.limits.page_rows {
            return Err(Error::InvalidDeliveryInput);
        }
        self.bound(self.encode(page)?.as_bytes())?;
        let tx = self.db.transaction()?;
        tx.execute("INSERT INTO feed(destination,cursor,complete,trusted,polled) VALUES (?1,?2,?3,1,?4) ON CONFLICT(destination) DO UPDATE SET cursor=excluded.cursor,complete=max(feed.complete,excluded.complete),trusted=1,polled=excluded.polled",params![page.destination.as_str(),page.cursor.as_str(),page.complete,page.polled])?;
        for order in &page.orders {
            tx.execute("INSERT INTO orders VALUES (?1,?2,?3,?4,?5,?6,(SELECT generation FROM feed WHERE destination=?1)) ON CONFLICT(destination,id) DO UPDATE SET sample=excluded.sample,evidence=excluded.evidence,tombstone=excluded.tombstone,expiry=excluded.expiry,generation=excluded.generation", params![page.destination.as_str(),order.id.as_str(),order.sample.as_str(),crate::document(order)?,order.tombstone,order.expiry])?;
        }
        tx.commit()?;
        Ok(())
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
            .ok_or(Error::InvalidDeliveryInput)?;
        let encoded = stmt
            .query_map(
                params![
                    destination.as_str(),
                    sample.as_str(),
                    now,
                    i64::try_from(limit).map_err(|_| Error::InvalidDeliveryInput)?
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

#[cfg(test)]
mod tests;
