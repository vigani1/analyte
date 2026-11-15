//! Recoverable file handoff. Call blocking operations on a dedicated storage thread.

use std::{
    fs::{self, File},
    io::{self, Read},
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime},
};
use thiserror::Error;

/// File operations preserve the input on failure.
#[derive(Debug, Error)]
pub enum Error {
    /// Original filesystem failure and affected path.
    #[error("file operation at {path}: {source}")]
    Io {
        /// Affected path.
        path: PathBuf,
        /// Original filesystem failure.
        source: io::Error,
    },
    /// The input exceeds the configured bound; it stays claimed for investigation.
    #[error("file exceeds size limit: {0}")]
    Oversized(PathBuf),
    /// A path is not a regular file or a setting is invalid.
    #[error("invalid file handoff setting or input: {0}")]
    Invalid(PathBuf),
}

fn io_error(path: &Path, source: io::Error) -> Error {
    Error::Io {
        path: path.into(),
        source,
    }
}

/// Observation of one producer file, used to establish a stable interval.
pub struct Observation {
    path: PathBuf,
    size: u64,
    modified: SystemTime,
    since: Instant,
}

impl Observation {
    /// Starts watching a regular file without following a symbolic link.
    ///
    /// # Errors
    /// Reports missing metadata, unsupported modification times or nonregular input.
    pub fn new(path: PathBuf) -> Result<Self, Error> {
        let metadata = fs::symlink_metadata(&path).map_err(|e| io_error(&path, e))?;
        if !metadata.is_file() {
            return Err(Error::Invalid(path));
        }
        let modified = metadata.modified().map_err(|e| io_error(&path, e))?;
        Ok(Self {
            path,
            size: metadata.len(),
            modified,
            since: Instant::now(),
        })
    }

    /// Returns true only after unchanged size and modification time for the full interval.
    /// The producer must stop writing after this interval; rename handoff is preferred.
    ///
    /// # Errors
    /// Reports metadata failure or replacement with a nonregular file.
    pub fn ready(&mut self, stable: Duration) -> Result<bool, Error> {
        if stable.is_zero() {
            return Err(Error::Invalid(self.path.clone()));
        }
        let current = Self::new(self.path.clone())?;
        if current.size != self.size || current.modified != self.modified {
            *self = current;
            return Ok(false);
        }
        Ok(self.since.elapsed() >= stable)
    }
}

/// A claimed file retained until explicitly archived or quarantined.
/// On a crash the file remains in the claimed directory and can be recovered.
pub struct Claimed {
    path: PathBuf,
    max_bytes: u64,
}

impl Claimed {
    /// Atomically claims a completed input into a fresh private subdirectory.
    /// Source and claimed area must be on the same filesystem; rename failure leaves input intact.
    /// The caller must establish completion by producer rename or `Observation` first.
    ///
    /// # Errors
    /// Rejects nonregular inputs, zero limits, destination collisions and filesystem errors.
    pub fn claim(source: &Path, destination: &Path, max_bytes: u64) -> Result<Self, Error> {
        if !(1..=16_777_216).contains(&max_bytes) {
            return Err(Error::Invalid(source.into()));
        }
        let metadata = fs::symlink_metadata(source).map_err(|e| io_error(source, e))?;
        if !metadata.is_file() {
            return Err(Error::Invalid(source.into()));
        }
        // A fresh directory prevents rename from replacing an existing receipt on Unix.
        fs::create_dir(destination).map_err(|e| io_error(destination, e))?;
        let path = destination.join("input");
        fs::rename(source, &path).map_err(|e| io_error(source, e))?;
        Ok(Self { path, max_bytes })
    }

    /// Reopens the input in a claimed receipt directory without moving or deleting it.
    ///
    /// # Errors
    /// Rejects invalid limits, missing files and nonregular input.
    pub fn recover(directory: &Path, max_bytes: u64) -> Result<Self, Error> {
        let path = directory.join("input");
        if !(1..=16_777_216).contains(&max_bytes)
            || !fs::symlink_metadata(&path)
                .map_err(|e| io_error(&path, e))?
                .is_file()
        {
            return Err(Error::Invalid(path));
        }
        Ok(Self { path, max_bytes })
    }

    /// Reads exact file bytes with an allocation bound, including concurrent growth detection.
    ///
    /// # Errors
    /// Oversized inputs stay claimed; all read failures preserve the file.
    pub fn read(&self) -> Result<Vec<u8>, Error> {
        let file = File::open(&self.path).map_err(|e| io_error(&self.path, e))?;
        let limit = self
            .max_bytes
            .checked_add(1)
            .ok_or_else(|| Error::Invalid(self.path.clone()))?;
        let mut bytes = Vec::new();
        file.take(limit)
            .read_to_end(&mut bytes)
            .map_err(|e| io_error(&self.path, e))?;
        if u64::try_from(bytes.len()).map_err(|_| Error::Oversized(self.path.clone()))?
            > self.max_bytes
        {
            return Err(Error::Oversized(self.path.clone()));
        }
        Ok(bytes)
    }

    /// Moves the whole receipt to a fresh archive or quarantine directory, never deleting input.
    ///
    /// # Errors
    /// Refuses collisions and cross-filesystem moves; the claim remains recoverable on failure.
    pub fn finish(self, destination: &Path) -> Result<(), Error> {
        fs::create_dir(destination).map_err(|e| io_error(destination, e))?;
        fs::rename(&self.path, destination.join("input")).map_err(|e| io_error(&self.path, e))
    }
}
