//! Open storage responsibilities.
use crate::{Error, FreeSpace, Limits, SCHEMA_VERSION, Store};
use rusqlite::Connection;
use std::{path::Path, time::Duration};

impl Store {
    /// Opens, checks integrity, and applies forward-only migrations before accepting writes.
    /// # Errors
    /// Refuses corruption, newer schemas, invalid limits and filesystem/database failures.
    pub fn open(path: &Path, limits: Limits, space: Box<dyn FreeSpace>) -> Result<Self, Error> {
        if limits.record_bytes == 0
            || limits.page_rows == 0
            || limits.busy_timeout.is_zero()
            || limits.busy_timeout > Duration::from_secs(30)
        {
            return Err(Error::InvalidInput);
        }
        let lock = Self::lock(path, false)?;
        if path.with_extension("restore-pending").exists() {
            return Err(Error::Conflict);
        }
        Self::open_locked(path, limits, space, lock)
    }

    pub(super) fn open_locked(
        path: &Path,
        limits: Limits,
        space: Box<dyn FreeSpace>,
        lock: std::fs::File,
    ) -> Result<Self, Error> {
        let mut db = Connection::open(path)?;
        db.busy_timeout(limits.busy_timeout)?;
        let version: i64 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version > SCHEMA_VERSION {
            return Err(Error::SchemaTooNew(version));
        }
        Self::integrity(&db)?;
        db.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON;",
        )?;
        if version == 0 {
            let tx = db.transaction()?;
            tx.execute_batch(include_str!("schema.sql"))?;
            tx.commit()?;
        }
        if version <= 1 {
            Self::migrate_v2(&mut db)?;
        }
        if version <= 2 {
            Self::migrate_v3(&mut db, limits.record_bytes)?;
        }
        if version <= 3 {
            let tx = db.transaction()?;
            tx.execute_batch(include_str!("migration_v4.sql"))?;
            tx.commit()?;
        }
        if version <= 4 {
            let tx = db.transaction()?;
            tx.execute_batch(include_str!("migration_v5.sql"))?;
            tx.commit()?;
        }
        if version <= 5 {
            let tx = db.transaction()?;
            tx.execute_batch(include_str!("migration_v6.sql"))?;
            tx.commit()?;
        }
        if version <= 6 {
            Self::migrate_v7(&mut db, limits.record_bytes)?;
        }
        if version <= 7 {
            let tx = db.transaction()?;
            tx.execute_batch(include_str!("migration_v8.sql"))?;
            Self::integrity(&tx)?;
            tx.commit()?;
        }
        if version <= 8 {
            Self::migrate_v9(&mut db, limits.record_bytes)?;
        }
        // SQL constraints also protect evidence from accidental maintenance updates.
        for table in ["receipts", "frames", "interpretations", "attempts", "audit"] {
            db.execute_batch(&format!("CREATE TRIGGER IF NOT EXISTS immutable_{table}_update BEFORE UPDATE ON {table} BEGIN SELECT RAISE(ABORT,'immutable evidence'); END; CREATE TRIGGER IF NOT EXISTS immutable_{table}_delete BEFORE DELETE ON {table} BEGIN SELECT RAISE(ABORT,'immutable evidence'); END;"))?;
        }
        Ok(Self {
            _lock: lock,
            db,
            path: path.into(),
            limits,
            space,
        })
    }

    pub(super) fn integrity(db: &Connection) -> Result<(), Error> {
        let result: String = db.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
        if result != "ok" {
            return Err(Error::Corruption);
        }
        let foreign_error = db.prepare("PRAGMA foreign_key_check")?.exists([])?;
        if foreign_error {
            return Err(Error::Corruption);
        }
        Ok(())
    }
}
