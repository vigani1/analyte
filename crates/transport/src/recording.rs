//! Append-only protected recordings. The host must use a dedicated storage thread.

use crate::CHUNK_LIMIT;
use std::{
    fs::OpenOptions,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, SyncSender},
    time::Instant,
};
use thiserror::Error;

/// Recording failure must be surfaced by the host independently of transport errors.
#[derive(Debug, Error)]
pub enum Error {
    /// A full queue stops the recording visibly rather than losing bytes silently.
    #[error("recording queue full or closed")]
    Queue,
    /// A record exceeds the format's chunk bound.
    #[error("recording record exceeds byte bound")]
    Oversized,
    /// Original file failure and recording path.
    #[error("recording {path}: {source}")]
    Io {
        /// Protected recording file.
        path: PathBuf,
        /// Original failure.
        source: io::Error,
    },
}

/// Record kinds distinguish sessions, bytes and terminal reasons.
#[derive(Clone, Copy)]
pub enum Kind {
    /// Session starts; bytes contain UTF-8 connection settings.
    Open,
    /// Exact incoming bytes.
    In,
    /// Exact successfully written outgoing bytes.
    Out,
    /// Session ends; bytes contain a nonclinical reason.
    Close,
}

struct Record {
    kind: Kind,
    at: Instant,
    bytes: Vec<u8>,
}

/// Nonblocking recording tap with a bounded queue.
pub struct Tap {
    failure: tokio::sync::watch::Receiver<Option<Failure>>,
    sender: SyncSender<Record>,
}

/// Dedicated recording worker; its result is the authoritative file failure.
/// The host owns and joins its thread. Dropping the tap ends the worker after draining.
pub struct Worker {
    failure: tokio::sync::watch::Sender<Option<Failure>>,
    receiver: Receiver<Record>,
    path: PathBuf,
    origin: Instant,
}

/// Creates a bounded recording queue without starting a detached thread.
///
/// # Errors
/// Rejects queue capacities outside 1..=1024.
pub fn recording(path: PathBuf, capacity: usize) -> Result<(Tap, Worker), Error> {
    if !(1..=1024).contains(&capacity) {
        return Err(Error::Queue);
    }
    let (sender, receiver) = mpsc::sync_channel(capacity);
    let (failure_tx, failure_rx) = tokio::sync::watch::channel(None);
    Ok((
        Tap {
            sender,
            failure: failure_rx,
        },
        Worker {
            failure: failure_tx,
            receiver,
            path,
            origin: Instant::now(),
        },
    ))
}

impl Tap {
    /// Returns the worker failure even when the connection is idle.
    #[must_use]
    pub fn failure(&self) -> Option<Failure> {
        self.failure.borrow().clone()
    }

    /// Offers exact bytes without waiting for storage. Failure must stop or flag the session.
    ///
    /// # Errors
    /// Reports oversized records or queue saturation/worker closure immediately.
    pub fn record(&self, kind: Kind, at: Instant, bytes: &[u8]) -> Result<(), Failure> {
        if let Some(failure) = self.failure() {
            return Err(failure);
        }
        if bytes.len() > CHUNK_LIMIT {
            return Err(Failure::Oversized);
        }
        self.sender
            .try_send(Record {
                kind,
                at,
                bytes: bytes.to_vec(),
            })
            .map_err(|error| match error {
                mpsc::TrySendError::Full(_) => Failure::QueueFull,
                mpsc::TrySendError::Disconnected(_) => self.failure().unwrap_or(Failure::Closed),
            })
    }
}

impl Worker {
    /// Appends complete length-delimited records and synchronizes each record to disk.
    /// Existing data is never truncated. Run only on a dedicated storage thread.
    ///
    /// # Errors
    /// Returns path-specific open, write or synchronization errors.
    pub fn run(self) -> Result<(), Error> {
        self.write().map_err(|source| {
            self.failure.send_replace(Some(Failure::Io {
                path: self.path.clone(),
                kind: source.kind(),
            }));
            Error::Io {
                path: self.path,
                source,
            }
        })
    }

    fn write(&self) -> io::Result<()> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(Path::new(&self.path))?;
        loop {
            let record = match self
                .receiver
                .recv_timeout(std::time::Duration::from_secs(1))
            {
                Ok(record) => record,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    continue;
                }
            };
            let kind = match record.kind {
                Kind::Open => 0,
                Kind::In => 1,
                Kind::Out => 2,
                Kind::Close => 3,
            };
            let elapsed = record.at.saturating_duration_since(self.origin).as_nanos();
            let length = u32::try_from(record.bytes.len()).map_err(io::Error::other)?;
            file.write_all(b"ATR1")?;
            file.write_all(&[kind])?;
            file.write_all(&elapsed.to_le_bytes())?;
            file.write_all(&length.to_le_bytes())?;
            file.write_all(&record.bytes)?;
            file.sync_data()?;
        }
        Ok(())
    }
}

/// Recorder status delivered independently on the connection event stream.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum Failure {
    /// Queue saturation; recording stops rather than silently omitting a chunk.
    #[error("recording queue full")]
    QueueFull,
    /// Worker no longer accepts records.
    #[error("recording worker closed")]
    Closed,
    /// A record exceeded the bound.
    #[error("recording record too large")]
    Oversized,
    /// File failure, with protected path and portable error kind.
    #[error("recording {path}: {kind:?}")]
    Io {
        /// Recording file path.
        path: PathBuf,
        /// Original I/O error kind.
        kind: io::ErrorKind,
    },
}
