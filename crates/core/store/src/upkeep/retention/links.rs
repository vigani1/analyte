//! Establishes receipt links from retained order and sample evidence, preserving uncertainty.
use crate::{Interpretation, MaintenanceError as Error, Reading, key};
use analyte_model::{IdentityStatus, ReceiptId};
use rusqlite::{Connection, OptionalExtension, params};

#[derive(PartialEq, Eq)]
pub(super) enum Link {
    Linked,
    Separate,
    Unknown,
}

pub(super) fn order(
    db: &Connection,
    receipt: ReceiptId,
    destination: &str,
    order: &str,
    limits: &crate::Limits,
) -> Result<Link, Error> {
    let sample: Option<String> = db
        .query_row(
            "SELECT CASE WHEN length(CAST(sample AS BLOB))<=?3 THEN sample END FROM orders o WHERE destination=?1 AND id=?2 AND CASE WHEN json_valid(json_extract(evidence,'$.document')) THEN NOT EXISTS(SELECT 1 FROM machine_orders m WHERE m.destination=o.destination AND m.order_id=o.id AND CAST(json_extract(json_extract(o.evidence,'$.document'),'$.version') AS TEXT) IS NOT m.version) ELSE 0 END",
            params![destination, order, i64::try_from(limits.record_bytes).map_err(|_|Error::InvalidMaintenanceInput)?],
            |r| r.get(0),
        )
        .optional()?;
    let mut statement = db.prepare("SELECT CASE WHEN length(CAST(evidence AS BLOB))<=?2 THEN evidence END FROM interpretations WHERE receipt=?1 LIMIT ?3")?;
    let mut rows = statement.query(params![
        key(&receipt)?,
        i64::try_from(limits.record_bytes).map_err(|_| Error::InvalidMaintenanceInput)?,
        i64::try_from(
            limits
                .page_rows
                .checked_add(1)
                .ok_or(Error::InvalidMaintenanceInput)?
        )
        .map_err(|_| Error::InvalidMaintenanceInput)?
    ])?;
    let mut unknown = false;
    let mut count = 0usize;
    while let Some(row) = rows.next()? {
        count = count.checked_add(1).ok_or(Error::InvalidMaintenanceInput)?;
        if count > limits.page_rows {
            return Ok(Link::Unknown);
        }
        let encoded: Option<String> = row.get(0)?;
        let Some(encoded) = encoded else {
            return Ok(Link::Unknown);
        };
        let evidence: Interpretation = serde_json::from_str(&encoded)?;
        let Reading::Results(results) = evidence.outcome else {
            return Ok(Link::Unknown);
        };
        if results.is_empty() {
            unknown = true;
        }
        for result in results {
            let IdentityStatus::Resolved(identity) = result.identity else {
                return Ok(Link::Unknown);
            };
            let source = identity.observation.order;
            let direct = source.namespace.as_str() == destination;
            let placer: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM machine_order_items i JOIN machine_orders m ON m.id=i.machine_order WHERE m.destination=?1 AND m.order_id=?2 AND i.placer=?3 AND i.placer_namespace=?4)",params![destination,order,source.value.as_str(),source.namespace.as_str()],|r|r.get(0))?;
            if placer || direct && source.value.as_str() == order {
                return Ok(Link::Linked);
            }
            if source.value.as_str() == order {
                unknown = true;
                continue;
            }
            match (result.sample.as_ref(), sample.as_deref()) {
                (Some(actual), Some(expected)) if actual.id.as_str() == expected => {
                    return Ok(Link::Linked);
                }
                (Some(_), Some(_)) => {}
                _ if direct => {}
                _ => unknown = true,
            }
        }
    }
    // Missing or incomplete evidence cannot prove that a hold or open operation is unrelated.
    Ok(if count == 0 || unknown {
        Link::Unknown
    } else {
        Link::Separate
    })
}

pub(super) fn blocked(
    db: &Connection,
    receipt: ReceiptId,
    machine: &str,
    limits: &crate::Limits,
) -> Result<bool, Error> {
    let mut statement = db.prepare("SELECT json_extract(target,'$[0]'),json_extract(target,'$[1]') FROM retention_holds WHERE kind='order' AND active=1 UNION SELECT destination,order_id FROM machine_orders m WHERE machine=?1 AND (state NOT IN ('Done','Rejected','CancelConfirmed') OR EXISTS(SELECT 1 FROM machine_order_items i WHERE i.machine_order=m.id AND i.state NOT IN ('Done','Rejected','CancelConfirmed'))) LIMIT ?2")?;
    let mut rows = statement.query(params![
        machine,
        i64::try_from(
            limits
                .page_rows
                .checked_add(1)
                .ok_or(Error::InvalidMaintenanceInput)?
        )
        .map_err(|_| Error::InvalidMaintenanceInput)?
    ])?;
    let mut count = 0usize;
    while let Some(row) = rows.next()? {
        count = count.checked_add(1).ok_or(Error::InvalidMaintenanceInput)?;
        if count > limits.page_rows
            || order(
                db,
                receipt,
                &row.get::<_, String>(0)?,
                &row.get::<_, String>(1)?,
                limits,
            )? != Link::Separate
        {
            return Ok(true);
        }
    }
    let mut statement=db.prepare("SELECT p.evidence FROM poll_intents i JOIN parked p ON p.id=i.parked WHERE i.machine=?1 AND i.state!='confirmed' LIMIT ?2")?;
    let mut rows = statement.query(params![
        machine,
        i64::try_from(
            limits
                .page_rows
                .checked_add(1)
                .ok_or(Error::InvalidMaintenanceInput)?
        )
        .map_err(|_| Error::InvalidMaintenanceInput)?
    ])?;
    let mut count = 0usize;
    while let Some(row) = rows.next()? {
        count = count.checked_add(1).ok_or(Error::InvalidMaintenanceInput)?;
        if count > limits.page_rows {
            return Ok(true);
        }
        let evidence: String = row.get(0)?;
        if evidence.len() > limits.record_bytes {
            return Ok(true);
        }
        let details = crate::ParkedItem {
            id: 0,
            evidence,
            resolution: None,
        }
        .details()?;
        let mut concrete = false;
        for target in details.targets {
            if target.as_str() == machine {
                continue;
            }
            let id = serde_json::from_value::<ReceiptId>(serde_json::Value::String(
                target.as_str().into(),
            ));
            let Ok(id) = id else {
                return Ok(true);
            };
            if id == receipt {
                return Ok(true);
            }
            let exists:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM receipts WHERE id=?1 UNION SELECT 1 FROM retention_tombstones WHERE receipt=?1)",[key(&id)?],|r|r.get(0))?;
            if !exists {
                return Ok(true);
            }
            concrete = true;
        }
        // Legacy machine-only poll intents have no trustworthy per-receipt scope.
        if !concrete {
            return Ok(true);
        }
    }
    Ok(false)
}
