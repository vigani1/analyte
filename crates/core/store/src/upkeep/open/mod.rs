//! Opens the store under its lock, checks integrity and applies forward-only migrations.
use crate::{
    Error, FreeSpace, Limits, SCHEMA_VERSION, Store,
    startup::{Startup, StartupStep},
};
use rusqlite::Connection;
use std::path::Path;

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
        if version == 0 {
            startup.run(StartupStep::Migration { from: 0, to: 1 }, || {
                let tx = db.transaction()?;
                tx.execute_batch(include_str!("../schema.sql"))?;
                tx.commit()?;
                Ok(())
            })?;
        }
        if version <= 1 {
            startup.run(StartupStep::Migration { from: 1, to: 2 }, || {
                Self::migrate_v2(&mut db)?;
                Ok(())
            })?;
        }
        if version <= 2 {
            startup.run(StartupStep::Migration { from: 2, to: 3 }, || {
                Self::migrate_v3(&mut db, limits.record_bytes)?;
                Ok(())
            })?;
        }
        if version <= 3 {
            startup.run(StartupStep::Migration { from: 3, to: 4 }, || {
                let tx = db.transaction()?;
                tx.execute_batch(include_str!("../migration_v4.sql"))?;
                tx.commit()?;
                Ok(())
            })?;
        }
        if version <= 4 {
            startup.run(StartupStep::Migration { from: 4, to: 5 }, || {
                let tx = db.transaction()?;
                tx.execute_batch(include_str!("../migration_v5.sql"))?;
                tx.commit()?;
                Ok(())
            })?;
        }
        if version <= 5 {
            startup.run(StartupStep::Migration { from: 5, to: 6 }, || {
                let tx = db.transaction()?;
                tx.execute_batch(include_str!("../migration_v6.sql"))?;
                tx.commit()?;
                Ok(())
            })?;
        }
        if version <= 6 {
            startup.run(StartupStep::Migration { from: 6, to: 7 }, || {
                Self::migrate_v7(&mut db, limits.record_bytes)?;
                Ok(())
            })?;
        }
        if version <= 7 {
            startup.run(StartupStep::Migration { from: 7, to: 8 }, || {
                let tx = db.transaction()?;
                tx.execute_batch(include_str!("../migration_v8.sql"))?;
                Self::integrity(&tx)?;
                tx.commit()?;
                Ok(())
            })?;
        }
        if version <= 8 {
            startup.run(StartupStep::Migration { from: 8, to: 9 }, || {
                Self::migrate_v9(&mut db, limits.record_bytes)?;
                Ok(())
            })?;
        }
        if version <= 9 {
            migrate_retention(&mut db, startup)?;
        }
        if version <= 10 {
            migrate_orders(&mut db, startup)?;
        }
        migrate_alerts(&mut db, startup, version)?;
        install_guards(&db, startup)?;
        db.progress_handler(0, None::<fn() -> bool>)?;
        Ok(Self {
            retention_key: None,
            _lock: lock,
            db,
            path: path.into(),
            limits,
            space,
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
            .ok_or(Error::InvalidInput)?,
    ))
}

#[cfg(test)]
mod tests;

fn migrate_retention(db: &mut Connection, startup: &Startup) -> Result<(), Error> {
    startup.run(StartupStep::Migration { from: 9, to: 10 }, || {
        let tx = db.transaction()?;
        tx.execute_batch(include_str!("../migration_v10.sql"))?;
        Store::integrity(&tx)?;
        tx.commit()?;
        Ok(())
    })
}

fn migrate_orders(db: &mut Connection, startup: &Startup) -> Result<(), Error> {
    startup.run(StartupStep::Migration { from: 10, to: 11 }, || {
        let tx = db.transaction()?;
        tx.execute_batch(include_str!("../migration_v11.sql"))?;
        Store::integrity(&tx)?;
        tx.commit()?;
        Ok(())
    })
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
fn migrate_alerts(db: &mut Connection, startup: &Startup, version: i64) -> Result<(), Error> {
    if version > 11 {
        return Ok(());
    }
    startup.run(StartupStep::Migration { from: 11, to: 12 }, || {
        let tx = db.transaction()?;
        tx.execute_batch(include_str!("../migration_v12.sql"))?;
        Store::integrity(&tx)?;
        tx.commit()?;
        Ok(())
    })
}
