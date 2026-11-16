//! Creates protected database backups.
use crate::{Error, Store};
use rusqlite::Connection;
use std::{path::Path, time::Instant};

impl Store {
    /// Uses SQLite online backup, then checks the restored copy before reporting success.
    /// # Errors
    /// Refuses an existing target, timeout, corruption or database/filesystem failures.
    pub fn backup(&self, target: &Path, deadline: Instant) -> Result<(), Error> {
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(target)?;
        drop(file);
        let mut copy = Connection::open(target)?;
        {
            let backup = rusqlite::backup::Backup::new(&self.db, &mut copy)?;
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
        Self::integrity(&copy)
    }
}

#[cfg(test)]
mod tests;
