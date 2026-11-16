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
        if !(1..=3650).contains(&days) {
            return Err(Error::InvalidInput);
        }
        let cutoff = now
            .checked_sub(
                i64::from(days)
                    .checked_mul(86400)
                    .ok_or(Error::InvalidInput)?,
            )
            .ok_or(Error::InvalidInput)?;
        Ok(self.db.query_row(
            include_str!("eligibility.sql"),
            params![key(&receipt)?, cutoff],
            |r| r.get(0),
        )?)
    }
}
