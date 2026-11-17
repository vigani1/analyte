//! Prepares incremental vacuum offline and returns a bounded number of free pages online.
use crate::{FreeSpace, Limits, MaintenanceError as Error, Store, startup::Startup};
use std::{path::Path, time::Instant};
impl Store {
    /// Refuses finite retention until the file supports incremental vacuum.
    /// # Errors
    /// Names the required offline conversion; reports database failures.
    pub fn retention_space_ready(&self) -> Result<(), Error> {
        let mode: i64 = self.db.query_row("PRAGMA auto_vacuum", [], |r| r.get(0))?;
        if mode != 2 {
            return Err(Error::RetentionVacuumRequired);
        }
        Ok(())
    }
    /// Converts an existing file under exclusive offline ownership, before reopening machines.
    /// SQLite VACUUM is atomic; interruption preserves the original store. Earlier schema
    /// migrations may have committed. The deadline also bounds integrity checks and conversion.
    /// # Errors
    /// Refuses active owners, insufficient capacity, timeout, corruption and I/O failures.
    pub fn prepare_retention_space(
        path: &Path,
        limits: Limits,
        space: Box<dyn FreeSpace>,
        deadline: Instant,
    ) -> Result<(), Error> {
        let lock = Self::lock(path, true)?;
        let startup = Startup::new(deadline);
        let mut store = Self::open_locked_starting(path, limits, space, lock, &startup)?;
        startup.install(&mut store.db)?;
        let outcome = startup.run(crate::StartupStep::Finalizing, || {
            let bytes: i64 = store.db.query_row(
                "SELECT page_count*page_size FROM pragma_page_count(),pragma_page_size()",
                [],
                |r| r.get(0),
            )?;
            store.capacity(0)?;
            let needed = u64::try_from(bytes)
                .ok()
                .and_then(|bytes| bytes.checked_mul(2))
                .and_then(|bytes| bytes.checked_add(store.limits.reserve_bytes))
                .ok_or(Error::Capacity)?;
            if store.space.available(path)? <= needed {
                return Err(Error::Capacity);
            }
            store
                .db
                .execute_batch("PRAGMA auto_vacuum=INCREMENTAL; VACUUM;")?;
            store.retention_space_ready()
        });
        store.db.progress_handler(0, None::<fn() -> bool>)?;
        outcome
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
