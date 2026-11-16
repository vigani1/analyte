//! Restores offline stores while retaining the displaced database and pausing machine commands.
use crate::{Audit, Error, FreeSpace, Limits, Store};
use rusqlite::{Connection, OptionalExtension};
use std::{
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
    time::Instant,
};

impl Store {
    pub(crate) fn lock(path: &Path, exclusive: bool) -> Result<File, Error> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path.with_extension("lock"))?;
        let result = if exclusive {
            file.try_lock()
        } else {
            file.try_lock_shared()
        };
        result.map_err(|error| match error {
            std::fs::TryLockError::WouldBlock => Error::InUse,
            std::fs::TryLockError::Error(error) => Error::Io(error),
        })?;
        Ok(file)
    }

    /// Verifies and migrates a staged backup, retaining the displaced database.
    /// The restore audit is committed before publication. Interrupted staging is
    /// retained and blocks another restore rather than deleting evidence.
    /// # Errors
    /// Refuses active owners, existing recovery files, invalid backups and I/O failures.
    pub fn restore(
        path: &Path,
        source: &Path,
        limits: Limits,
        space: Box<dyn FreeSpace>,
        audit: &Audit,
        deadline: Instant,
    ) -> Result<PathBuf, Error> {
        if audit.action.as_str() != "store-restore" {
            return Err(Error::AuditMismatch);
        }
        limits.validate()?;
        let lock = Self::lock(path, true)?;
        let stage = path.with_extension("restore-pending");
        let kept = path.with_extension("before-restore");
        if kept.exists() || stage.exists() {
            return Err(Error::Conflict);
        }
        let source_db =
            Connection::open_with_flags(source, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        Self::integrity(&source_db)?;
        let version: i64 = source_db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version > crate::SCHEMA_VERSION {
            return Err(Error::SchemaTooNew(version));
        }
        if version < 1 {
            return Err(Error::Corruption);
        }
        Self::restore_capacity(&source_db, path, &limits, &*space)?;
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&stage)?;
        drop(file);
        if let Err(error) =
            Self::fill_stage(&source_db, &stage, limits, space, &lock, audit, deadline)
        {
            // A refusal or failure seen here leaves the current store and the backup
            // untouched; removing our own partial copy keeps startup available. Only a
            // crash leaves the stage behind, and startup then refuses until reviewed.
            for leftover in [
                stage.clone(),
                stage.with_extension("restore-pending-wal"),
                stage.with_extension("restore-pending-shm"),
            ] {
                if let Err(cleanup) = std::fs::remove_file(&leftover)
                    && cleanup.kind() != std::io::ErrorKind::NotFound
                {
                    return Err(cleanup.into());
                }
            }
            return Err(error);
        }
        OpenOptions::new().write(true).open(&stage)?.sync_all()?;
        if path.exists() {
            let current = Connection::open(path)?;
            current
                .execute_batch("PRAGMA wal_checkpoint(TRUNCATE); PRAGMA journal_mode=DELETE;")?;
            drop(current);
            std::fs::rename(path, &kept)?;
        }
        std::fs::rename(&stage, path)?;
        drop(lock);
        Ok(kept)
    }

    /// Copies the backup into the stage, opens it through ordinary startup (which may
    /// migrate it), records the audit entry and admits the final size.
    fn fill_stage(
        source_db: &Connection,
        stage: &Path,
        limits: Limits,
        space: Box<dyn FreeSpace>,
        lock: &File,
        audit: &Audit,
        deadline: Instant,
    ) -> Result<(), Error> {
        let mut copy = Connection::open(stage)?;
        {
            let backup = rusqlite::backup::Backup::new(source_db, &mut copy)?;
            loop {
                if Instant::now() >= deadline {
                    return Err(Error::Timeout);
                }
                match backup.step(64)? {
                    rusqlite::backup::StepResult::Done => break,
                    rusqlite::backup::StepResult::More => {}
                    rusqlite::backup::StepResult::Busy | rusqlite::backup::StepResult::Locked => {
                        return Err(Error::Timeout);
                    }
                    _ => return Err(Error::Corruption), // Future SQLite states fail closed.
                }
            }
        }
        drop(copy);
        let mut restored = Self::open_locked(stage, limits, space, lock.try_clone()?)?;
        restored.audit(audit)?;
        restored
            .db
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE); PRAGMA journal_mode=DELETE;")?;
        // Opening the stage may migrate an older backup and grow it. Admit the
        // final restored store under the same reserve and size rule as intake.
        restored.capacity(0)?;
        drop(restored);
        Ok(())
    }

    /// Reports whether the most recent restore still needs operator reconciliation.
    /// # Errors
    /// Reports database failures; unknown evidence never authorizes a command.
    pub fn restore_unreconciled(&self) -> Result<bool, Error> {
        Self::restore_paused(&self.db)
    }

    pub(crate) fn restore_paused(db: &Connection) -> Result<bool, Error> {
        let action: Option<String> = db.query_row("SELECT json_extract(evidence,'$.action') FROM audit WHERE json_extract(evidence,'$.action') IN ('store-restore','orders-resume-after-restore') ORDER BY sequence DESC LIMIT 1", [], |r| r.get(0)).optional()?;
        Ok(action.as_deref() == Some("store-restore"))
    }

    /// Audits the operator's reconciliation and releases the restore pause atomically.
    /// # Errors
    /// Refuses a wrong audit action or a store that is not paused.
    pub fn resume_after_restore(&mut self, audit: &Audit) -> Result<(), Error> {
        if audit.action.as_str() != "orders-resume-after-restore" {
            return Err(Error::AuditMismatch);
        }
        let encoded = self.encode(audit)?;
        self.capacity(encoded.len())?;
        let tx = self
            .db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        if !Self::restore_paused(&tx)? {
            return Err(Error::Conflict);
        }
        tx.execute("INSERT INTO audit(evidence) VALUES (?1)", [encoded])?;
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
