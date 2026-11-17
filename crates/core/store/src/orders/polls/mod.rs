//! Records cursor-changing order polls and operator reconciliation.
use crate::{Audit, OrderError as Error, Parked, Store, parked::ParkingEvidence};
use analyte_model::{Identifier, Reason};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
#[cfg(test)]
mod tests;

/// Store-assigned identity of one poll admission; retained after confirmation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PollIntentId(i64);
impl std::fmt::Display for PollIntentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Durable knowledge of one cursor-changing poll, independent of transport state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollIntentState {
    /// Admitted before transmission; interruption may leave an unknown cursor movement.
    Pending,
    /// A retained response or explicit operator evidence reconciled custody.
    Confirmed,
    /// No reliable response outcome; a new poll is forbidden until reconciliation.
    Uncertain,
}

/// Evidence supplied after admission; returning to Pending is never an outcome.
#[derive(Debug, Clone, Copy)]
pub enum PollOutcome {
    /// Response custody is durable, or an operator explicitly reconciled it.
    Confirmed,
    /// Transfer, storage or connection evidence cannot establish custody.
    Uncertain,
}

/// Protected retained poll snapshot; every reason change also appends history.
#[derive(Debug)]
pub struct PollIntent {
    /// Stable poll admission identity.
    pub id: PollIntentId,
    /// Installed machine whose cursor may have moved.
    pub machine: Identifier,
    /// Linked operator evidence, retained even after resolution.
    pub parked: i64,
    /// Last evidenced custody state.
    pub state: PollIntentState,
    /// Exact caller or legacy explanation.
    pub reason: Reason,
}

fn read(row: &rusqlite::Row<'_>) -> rusqlite::Result<PollIntent> {
    let decode = || -> Result<PollIntent, Error> {
        let id: i64 = row.get(0)?;
        if id <= 0 {
            return Err(Error::Corruption);
        }
        Ok(PollIntent {
            id: PollIntentId(id),
            machine: Identifier::new(&row.get::<_, String>(1)?).map_err(|_| Error::Corruption)?,
            parked: row.get(2)?,
            state: row.get(3)?,
            reason: Reason::new(&row.get::<_, String>(4)?).map_err(|_| Error::Corruption)?,
        })
    };
    decode().map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
    })
}

fn outstanding(db: &Connection, machine: &Identifier) -> Result<Option<PollIntent>, Error> {
    Ok(db.query_row("SELECT id,machine,parked,state,reason FROM poll_intents WHERE machine=?1 AND state IN (?2,?3) ORDER BY id LIMIT 1", params![machine.as_str(), PollIntentState::Pending, PollIntentState::Uncertain], read).optional()?)
}

impl Store {
    /// Atomically admits one poll before bytes leave and retains operator-visible evidence.
    /// A timeout may leave Pending committed; inspect the machine's outstanding intent.
    /// # Errors
    /// Refuses unresolved prior polls, oversized evidence, capacity or database failures.
    pub fn begin_poll_intent(
        &mut self,
        machine: &Identifier,
        reason: &Reason,
    ) -> Result<PollIntentId, Error> {
        let details = Parked {
            kind: Identifier::new("poll_intent").map_err(Error::Text)?,
            reason: reason.clone(),
            targets: vec![machine.clone()],
        };
        let encoded = self.encode(&ParkingEvidence {
            details,
            context: None,
        })?;
        self.capacity(
            encoded
                .len()
                .checked_add(
                    reason
                        .as_str()
                        .len()
                        .checked_mul(2)
                        .ok_or(Error::Capacity)?,
                )
                .and_then(|n| n.checked_add(256))
                .ok_or(Error::Capacity)?,
        )?;
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if outstanding(&tx, machine)?.is_some() {
            return Err(Error::PollIntentUnresolved);
        }
        tx.execute("INSERT INTO parked(evidence) VALUES (?1)", [encoded])?;
        let parked = tx.last_insert_rowid();
        tx.execute(
            "INSERT INTO poll_intents(machine,parked,state,reason) VALUES (?1,?2,?3,?4)",
            params![
                machine.as_str(),
                parked,
                PollIntentState::Pending,
                reason.as_str()
            ],
        )?;
        let id = PollIntentId(tx.last_insert_rowid());
        tx.execute(
            "INSERT INTO poll_intent_history(intent,state,reason) VALUES (?1,?2,?3)",
            params![id.0, PollIntentState::Pending, reason.as_str()],
        )?;
        tx.commit()?;
        Ok(id)
    }

    /// Reads at most one unresolved intent through the machine/state index.
    /// Absence alone permits admission; the write rechecks it transactionally.
    /// # Errors
    /// Reports stored-value decoding or database failures; never substitutes absence.
    pub fn unconfirmed_poll_intent(
        &self,
        machine: &Identifier,
    ) -> Result<Option<PollIntent>, Error> {
        outstanding(&self.db, machine)
    }

    /// Appends outcome evidence and its audit atomically. Confirmed custody is terminal.
    /// Audit targets must name `poll:<id>`; cancellation may leave this write committed.
    /// Repeating confirmation or cleanup after a committed confirmation is a no-op,
    /// and never downgrades known custody or authorizes replay of this identity.
    /// # Errors
    /// Refuses unknown intents, unrelated audit targets, bounds, capacity and database failures.
    pub fn record_poll_outcome(
        &mut self,
        id: PollIntentId,
        outcome: PollOutcome,
        audit: &Audit,
    ) -> Result<PollIntent, Error> {
        let encoded = self.encode(audit)?;
        let target = format!("poll:{id}");
        if !audit.targets.iter().any(|named| named.as_str() == target) {
            return Err(Error::AuditMismatch);
        }
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut intent = tx
            .query_row(
                "SELECT id,machine,parked,state,reason FROM poll_intents WHERE id=?1",
                [id.0],
                read,
            )
            .optional()?
            .ok_or(Error::UnknownPollIntent)?;
        let state = match outcome {
            PollOutcome::Confirmed => PollIntentState::Confirmed,
            PollOutcome::Uncertain => PollIntentState::Uncertain,
        };
        match intent.state {
            PollIntentState::Confirmed => return Ok(intent),
            PollIntentState::Pending | PollIntentState::Uncertain => {}
        }
        if intent.state == state && intent.reason == audit.reason {
            return Ok(intent);
        }
        let required = resolution_bytes(
            &tx,
            intent.parked,
            encoded.len(),
            audit.reason.as_str().len(),
        )?;
        Self::capacity_on(&tx, &self.path, &self.limits, &*self.space, required)?;
        tx.execute(
            "UPDATE poll_intents SET state=?2,reason=?3 WHERE id=?1",
            params![id.0, state, audit.reason.as_str()],
        )?;
        tx.execute(
            "INSERT INTO poll_intent_history(intent,state,reason,audit) VALUES (?1,?2,?3,?4)",
            params![id.0, state, audit.reason.as_str(), encoded],
        )?;
        match outcome {
            PollOutcome::Confirmed => {
                let unresolved: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM poll_intents WHERE parked=?1 AND state IN (?2,?3))",params![intent.parked,PollIntentState::Pending,PollIntentState::Uncertain],|row|row.get(0))?;
                if !unresolved
                    && tx.execute(
                        "UPDATE parked SET resolution=?2 WHERE id=?1 AND resolution IS NULL",
                        params![intent.parked, encoded],
                    )? != 1
                {
                    return Err(Error::Conflict);
                }
            }
            PollOutcome::Uncertain => {}
        }
        tx.execute("INSERT INTO audit(evidence) VALUES (?1)", [encoded])?;
        tx.commit()?;
        intent.state = state;
        intent.reason = audit.reason.clone();
        Ok(intent)
    }
}

// Include the parked resolution, global audit, and each linked poll's reason/history.
pub(crate) fn resolution_bytes(
    db: &Connection,
    parked: i64,
    audit_bytes: usize,
    reason_bytes: usize,
) -> Result<usize, Error> {
    let rows: i64 = db.query_row(
        "SELECT count(*) FROM poll_intents WHERE parked=?1 AND state IN (?2,?3)",
        params![parked, PollIntentState::Pending, PollIntentState::Uncertain],
        |row| row.get(0),
    )?;
    let rows = usize::try_from(rows).map_err(|_| Error::Corruption)?;
    rows.checked_add(2)
        .and_then(|n| n.checked_mul(audit_bytes))
        .and_then(|n| {
            rows.checked_mul(reason_bytes)
                .and_then(|r| r.checked_mul(2))
                .and_then(|r| n.checked_add(r))
        })
        .and_then(|n| rows.checked_mul(128).and_then(|r| n.checked_add(r)))
        .ok_or(Error::Capacity)
}

// The existing operator parked-resolution command reconciles every machine linked
// to that evidence, exactly as resolving a legacy custody row did before migration.
pub(crate) fn resolve_on(
    tx: &Transaction<'_>,
    parked: i64,
    audit: &Audit,
    encoded: &str,
) -> Result<(), Error> {
    tx.execute("INSERT INTO poll_intent_history(intent,state,reason,audit) SELECT id,?2,?3,?4 FROM poll_intents WHERE parked=?1 AND state IN (?5,?6)",params![parked,PollIntentState::Confirmed,audit.reason.as_str(),encoded,PollIntentState::Pending,PollIntentState::Uncertain])?;
    tx.execute(
        "UPDATE poll_intents SET state=?2,reason=?3 WHERE parked=?1 AND state IN (?4,?5)",
        params![
            parked,
            PollIntentState::Confirmed,
            audit.reason.as_str(),
            PollIntentState::Pending,
            PollIntentState::Uncertain
        ],
    )?;
    Ok(())
}
