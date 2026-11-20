//! Lists durable machine orders and their states.
use super::{
    CachedFields, CachedOrder, Error, Identifier, MachineOrder, MachineOrderFilter,
    MachineOrderItem, MachineOrderKey, OrderState, SqlOrderState, Store, load, params,
};
use rusqlite::OptionalExtension as _;

impl Store {
    /// Looks up a reservation by its immutable key after an interrupted store wait.
    /// # Errors
    /// Reports storage failures or invalid retained evidence; absence is explicit.
    pub fn machine_order(&self, identity: &MachineOrderKey) -> Result<Option<MachineOrder>, Error> {
        let id = self
            .db
            .query_row(
                "SELECT id FROM machine_orders WHERE destination=?1 AND order_id=?2 AND machine=?3",
                params![
                    identity.destination.as_str(),
                    identity.order.as_str(),
                    identity.machine.as_str()
                ],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;
        if id.is_none()
            && crate::upkeep::retention::orders::was_purged(
                &self.db,
                self.retention_key.as_ref(),
                identity,
            )?
        {
            return Err(Error::Purged);
        }
        id.map(|id| load(&self.db, id)).transpose()
    }

    /// Returns a bounded page without exposing cache payloads.
    /// # Errors
    /// Reports database failures or invalid persisted evidence.
    pub fn machine_orders(&self, filter: &MachineOrderFilter) -> Result<Vec<MachineOrder>, Error> {
        let mut statement = self.db.prepare("SELECT id FROM machine_orders WHERE id>?1 AND (?2 IS NULL OR machine=?2) AND (?3 IS NULL OR state=?3) ORDER BY id LIMIT ?4")?;
        let state = filter.state.map(SqlOrderState);
        let ids = statement
            .query_map(
                params![
                    filter.after,
                    filter.machine.as_ref().map(Identifier::as_str),
                    state,
                    i64::try_from(self.limits.page_rows).map_err(|_| Error::InvalidOrderInput)?
                ],
                |r| r.get::<_, i64>(0),
            )?
            .collect::<Result<Vec<_>, _>>()?;
        ids.into_iter().map(|id| load(&self.db, id)).collect()
    }

    /// Counts machine orders per lifecycle state, in state name order; states
    /// with no order are left out.
    /// # Errors
    /// Reports database failures or corrupt stored states and counts.
    pub fn machine_order_counts(&self) -> Result<Vec<(OrderState, u64)>, Error> {
        let rows: Vec<(SqlOrderState, i64)> = self
            .db
            .prepare("SELECT state,count(*) FROM machine_orders GROUP BY state ORDER BY state")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<_, _>>()?;
        rows.into_iter()
            .map(|(SqlOrderState(state), count)| {
                Ok((state, u64::try_from(count).map_err(|_| Error::Corruption)?))
            })
            .collect()
    }

    /// Returns a bounded cancellation page; admission still rechecks the current feed.
    /// # Errors
    /// Reports retained evidence and database failures.
    pub fn cancel_candidates(
        &self,
        destination: &Identifier,
        machine: &Identifier,
        now: i64,
        freshness: u64,
    ) -> Result<Vec<(MachineOrder, CachedOrder)>, Error> {
        if !self.order_trust(destination, now, freshness)?.is_trusted() {
            return Err(Error::Conflict);
        }
        let mut after = 0;
        let mut candidates = Vec::new();
        // Bound filtering work without letting an ineligible first page starve later cancels.
        for _ in 0..128 {
            let mut statement = self.db.prepare("SELECT m.id,o.evidence FROM machine_orders m JOIN orders o ON o.destination=m.destination AND o.id=m.order_id JOIN feed f ON f.destination=o.destination AND f.generation=o.generation WHERE m.destination=?1 AND m.machine=?2 AND m.id>?3 AND NOT EXISTS(SELECT 1 FROM machine_order_history h WHERE h.machine_order=m.id AND h.cancel_version=(json_extract(o.evidence,'$.document')->'$.version')) ORDER BY m.id LIMIT ?4")?;
            let rows = statement
                .query_map(
                    params![
                        destination.as_str(),
                        machine.as_str(),
                        after,
                        i64::try_from(self.limits.page_rows)
                            .map_err(|_| Error::InvalidOrderInput)?
                    ],
                    |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)),
                )?
                .collect::<Result<Vec<_>, _>>()?;
            let exhausted = rows.len() < self.limits.page_rows;
            for (id, evidence) in rows {
                after = id;
                let order = load(&self.db, id)?;
                let cached: CachedOrder = serde_json::from_str(&evidence)?;
                let fields: CachedFields = serde_json::from_str(&cached.document)?;
                if !order.items.iter().any(MachineOrderItem::analyzer_cancelled)
                    && matches!(order.state, OrderState::Sent | OrderState::Accepted)
                    && cached.expiry > now
                    && fields
                        .state
                        .eligible_for(
                            OrderState::CancelRequested,
                            fields.tombstone,
                            fields
                                .target_analyzers
                                .as_ref()
                                .map(|targets| targets.iter().map(Identifier::as_str)),
                            machine.as_str(),
                        )
                        .is_ok()
                {
                    candidates.push((order, cached));
                    if candidates.len() == self.limits.page_rows {
                        return Ok(candidates);
                    }
                }
            }
            if exhausted {
                return Ok(candidates);
            }
        }
        Err(Error::InvalidOrderInput)
    }
}

impl Store {
    /// Finds a retained command by machine and exact placer value; ambiguity is refused.
    /// # Errors
    /// Reports ambiguity or invalid/database evidence.
    pub fn machine_order_by_placer(
        &self,
        machine: &Identifier,
        placer: &str,
    ) -> Result<Option<MachineOrder>, Error> {
        let mut query = self.db.prepare("SELECT DISTINCT o.id FROM machine_orders o JOIN machine_order_items i ON i.machine_order=o.id WHERE o.machine=?1 AND i.placer=?2 LIMIT 2")?;
        let ids = query
            .query_map(params![machine.as_str(), placer], |row| {
                row.get::<_, i64>(0)
            })?
            .collect::<Result<Vec<_>, _>>()?;
        match ids.as_slice() {
            [] => Ok(None),
            [id] => load(&self.db, *id).map(Some),
            _ => Err(Error::Conflict),
        }
    }
}
