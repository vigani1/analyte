//! Opens the store under its lock, checks integrity and applies forward-only migrations.
use crate::{
    FreeSpace, Limits, MaintenanceError as Error, SCHEMA_VERSION, Store,
    startup::{Startup, StartupStep},
};
use rusqlite::Connection;
use std::path::Path;
#[cfg(test)]
mod tests;

impl Store {
    /// Opens, checks integrity, and applies forward-only migrations before accepting writes.
    /// Uses `DEFAULT_STARTUP_TIMEOUT`; each schema version commits atomically.
    /// # Errors
    /// Refuses corruption, newer schemas, invalid limits and filesystem/database failures,
    /// and names the active step when the startup budget expires.
    pub fn open(path: &Path, limits: Limits, space: Box<dyn FreeSpace>) -> Result<Self, Error> {
        Self::open_with_startup(path, limits, space, &default_startup()?)
    }

    pub(crate) fn open_with_startup(
        path: &Path,
        limits: Limits,
        space: Box<dyn FreeSpace>,
        startup: &Startup,
    ) -> Result<Self, Error> {
        let result = Self::open_starting(path, limits, space, startup);
        if startup.expired() {
            Err(startup.error())
        } else {
            result
        }
    }

    fn open_starting(
        path: &Path,
        limits: Limits,
        space: Box<dyn FreeSpace>,
        startup: &Startup,
    ) -> Result<Self, Error> {
        startup.check()?;
        limits.validate()?;
        let lock = Self::lock(path, false)?;
        if path.with_extension("restore-pending").exists() {
            return Err(Error::Conflict);
        }
        Self::open_locked_starting(path, limits, space, lock, startup)
    }

    pub(crate) fn open_locked(
        path: &Path,
        limits: Limits,
        space: Box<dyn FreeSpace>,
        lock: std::fs::File,
    ) -> Result<Self, Error> {
        Self::open_locked_starting(path, limits, space, lock, &default_startup()?)
    }

    pub(crate) fn open_locked_starting(
        path: &Path,
        limits: Limits,
        space: Box<dyn FreeSpace>,
        lock: std::fs::File,
        startup: &Startup,
    ) -> Result<Self, Error> {
        let (mut db, version) = startup.run(StartupStep::Opening, || {
            let mut db = Connection::open(path)?;
            startup.install(&mut db)?;
            db.busy_timeout(limits.busy_timeout)?;
            // A deferred transaction that reads and then writes gets SQLITE_BUSY
            // at once, without waiting, when another connection (an operator
            // command) is writing. Taking the write lock at BEGIN lets busy_timeout
            // bound every wait instead.
            db.set_transaction_behavior(rusqlite::TransactionBehavior::Immediate);
            let version: i64 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
            if version == 0 {
                db.execute_batch("PRAGMA auto_vacuum=INCREMENTAL")?;
            }
            Ok((db, version))
        })?;
        if version > SCHEMA_VERSION {
            return Err(Error::SchemaTooNew(version));
        }
        startup.run(StartupStep::Integrity, || Self::integrity(&db))?;
        db.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON; PRAGMA secure_delete=ON;",
        )?;
        super::migration::apply(&mut db, startup, version, limits.record_bytes)?;
        install_guards(&db, startup)?;
        db.progress_handler(0, None::<fn() -> bool>)?;
        Ok(Self {
            retention_key: None,
            db,
            path: path.into(),
            limits,
            space,
            _lock: lock,
        })
    }

    pub(crate) fn integrity(db: &Connection) -> Result<(), Error> {
        tracing::info!(step = "integrity check", "store integrity check started");
        let started = std::time::Instant::now();
        let outcome = (|| {
            let result: String = db.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
            if result != "ok" {
                return Err(Error::Corruption);
            }
            let foreign_error = db.prepare("PRAGMA foreign_key_check")?.exists([])?;
            if foreign_error {
                return Err(Error::Corruption);
            }
            Ok(())
        })();
        tracing::info!(
            step = "integrity check",
            elapsed_ms = started.elapsed().as_millis(),
            succeeded = outcome.is_ok(),
            "store integrity check finished"
        );
        outcome
    }
}

fn default_startup() -> Result<Startup, Error> {
    Ok(Startup::new(
        std::time::Instant::now()
            .checked_add(crate::DEFAULT_STARTUP_TIMEOUT)
            .ok_or(Error::InvalidMaintenanceInput)?,
    ))
}

fn install_guards(db: &Connection, startup: &Startup) -> Result<(), Error> {
    startup.run(StartupStep::Finalizing, || {
        // SQL constraints also protect evidence from accidental maintenance updates.
        for table in ["receipts", "frames", "interpretations", "attempts", "audit"] {
            db.execute_batch(&format!("CREATE TRIGGER IF NOT EXISTS immutable_{table}_update BEFORE UPDATE ON {table} BEGIN SELECT RAISE(ABORT,'immutable evidence'); END; CREATE TRIGGER IF NOT EXISTS immutable_{table}_delete BEFORE DELETE ON {table} BEGIN SELECT RAISE(ABORT,'immutable evidence'); END;"))?;
        }
            Ok(())
        })
}
