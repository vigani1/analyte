//! Migrates stored schemas while retaining existing evidence.
use crate::{MaintenanceError as Error, Store};
use rusqlite::Connection;
mod revisions;

impl Store {
    pub(crate) fn migrate_v2(db: &mut Connection) -> Result<(), Error> {
        // Rebuild with foreign keys temporarily disabled so attempts continue to
        // reference the same outbox name. The transaction retains every sequence.
        db.execute_batch("PRAGMA foreign_keys=OFF")?;
        let result = (|| {
            let tx = db.transaction()?;
            tx.execute_batch(include_str!("../migration_v2.sql"))?;
            Self::integrity(&tx)?;
            tx.commit()?;
            Ok::<_, Error>(())
        })();
        db.execute_batch("PRAGMA foreign_keys=ON")?;
        result?;
        Self::integrity(db)
    }
}

impl Store {
    pub(crate) fn migrate_v7(db: &mut Connection, limit: usize) -> Result<(), Error> {
        db.execute_batch("PRAGMA foreign_keys=OFF")?;
        let result = (|| {
            let tx = db.transaction()?;
            tx.execute_batch(include_str!("../migration_v7.sql"))?;
            migrate_rows(
                &tx,
                "machine_order_history",
                "sequence",
                limit,
                |id, encoded| {
                    let record: serde_json::Value = serde_json::from_str(encoded)?;
                    let code = record
                        .get("source_code")
                        .and_then(serde_json::Value::as_str)
                        .ok_or(Error::Corruption)?;
                    let owner = crate::machine_orders::ownership::OrderOwner::legacy(code)?;
                    let (kind, owner, version) = owner.columns();
                    tx.execute("UPDATE machine_order_history SET owner_kind=?2,owner_id=?3,cancel_version=?4 WHERE sequence=?1", rusqlite::params![id,kind,owner,version])?;
                    Ok(())
                },
            )?;
            migrate_rows(&tx, "parked", "id", limit, |id, encoded| {
                let canonical = parking(encoded)?;
                if canonical.len() > limit {
                    return Err(Error::InvalidMaintenanceInput);
                }
                tx.execute(
                    "UPDATE parked SET original_evidence=evidence,evidence=?2 WHERE id=?1",
                    rusqlite::params![id, canonical],
                )?;
                Ok(())
            })?;
            tx.execute_batch("CREATE TRIGGER machine_order_history_update BEFORE UPDATE ON machine_order_history BEGIN SELECT RAISE(ABORT,'immutable lifecycle history'); END;
CREATE TRIGGER parked_evidence_immutable BEFORE UPDATE OF evidence,original_evidence ON parked BEGIN SELECT RAISE(ABORT,'immutable parking evidence'); END;
PRAGMA user_version=7;")?;
            Self::integrity(&tx)?;
            tx.commit()?;
            Ok::<_, Error>(())
        })();
        db.execute_batch("PRAGMA foreign_keys=ON")?;
        result
    }
}

impl Store {
    pub(crate) fn migrate_v9(db: &mut Connection, limit: usize) -> Result<(), Error> {
        let tx = db.transaction()?;
        tx.execute_batch(include_str!("../migration_v9.sql"))?;
        migrate_rows(&tx, "parked", "id", limit, |id, encoded| {
            let evidence: crate::parked::ParkingEvidence = serde_json::from_str(encoded)?;
            let details = evidence.details;
            // Compatibility belongs only to migration; normal admission never reads categories.
            if details.kind.as_str() != "autolyser_poll_custody" {
                return Ok(());
            }
            if details.targets.is_empty() {
                return Err(Error::Corruption);
            }
            let (resolved, resolution): (bool, Option<String>) = tx.query_row(
                "SELECT resolution IS NOT NULL,CASE WHEN length(CAST(resolution AS BLOB))<=?2 THEN resolution END FROM parked WHERE id=?1",
                rusqlite::params![id,i64::try_from(limit).map_err(|_| Error::InvalidMaintenanceInput)?],
                |row| Ok((row.get(0)?,row.get(1)?)),
            )?;
            if resolved && resolution.is_none() {
                return Err(Error::InvalidMaintenanceInput);
            }
            let state = if resolution.is_some() {
                crate::PollIntentState::Confirmed
            } else {
                crate::PollIntentState::Uncertain
            };
            let mut machines = std::collections::HashSet::new();
            for machine in details.targets {
                if !machines.insert(machine.clone()) {
                    continue;
                }
                tx.execute(
                    "INSERT INTO poll_intents(machine,parked,state,reason) VALUES (?1,?2,?3,?4)",
                    rusqlite::params![machine.as_str(), id, state, details.reason.as_str()],
                )?;
                let intent = tx.last_insert_rowid();
                tx.execute("INSERT INTO poll_intent_history(intent,state,reason,audit) VALUES (?1,?2,?3,?4)",rusqlite::params![intent,state,details.reason.as_str(),resolution])?;
            }
            Ok(())
        })?;
        tx.execute_batch("PRAGMA user_version=9")?;
        Self::integrity(&tx)?;
        tx.commit()?;
        Ok(())
    }
}

// At most one bounded record is decoded at a time. A bad row aborts the whole migration.
fn migrate_rows(
    db: &Connection,
    table: &'static str,
    identity: &str,
    limit: usize,
    mut convert: impl FnMut(i64, &str) -> Result<(), Error>,
) -> Result<(), Error> {
    use rusqlite::OptionalExtension;
    let mut after: Option<i64> = None;
    let selection = format!(
        "SELECT {identity},CASE WHEN length(CAST(evidence AS BLOB))<=?2 THEN evidence END FROM {table}"
    );
    let first = format!("{selection} WHERE ?1 IS NULL ORDER BY {identity} LIMIT 1");
    let following = format!("{selection} WHERE {identity}>?1 ORDER BY {identity} LIMIT 1");
    loop {
        let sql = if after.is_none() { &first } else { &following };
        let row: Option<(i64, Option<String>)> = db
            .query_row(
                sql,
                rusqlite::params![
                    after,
                    i64::try_from(limit).map_err(|_| Error::InvalidMaintenanceInput)?
                ],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let Some((id, encoded)) = row else { break };
        let converted = encoded
            .ok_or(Error::InvalidMaintenanceInput)
            .and_then(|encoded| convert(id, &encoded));
        converted.map_err(|cause| Error::MigrationEvidence {
            table,
            row: id,
            cause: Box::new(cause),
        })?;
        after = Some(id);
    }
    Ok(())
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum LegacyParkingKind {
    InterpretationFailure,
    ResultReconciliation,
    Parked,
    Conflict,
    IncompleteTransfer,
}

fn parking(encoded: &str) -> Result<String, Error> {
    use crate::parked::{ParkingContext, ParkingEvidence};
    let value: serde_json::Value = serde_json::from_str(encoded)?;
    if value.is_object() {
        let evidence: ParkingEvidence = serde_json::from_value(value)?;
        return Ok(crate::document(&evidence)?);
    }
    let kind: LegacyParkingKind =
        serde_json::from_value(value.get(0).ok_or(Error::Corruption)?.clone())?;
    let string = |i| {
        value
            .get(i)
            .and_then(serde_json::Value::as_str)
            .ok_or(Error::Corruption)
    };
    let integer = |i| {
        value
            .get(i)
            .and_then(serde_json::Value::as_i64)
            .ok_or(Error::Corruption)
    };
    let target = match kind {
        LegacyParkingKind::IncompleteTransfer => string(1)?.to_owned(),
        LegacyParkingKind::InterpretationFailure
        | LegacyParkingKind::ResultReconciliation
        | LegacyParkingKind::Parked
        | LegacyParkingKind::Conflict => serde_json::from_str::<String>(string(1)?)?,
    };
    let (name, reason, context) = match kind {
        LegacyParkingKind::InterpretationFailure => (
            "interpretation_failure",
            string(2)?.to_owned(),
            value
                .get(3)
                .map(|_| integer(3).map(|version| ParkingContext::Interpretation { version }))
                .transpose()?,
        ),
        LegacyParkingKind::ResultReconciliation => (
            "result_reconciliation",
            format!("result reconciliation: {}", string(3)?),
            Some(ParkingContext::Result {
                version: value.get(4).map(|_| integer(4)).transpose()?,
                index: usize::try_from(integer(2)?).map_err(|_| Error::Corruption)?,
            }),
        ),
        LegacyParkingKind::Parked => ("parked", string(2)?.to_owned(), None),
        LegacyParkingKind::Conflict => ("conflict", string(2)?.to_owned(), None),
        LegacyParkingKind::IncompleteTransfer => {
            ("incomplete_transfer", string(2)?.to_owned(), None)
        }
    };
    Ok(crate::parked::evidence(name, &target, &reason, context)?)
}

#[cfg(test)]
mod tests;

/// Each entry advances exactly one version; SQL and evidence conversions share the same checks.
enum Step {
    Sql(&'static str),
    Evidence(fn(&mut Connection, usize) -> Result<(), Error>),
}

const MIGRATIONS: &[(i64, i64, Step)] = &[
    (0, 1, Step::Sql(include_str!("../schema.sql"))),
    (1, 2, Step::Evidence(|db, _| Store::migrate_v2(db))),
    (2, 3, Step::Evidence(Store::migrate_v3)),
    (3, 4, Step::Sql(include_str!("../migration_v4.sql"))),
    (4, 5, Step::Sql(include_str!("../migration_v5.sql"))),
    (5, 6, Step::Sql(include_str!("../migration_v6.sql"))),
    (6, 7, Step::Evidence(Store::migrate_v7)),
    (7, 8, Step::Sql(include_str!("../migration_v8.sql"))),
    (8, 9, Step::Evidence(Store::migrate_v9)),
    (9, 10, Step::Sql(include_str!("../migration_v10.sql"))),
    (10, 11, Step::Sql(include_str!("../migration_v11.sql"))),
    (11, 12, Step::Sql(include_str!("../migration_v12.sql"))),
    (12, 13, Step::Sql(include_str!("../migration_v13.sql"))),
    (13, 14, Step::Sql(include_str!("../migration_v14.sql"))),
    (14, 15, Step::Sql(include_str!("../migration_v15.sql"))),
];

pub(super) fn apply(
    db: &mut Connection,
    startup: &crate::startup::Startup,
    version: i64,
    limit: usize,
) -> Result<(), Error> {
    for &(from, to, ref step) in MIGRATIONS {
        if to <= version {
            continue;
        }
        startup.run(crate::StartupStep::Migration { from, to }, || {
            match step {
                Step::Sql(sql) => {
                    let tx = db.transaction()?;
                    tx.execute_batch(sql)?;
                    Store::integrity(&tx)?;
                    tx.commit()?;
                }
                Step::Evidence(convert) => convert(db, limit)?,
            }
            Store::integrity(db)
        })?;
    }
    Ok(())
}
