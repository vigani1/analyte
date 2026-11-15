//! Capacity storage responsibilities.
use crate::{Error, FreeSpace, Limits, Store};
use rusqlite::Connection;
use std::path::Path;

impl Store {
    pub(super) fn bound(&self, bytes: &[u8]) -> Result<(), Error> {
        if bytes.len() > self.limits.record_bytes {
            return Err(Error::InvalidInput);
        }
        Ok(())
    }

    pub(super) fn capacity(&self, bytes: usize) -> Result<(), Error> {
        Self::capacity_on(&self.db, &self.path, &self.limits, &*self.space, bytes)
    }

    pub(super) fn capacity_on(
        db: &Connection,
        path: &Path,
        limits: &Limits,
        space: &dyn FreeSpace,
        bytes: usize,
    ) -> Result<(), Error> {
        let input = u64::try_from(bytes).map_err(|_| Error::Capacity)?;
        let needed = limits
            .reserve_bytes
            .checked_add(input)
            .ok_or(Error::Capacity)?;
        let pages: u32 = db.query_row("PRAGMA page_count", [], |r| r.get(0))?;
        let size: u32 = db.query_row("PRAGMA page_size", [], |r| r.get(0))?;
        if space.available(path)? <= needed
            || u64::from(pages)
                .checked_mul(u64::from(size))
                .and_then(|n| n.checked_add(input))
                .ok_or(Error::Capacity)?
                >= limits.database_bytes
        {
            return Err(Error::Capacity);
        }
        Ok(())
    }

    /// Admission for a restore: the staged copy of `source` must fit above the
    /// disk reserve beside the current store, and the restored store must stay
    /// under the database size limit. The same reserve and limit as ordinary
    /// admission, so restore cannot publish what normal intake would refuse.
    pub(super) fn restore_capacity(
        source: &Connection,
        path: &Path,
        limits: &Limits,
        space: &dyn FreeSpace,
    ) -> Result<(), Error> {
        let pages: u32 = source.query_row("PRAGMA page_count", [], |r| r.get(0))?;
        let size: u32 = source.query_row("PRAGMA page_size", [], |r| r.get(0))?;
        let copy = u64::from(pages)
            .checked_mul(u64::from(size))
            .ok_or(Error::Capacity)?;
        let needed = limits
            .reserve_bytes
            .checked_add(copy)
            .ok_or(Error::Capacity)?;
        if space.available(path)? <= needed || copy >= limits.database_bytes {
            return Err(Error::Capacity);
        }
        Ok(())
    }

    /// Checks, without writing anything, whether new evidence of up to
    /// `expected_bytes` would be admitted under the disk reserve and the
    /// database size limit. Dialogs call this before accepting a new transfer
    /// (before permitting a peer to begin a transfer, ADR 0003). It is a snapshot:
    /// every later write checks capacity again and can still refuse.
    /// # Errors
    /// Returns `Error::Capacity` when admission would cross a limit, or the
    /// specific error of a failed free-space probe.
    pub fn admission(&self, expected_bytes: usize) -> Result<(), Error> {
        self.capacity(expected_bytes)
    }
}
