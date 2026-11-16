//! Checks every receipt retention condition in one fail-closed store rule.
use crate::{Error, Store, key};
use analyte_model::ReceiptId;
use rusqlite::params;

impl Store {
    /// Reports whether every result and linked operation is safely finished (ADR 0013 item 2).
    /// Unknown acknowledgments, failed interpretations and ambiguous operation links retain data.
    /// # Errors
    /// Refuses invalid periods and propagates database errors rather than treating them as eligibility.
    pub fn retention_eligible(
        &self,
        receipt: ReceiptId,
        days: u16,
        now: i64,
    ) -> Result<bool, Error> {
        eligible(&self.db, receipt, days, now, &self.limits)
    }
}
pub(super) fn eligible(
    db: &rusqlite::Connection,
    receipt: ReceiptId,
    days: u16,
    now: i64,
    limits: &crate::Limits,
) -> Result<bool, Error> {
    if !(1..=3650).contains(&days) {
        return Err(Error::InvalidMaintenanceInput);
    }
    let cutoff = now
        .checked_sub(
            i64::from(days)
                .checked_mul(86400)
                .ok_or(Error::InvalidMaintenanceInput)?,
        )
        .ok_or(Error::InvalidMaintenanceInput)?;
    let finished: bool = db.query_row(
        include_str!("eligibility.sql"),
        params![key(&receipt)?, cutoff],
        |r| r.get(0),
    )?;
    if !finished {
        return Ok(false);
    }
    let machine: String = db.query_row(
        "SELECT json_extract(metadata,'$.machine') FROM receipts WHERE id=?1",
        [key(&receipt)?],
        |r| r.get(0),
    )?;
    Ok(!super::links::blocked(db, receipt, &machine, limits)?)
}
