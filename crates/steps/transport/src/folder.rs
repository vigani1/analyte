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

/// Bounded directory watcher driven on the host's dedicated storage thread.
/// Each scan makes at most 1024 observations; excess entries stop with an error.
/// Claimed receipts are returned to the host, which archives or quarantines only
/// after its durable consumer confirms completion.
pub struct Watcher {
    incoming: PathBuf,
    claimed: PathBuf,
    stable: Option<Duration>,
    max_bytes: u64,
    observations: std::collections::BTreeMap<PathBuf, Observation>,
    next: u64,
}

impl Watcher {
    /// Validates existing incoming/claimed directories and a bounded file size.
    /// `None` requires the producer to rename complete files into incoming.
    /// # Errors
    /// Rejects invalid paths, zero stable periods and unsupported size bounds.
    pub fn new(
        incoming: PathBuf,
        claimed: PathBuf,
        stable: Option<Duration>,
        max_bytes: u64,
    ) -> Result<Self, Error> {
        if !incoming.is_dir()
            || !claimed.is_dir()
            || incoming == claimed
            || stable.is_some_and(|period| period.is_zero())
            || !(1..=16_777_216).contains(&max_bytes)
        {
            return Err(Error::Invalid(incoming));
        }
        Ok(Self {
            incoming,
            claimed,
            stable,
            max_bytes,
            observations: std::collections::BTreeMap::new(),
            next: 0,
        })
    }

    /// Scans regular files and atomically claims only completed ones.
    /// Call on a dedicated thread at a host-controlled, cancellable poll interval.
    /// # Errors
    /// Reports file and directory errors and refuses more than 1024 entries.
    pub fn scan(&mut self) -> Result<Vec<Claimed>, Error> {
        let mut ready = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        for (index, entry) in fs::read_dir(&self.incoming)
            .map_err(|e| io_error(&self.incoming, e))?
            .enumerate()
        {
            if index >= 1024 {
                return Err(Error::Invalid(self.incoming.clone()));
            }
            let path = entry.map_err(|e| io_error(&self.incoming, e))?.path();
            if !fs::symlink_metadata(&path)
                .map_err(|e| io_error(&path, e))?
                .is_file()
            {
                continue;
            }
            seen.insert(path.clone());
            let complete = if let Some(stable) = self.stable {
                match self.observations.entry(path.clone()) {
                    std::collections::btree_map::Entry::Vacant(entry) => {
                        entry.insert(Observation::new(path.clone())?);
                        false
                    }
                    std::collections::btree_map::Entry::Occupied(mut entry) => {
                        entry.get_mut().ready(stable)?
                    }
                }
            } else {
                true
            };
            if complete {
                ready.push(path);
            }
        }
        self.observations.retain(|path, _| seen.contains(path));
        let mut receipts = Vec::new();
        for source in ready {
            // Never replace a previous claim, including one left by a crash.
            let destination = loop {
                self.next = self
                    .next
                    .checked_add(1)
                    .ok_or_else(|| Error::Invalid(self.claimed.clone()))?;
                let destination = self.claimed.join(format!("receipt-{}", self.next));
                if !destination.exists() {
                    break destination;
                }
                if self.next > 1_000_000 {
                    return Err(Error::Invalid(self.claimed.clone()));
                }
            };
            receipts.push(Claimed::claim(&source, &destination, self.max_bytes)?);
            self.observations.remove(&source);
        }
        Ok(receipts)
    }

    /// Finds up to 1024 receipts left in the claimed area after a crash.
    /// # Errors
    /// Reports malformed receipt directories and filesystem failures without deleting them.
    pub fn recover(&self) -> Result<Vec<Claimed>, Error> {
        let mut receipts = Vec::new();
        for (index, entry) in fs::read_dir(&self.claimed)
            .map_err(|e| io_error(&self.claimed, e))?
            .enumerate()
        {
            if index >= 1024 {
                return Err(Error::Invalid(self.claimed.clone()));
            }
            let directory = entry.map_err(|e| io_error(&self.claimed, e))?.path();
            // An empty claim directory can remain if the process stopped before rename.
            if directory.join("input").exists() {
                receipts.push(Claimed::recover(&directory, self.max_bytes)?);
            }
        }
        Ok(receipts)
    }
}

impl Claimed {
    /// Streams this receipt through the common ordered events and optional recorder.
    /// The file stays claimed. Call `finish` only after the host confirms durable storage.
    /// This read-only connection never sends queued dialog writes to the producer.
    /// # Errors
    /// Reports bounded file reads or event delivery failure, retaining the file.
    pub fn deliver(&self, mut side: crate::IoSide) -> Result<(), Error> {
        let bytes = self.read()?;
        side.start(format!("folder path={}", self.path.display()))
            .map_err(|e| io_error(&self.path, io::Error::other(e)))?;
        let result = (|| {
            let session = side.generation;
            crate::serial::emit(&mut side, crate::Event::Opened { session })?;
            for chunk in bytes.chunks(crate::CHUNK_LIMIT) {
                if side.cancel.is_cancelled() {
                    break;
                }
                crate::serial::emit(
                    &mut side,
                    crate::Event::Received {
                        at: Instant::now(),
                        bytes: chunk.to_vec(),
                    },
                )?;
            }
            side.stop();
            let reason = if side.cancel.is_cancelled() {
                crate::Reason::Cancelled
            } else {
                crate::Reason::Disconnect
            };
            crate::serial::emit(&mut side, crate::Event::Closed(reason))
        })();
        side.stop();
        result.map_err(|e| io_error(&self.path, io::Error::other(e)))
    }
}

/// Host reply after durable receipt and interpretation; no reply preserves the claim.
#[derive(Clone, Copy)]
pub enum Disposition {
    /// Move the claim to the archive.
    Archive,
    /// Move the claim to quarantine.
    Quarantine,
}
/// One claimed file and its completion reply channel.
pub struct Item {
    /// Byte connection owned by the host until completion.
    pub connection: crate::Connection,
    /// Reply only after the host's custody decision.
    pub completed: std::sync::mpsc::SyncSender<Disposition>,
}
// A file is delivered serially: one queued chunk/command and a five-second
// handoff/operation budget bound memory and waits without declaring clinical acceptance.
const FILE_CHANNEL_LIMIT: usize = 1;
const HANDOFF_TIMEOUT: Duration = Duration::from_secs(5);
fn handoff_deadline() -> Instant {
    Instant::now()
        .checked_add(HANDOFF_TIMEOUT)
        .unwrap_or_else(Instant::now)
}
/// Blocking watch/claim/deliver owner. Run on a dedicated, joined thread.
/// Archive and quarantine are reachable only after a host reply; cancellation,
/// a lost reply or delivery failure leaves the claim recoverable on restart.
pub struct Watch {
    /// Producer rename handoff directory.
    pub incoming: PathBuf,
    /// Protected claim/archive/quarantine root.
    pub root: PathBuf,
    /// Maximum file size.
    pub limit: u64,
}
impl Watch {
    /// Drives claimed files until cancellation or the host disconnects.
    /// # Errors
    /// Preserves filesystem and transport causes; never finishes a failed handoff.
    pub fn run(
        self,
        cancelled: &std::sync::atomic::AtomicBool,
        sender: &tokio::sync::mpsc::Sender<Item>,
        mut recorder: Option<Box<dyn FnMut() -> Option<crate::recording::Tap> + Send>>,
        mut finish_name: impl FnMut() -> String,
    ) -> Result<(), WatchError> {
        use std::{
            sync::{atomic::Ordering, mpsc},
            thread,
        };
        let Self {
            incoming,
            root,
            limit,
        } = self;
        let claimed = root.join("claimed");
        fs::create_dir_all(&claimed)?;
        fs::create_dir_all(root.join("archive"))?;
        fs::create_dir_all(root.join("quarantine"))?;
        // Completion is producer rename into incoming, never a guessed stable size.
        let mut watcher = Watcher::new(incoming, claimed, None, limit)?;
        let mut claims = watcher.recover()?;
        while !cancelled.load(Ordering::Acquire) {
            if claims.is_empty() {
                claims = watcher.scan()?;
            }
            for claim in claims.drain(..) {
                if cancelled.load(Ordering::Acquire) {
                    return Ok(());
                }
                let (connection, mut side) =
                    crate::connection(crate::Limits::new(FILE_CHANNEL_LIMIT, HANDOFF_TIMEOUT)?);
                if let Some(tap) = recorder.as_mut().and_then(|factory| factory()) {
                    side = side.with_recording(tap);
                }
                let cancellation = connection.cancellation();
                let (reply, response) = mpsc::sync_channel(1);
                let item = Item {
                    connection,
                    completed: reply,
                };
                if !handoff(sender, item, cancelled) {
                    return Ok(());
                }
                if let Err(error) = claim.deliver(side) {
                    let interrupted = match &error {
                        Error::Io { source, .. } => source
                            .get_ref()
                            .and_then(|source| source.downcast_ref::<crate::Error>())
                            .is_some_and(|source| {
                                matches!(
                                    source,
                                    crate::Error::ConsumerClosed | crate::Error::Deadline(_)
                                )
                            }),
                        Error::Oversized(_) | Error::Invalid(_) => false,
                    };
                    if interrupted
                        && (cancelled.load(Ordering::Acquire) || cancellation.is_cancelled())
                    {
                        return Ok(());
                    }
                    return Err(error.into());
                }
                let Some(disposition) = await_disposition(&response, cancelled) else {
                    return Ok(());
                };
                let area = match disposition {
                    Disposition::Archive => "archive",
                    Disposition::Quarantine => "quarantine",
                };
                claim.finish(&root.join(area).join(finish_name()))?;
            }
            // Directory scans poll because producer renames have no notification channel.
            // This interval also bounds observation of the host cancellation flag.
            thread::sleep(Duration::from_millis(100));
        }
        Ok(())
    }
}

// The host exposes only an atomic cancellation flag, not a wakeable signal.
// A blocking send could outlive both cancellation and the handoff deadline.
fn handoff(
    sender: &tokio::sync::mpsc::Sender<Item>,
    mut item: Item,
    cancelled: &std::sync::atomic::AtomicBool,
) -> bool {
    use std::sync::atomic::Ordering;
    let until = handoff_deadline();
    loop {
        match sender.try_send(item) {
            Ok(()) => return true,
            Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => return false,
            Err(tokio::sync::mpsc::error::TrySendError::Full(returned)) => item = returned,
        }
        if cancelled.load(Ordering::Acquire) || Instant::now() >= until {
            return false;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

// Timed channel waits preserve cancellation responsiveness without a wakeable
// cancellation signal. A lost or late reply always leaves the claim recoverable.
fn await_disposition(
    response: &std::sync::mpsc::Receiver<Disposition>,
    cancelled: &std::sync::atomic::AtomicBool,
) -> Option<Disposition> {
    use std::sync::{atomic::Ordering, mpsc::RecvTimeoutError};
    let until = handoff_deadline();
    loop {
        match response.recv_timeout(Duration::from_millis(10)) {
            Ok(disposition) => return Some(disposition),
            Err(RecvTimeoutError::Disconnected) => return None,
            Err(RecvTimeoutError::Timeout) => {
                if cancelled.load(Ordering::Acquire) || Instant::now() >= until {
                    return None;
                }
            }
        }
    }
}

/// Specific failures from the blocking file owner.
#[derive(Debug, thiserror::Error)]
pub enum WatchError {
    /// Claim or completion operation failed.
    #[error(transparent)]
    File(#[from] Error),
    /// Directory creation failed.
    #[error(transparent)]
    Io(#[from] io::Error),
    /// Connection limit validation failed.
    #[error(transparent)]
    Transport(#[from] crate::Error),
}
