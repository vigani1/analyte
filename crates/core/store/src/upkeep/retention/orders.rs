//! Purges finished order bundles while retaining keyed identities that forbid machine replay.
use super::{DigestKey, PurgeRun, links};
use crate::{Error, MachineOrderKey, key};
use rusqlite::{Connection, OptionalExtension, params};

pub(crate) fn was_purged(
    db: &Connection,
    credential: Option<&DigestKey>,
    identity: &MachineOrderKey,
) -> Result<bool, Error> {
    let Some(credential) = credential else {
        let exists: bool =
            db.query_row("SELECT EXISTS(SELECT 1 FROM retention_orders)", [], |r| {
                r.get(0)
            })?;
        return if exists {
            Err(Error::RetentionKeyRequired)
        } else {
            Ok(false)
        };
    };
    let digest = credential.digest("machine-order", key(identity)?.as_bytes())?;
    Ok(db.query_row(
        "SELECT EXISTS(SELECT 1 FROM retention_orders WHERE identity_digest=?1)",
        [digest.as_slice()],
        |r| r.get(0),
    )?)
}
pub(super) fn batch(
    db: &Connection,
    credential: &DigestKey,
    run: &mut PurgeRun,
    limits: &crate::Limits,
) -> Result<(), Error> {
    let days = run.days.ok_or(Error::InvalidInput)?;
    let cutoff = run
        .time
        .checked_sub(
            i64::from(days)
                .checked_mul(86400)
                .ok_or(Error::InvalidInput)?,
        )
        .ok_or(Error::InvalidInput)?;
    let mut statement=db.prepare("SELECT id FROM machine_orders WHERE machine=?1 AND (?2 IS NULL OR id>?2) ORDER BY id LIMIT ?3")?;
    let candidates = statement
        .query_map(
            params![
                run.machine.as_str(),
                run.order_after,
                i64::try_from(limits.page_rows.min(16)).map_err(|_| Error::InvalidInput)?
            ],
            |r| r.get::<_, i64>(0),
        )?
        .collect::<Result<Vec<_>, _>>()?;
    run.examined = candidates.len();
    run.order_after = candidates.last().copied();
    for id in candidates {
        if eligible(db, id, cutoff, limits)? {
            let deliveries = purge(db, credential, id, run.time, limits.record_bytes)?;
            run.orders = run.orders.checked_add(1).ok_or(Error::InvalidInput)?;
            run.deliveries = run
                .deliveries
                .checked_add(deliveries)
                .ok_or(Error::InvalidInput)?;
            run.order_range = Some((run.order_range.map_or(id, |range| range.0), id));
        }
    }
    Ok(())
}
fn eligible(db: &Connection, id: i64, cutoff: i64, limits: &crate::Limits) -> Result<bool, Error> {
    let finished: bool = db.query_row(include_str!("orders.sql"), params![id, cutoff], |r| {
        r.get(0)
    })?;
    if !finished {
        return Ok(false);
    }
    let (destination, order): (String, String) = db.query_row(
        "SELECT destination,order_id FROM machine_orders WHERE id=?1",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    // Stream one UUID at a time; the enclosing SQLite deadline bounds the scan,
    // rather than making every deployment with more than one page retain all orders.
    let mut statement = db.prepare("SELECT id FROM receipts")?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        let encoded: String = row.get(0)?;
        let receipt = serde_json::from_str(&encoded)?;
        // Keep order evidence until linked or potentially linked receipts have finished retention.
        if links::order(db, receipt, &destination, &order, limits)? != links::Link::Separate {
            return Ok(false);
        }
    }
    Ok(true)
}
fn purge(
    db: &Connection,
    credential: &DigestKey,
    id: i64,
    now: i64,
    limit: usize,
) -> Result<usize, Error> {
    let (destination,order,machine,created,finalized):(String,String,String,i64,i64)=db.query_row("SELECT destination,order_id,machine,created_at,updated_at FROM machine_orders WHERE id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?)))?;
    let identity = MachineOrderKey {
        destination: analyte_model::Identifier::new(&destination).map_err(|_| Error::Corruption)?,
        order: analyte_model::Identifier::new(&order).map_err(|_| Error::Corruption)?,
        machine: analyte_model::Identifier::new(&machine).map_err(|_| Error::Corruption)?,
    };
    let identity_digest = credential.digest("machine-order", key(&identity)?.as_bytes())?;
    let order_digest = credential.digest("order", key(&(&destination, &order))?.as_bytes())?;
    let sample: Option<String> = db
        .query_row(
            "SELECT CASE WHEN length(CAST(sample AS BLOB))<=?3 THEN sample END FROM orders WHERE destination=?1 AND id=?2",
            params![destination, order, i64::try_from(limit).map_err(|_|Error::InvalidInput)?],
            |r| r.get(0),
        )
        .optional()?;
    let sample_digest = sample
        .map(|sample| credential.digest("sample", sample.as_bytes()))
        .transpose()?;
    let original: Option<String> = db
        .query_row(
            "SELECT CASE WHEN length(CAST(evidence AS BLOB))<=?3 THEN evidence END FROM orders WHERE destination=?1 AND id=?2",
            params![destination, order, i64::try_from(limit).map_err(|_|Error::InvalidInput)?],
            |r| r.get(0),
        )
        .optional()?;
    let original = original.map(|original| crate::digest(original.as_bytes()));
    db.execute(
        "INSERT INTO retention_key VALUES(1,?1) ON CONFLICT(singleton) DO NOTHING",
        [credential.fingerprint().as_slice()],
    )?;
    db.execute(
        "INSERT INTO retention_orders VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        params![
            id,
            identity_digest.as_slice(),
            machine,
            order_digest.as_slice(),
            sample_digest.as_ref().map(<[u8; 32]>::as_slice),
            original.as_ref().map(|d| d.0.as_slice()),
            created,
            finalized,
            now
        ],
    )?;
    let guards = remove_guards(db)?;
    db.execute(
        "DELETE FROM machine_order_item_history WHERE machine_order=?1",
        [id],
    )?;
    db.execute(
        "DELETE FROM machine_order_history WHERE machine_order=?1",
        [id],
    )?;
    db.execute(
        "DELETE FROM machine_order_items WHERE machine_order=?1",
        [id],
    )?;
    db.execute("DELETE FROM machine_orders WHERE id=?1", [id])?;
    let others: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM machine_orders WHERE destination=?1 AND order_id=?2)",
        params![destination, order],
        |r| r.get(0),
    )?;
    let mut deliveries = 0;
    if !others {
        deliveries=db.execute("INSERT INTO retention_order_deliveries SELECT o.id,?3,max(a.time) FROM outbox o JOIN attempts a ON a.delivery=o.id WHERE o.kind='order_event' AND o.destination=?1 AND o.order_id=?2 AND a.outcome='\"Done\"' GROUP BY o.id",params![destination,order,id])?;
        db.execute("DELETE FROM attempts WHERE delivery IN (SELECT id FROM outbox WHERE kind='order_event' AND destination=?1 AND order_id=?2)",params![destination,order])?;
        db.execute(
            "DELETE FROM outbox WHERE kind='order_event' AND destination=?1 AND order_id=?2",
            params![destination, order],
        )?;
        db.execute(
            "DELETE FROM orders WHERE destination=?1 AND id=?2",
            params![destination, order],
        )?;
    }
    for sql in guards {
        db.execute_batch(&sql)?;
    }
    Ok(deliveries)
}
fn remove_guards(db: &Connection) -> Result<Vec<String>, Error> {
    let mut guards = Vec::new();
    for name in [
        "machine_order_item_history_delete",
        "machine_order_history_delete",
        "machine_order_items_retained",
        "machine_orders_retained",
        "immutable_attempts_delete",
        "outbox_retained",
    ] {
        guards.push(
            db.query_row("SELECT sql FROM sqlite_master WHERE name=?1", [name], |r| {
                r.get::<_, String>(0)
            })?,
        );
        db.execute_batch(&format!("DROP TRIGGER {name}"))?;
    }
    Ok(guards)
}
