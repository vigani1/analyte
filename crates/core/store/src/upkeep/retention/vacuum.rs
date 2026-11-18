//! Checks for incremental vacuum and returns a bounded number of free pages online.
use crate::{MaintenanceError as Error, Store};
use std::time::Instant;
impl Store {
    /// Refuses finite retention when the file cannot return space incrementally.
    /// Every store this build creates can; the check fails closed on any other file.
    /// # Errors
    /// Names the missing storage mode; reports database failures.
    pub fn retention_space_ready(&self) -> Result<(), Error> {
        let mode: i64 = self.db.query_row("PRAGMA auto_vacuum", [], |r| r.get(0))?;
        if mode != 2 {
            return Err(Error::RetentionVacuumRequired);
        }
        Ok(())
    }
    /// Returns up to 32 free pages to the filesystem, without waiting for readers.
    /// A busy checkpoint leaves reclamation for the next run; protected content is unchanged.
    /// # Errors
    /// Refuses unsupported files, maintenance contention, deadlines and SQLite failures.
    pub fn reclaim_retention_space(&mut self, deadline: Instant) -> Result<(), Error> {
        if Instant::now() >= deadline {
            return Err(Error::Timeout);
        }
        let _maintenance = super::maintenance(&self.path, true)?;
        self.retention_space_ready()?;
        self.db.busy_timeout(std::time::Duration::ZERO)?;
        let interrupted = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = interrupted.clone();
        self.db.progress_handler(
            100,
            Some(move || {
                Instant::now() >= deadline && !flag.swap(true, std::sync::atomic::Ordering::AcqRel)
            }),
        )?;
        let result = self
            .db
            .execute_batch("PRAGMA incremental_vacuum(32); PRAGMA wal_checkpoint(TRUNCATE);");
        self.db.progress_handler(0, None::<fn() -> bool>)?;
        self.db.busy_timeout(self.limits.busy_timeout)?;
        if interrupted.load(std::sync::atomic::Ordering::Acquire) {
            return Err(Error::Timeout);
        }
        result.map_err(Error::from)
    }
}
