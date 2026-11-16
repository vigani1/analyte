//! Checks machine order eligibility and freshness.
use super::*;

impl Store {
    /// Reads a trusted, bounded page of pending work targeting this analyzer (LAW TF-2b 3.27.4).
    /// Shares the `SampleId` lookup's restore/trust/freshness rule. Pages scan at most the store
    /// row limit; filtered-empty is not a final empty answer while `after` is present.
    /// For one coherent multi-page answer, read all pages in one store-owner request.
    /// This is not send permission: callers still reserve items against current cache state.
    /// # Errors
    /// Reports database or retained-document decoding failures; never converts them to empty.
    pub fn pending_orders(
        &self,
        destination: &Identifier,
        machine: &Identifier,
        after: &str,
        now: i64,
        freshness: u64,
    ) -> Result<crate::PendingOrders, Error> {
        if !self.order_trust(destination, now, freshness)?.is_trusted() {
            return Ok(crate::PendingOrders::NotTrusted);
        }
        let limit = self
            .limits
            .page_rows
            .checked_add(1)
            .ok_or(Error::InvalidInput)?;
        let mut statement = self.db.prepare("SELECT evidence FROM orders o WHERE destination=?1 AND id>?3 AND tombstone=0 AND expiry>?4 AND generation=(SELECT generation FROM feed WHERE destination=?1) AND NOT EXISTS(SELECT 1 FROM machine_orders m WHERE m.destination=o.destination AND m.order_id=o.id AND m.machine=?2) ORDER BY id LIMIT ?5")?;
        let mut encoded = statement
            .query_map(
                params![
                    destination.as_str(),
                    machine.as_str(),
                    after,
                    now,
                    i64::try_from(limit).map_err(|_| Error::InvalidInput)?
                ],
                |row| row.get::<_, String>(0),
            )?
            .collect::<Result<Vec<_>, _>>()?;
        let more = encoded.len() > self.limits.page_rows;
        if more {
            encoded.pop();
        }
        let mut orders = Vec::new();
        let mut cursor = None;
        for encoded in encoded {
            let cached: CachedOrder = serde_json::from_str(&encoded)?;
            cursor = Some(cached.id.clone());
            let fields: CachedFields = serde_json::from_str(&cached.document)?;
            if fields
                .state
                .eligible_for(
                    OrderState::Requested,
                    fields.tombstone,
                    fields
                        .target_analyzers
                        .as_ref()
                        .map(|targets| targets.iter().map(Identifier::as_str)),
                    machine.as_str(),
                )
                .is_ok()
            {
                orders.push(cached);
            }
        }
        Ok(crate::PendingOrders::Page {
            orders,
            after: if more { cursor } else { None },
        })
    }

    /// Returns a bounded page of unreserved cache candidates, never permission to send.
    /// The host must map a candidate and call `reserve_send` immediately before I/O.
    /// # Errors
    /// Reports database or retained cache decoding failures.
    pub fn order_candidates(
        &self,
        destination: &Identifier,
        machine: &Identifier,
        after: &str,
        now: i64,
        freshness: u64,
    ) -> Result<Vec<CachedOrder>, Error> {
        if !self.order_trust(destination, now, freshness)?.is_trusted() {
            return Err(Error::Conflict);
        }
        let mut statement = self.db.prepare("SELECT evidence FROM orders o WHERE destination=?1 AND id>?3 AND generation=(SELECT generation FROM feed WHERE destination=?1) AND NOT EXISTS(SELECT 1 FROM machine_orders m WHERE m.destination=o.destination AND m.order_id=o.id AND m.machine=?2) ORDER BY id LIMIT ?4")?;
        let rows = statement
            .query_map(
                params![
                    destination.as_str(),
                    machine.as_str(),
                    after,
                    i64::try_from(self.limits.page_rows).map_err(|_| Error::InvalidInput)?
                ],
                |r| r.get::<_, String>(0),
            )?
            .collect::<Result<Vec<_>, _>>()?;
        rows.iter()
            .map(|row| Ok(serde_json::from_str(row)?))
            .collect()
    }
}
