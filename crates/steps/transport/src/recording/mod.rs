//! Appends protected wire recordings on a dedicated storage thread.

mod format;
use crate::CHUNK_LIMIT;
pub use format::{
    Direction, Event, Evidence, Exception, FormatError, Header, Limits, Link, read, write_event,
    write_header,
};
use std::{
    fs::OpenOptions,
    io::{self, Write as _},
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
    report: tokio::sync::watch::Sender<Option<Failure>>,
    sender: SyncSender<Record>,
}

/// Dedicated recording worker; its result is the authoritative file failure.
/// The host owns and joins its thread. Dropping the tap ends the worker after draining.
pub struct Worker {
    failure: tokio::sync::watch::Sender<Option<Failure>>,
    receiver: Receiver<Record>,
    path: PathBuf,
    header: Header,
}

/// Creates a bounded recording queue without starting a detached thread.
///
/// # Errors
/// Rejects queue capacities outside 1..=1024.
pub fn recording(path: PathBuf, capacity: usize) -> Result<(Tap, Worker), Error> {
    with_header(path, capacity, Header::live())
}

/// Creates a protected tap with the configured protocol stack and known machine metadata.
/// # Errors
/// Rejects invalid capacity or non-machine provenance for live captures.
pub fn with_header(path: PathBuf, capacity: usize, header: Header) -> Result<(Tap, Worker), Error> {
    if header.evidence != Evidence::Machine
        || !header.de_identification.contains("NOT de-identified")
    {
        return Err(Error::Oversized);
    }
    if !(1..=1024).contains(&capacity) {
        return Err(Error::Queue);
    }
    let (sender, receiver) = mpsc::sync_channel(capacity);
    let (failure_tx, failure_rx) = tokio::sync::watch::channel(None);
    Ok((
        Tap {
            sender,
            failure: failure_rx,
            report: failure_tx.clone(),
        },
        Worker {
            failure: failure_tx,
            receiver,
            path,
            header,
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
            self.report.send_replace(Some(Failure::Oversized));
            return Err(Failure::Oversized);
        }
        let result = self
            .sender
            .try_send(Record {
                kind,
                at,
                bytes: bytes.to_vec(),
            })
            .map_err(|error| match error {
                mpsc::TrySendError::Full(_) => Failure::QueueFull,
                mpsc::TrySendError::Disconnected(_) => self.failure().unwrap_or(Failure::Closed),
            });
        if let Err(failure) = &result {
            self.report.send_replace(Some(failure.clone()));
        }
        result
    }
}

impl Worker {
    /// Appends complete ADR 0009 JSONL records and synchronizes each record to disk.
    /// Existing data is never truncated. Run only on a dedicated storage thread.
    ///
    /// # Errors
    /// Returns path-specific open, write or synchronization errors.
    pub fn run(self) -> Result<(), Error> {
        self.run_guarded(|| Ok(()))
    }

    /// Checks host storage admission before opening and before each record.
    /// The check runs only on the dedicated worker; failure disables recording alone.
    /// # Errors
    /// Reports admission, recording queue and file failures without affecting transport.
    pub fn run_guarded(self, mut admission: impl FnMut() -> io::Result<()>) -> Result<(), Error> {
        self.write(&mut admission).map_err(|source| {
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

    fn write(&self, admission: &mut impl FnMut() -> io::Result<()>) -> io::Result<()> {
        admission()?;
        let mut options = OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(Path::new(&self.path))?;
        if file.metadata()?.len() != 0 {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "use a fresh recording file; existing captures are never changed",
            ));
        }
        write_header(&mut file, &self.header, Limits::default()).map_err(io::Error::other)?;
        file.sync_data()?;
        let mut origin = None;
        let mut count = 0usize;
        loop {
            if let Some(failure) = self.failure.borrow().as_ref() {
                return Err(io::Error::other(failure.to_string()));
            }
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
            admission()?;
            count = count
                .checked_add(1)
                .ok_or_else(|| io::Error::other("event count overflow"))?;
            if count > Limits::default().events
                || file.metadata()?.len()
                    >= u64::try_from(Limits::default().file_bytes).map_err(io::Error::other)?
            {
                return Err(io::Error::other("recording file/event limit"));
            }
            let (direction, link) = match record.kind {
                Kind::Open => (Direction::MachineToHost, Some(Link::Connect)),
                Kind::In => (Direction::MachineToHost, None),
                Kind::Out => (Direction::HostToMachine, None),
                Kind::Close => (Direction::MachineToHost, Some(Link::Disconnect)),
            };
            let elapsed = record
                .at
                .saturating_duration_since(*origin.get_or_insert(record.at))
                .as_millis();
            let event = Event {
                direction,
                time_ms: u64::try_from(elapsed).map_err(io::Error::other)?,
                bytes: record.bytes,
                link,
            };
            let mut encoded = Vec::new();
            write_event(&mut encoded, &event, Limits::default()).map_err(io::Error::other)?;
            let length = u64::try_from(encoded.len()).map_err(io::Error::other)?;
            let limit = u64::try_from(Limits::default().file_bytes).map_err(io::Error::other)?;
            if file
                .metadata()?
                .len()
                .checked_add(length)
                .is_none_or(|size| size > limit)
            {
                return Err(io::Error::other("recording file byte limit"));
            }
            file.write_all(&encoded)?;
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
    #[error("recording {path}: {stable_kind}", stable_kind = crate::evidence::io_kind(*.kind))]
    Io {
        /// Recording file path.
        path: PathBuf,
        /// Original I/O error kind.
        kind: io::ErrorKind,
    },
}
