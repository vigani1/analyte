//! Host-owned bounded byte streams. No transport interprets protocol bytes.
#![forbid(unsafe_code)]

use std::{
    io,
    time::{Duration, Instant},
};
use thiserror::Error;
use tokio::sync::mpsc;

pub mod tcp;

/// Maximum bytes in a single read or queued write.
pub const CHUNK_LIMIT: usize = 4096;

/// Limits validated before a connection is started.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    capacity: usize,
    deadline: Duration,
}

impl Limits {
    /// Rejects zero or excessive queue sizes and deadlines.
    ///
    /// # Errors
    /// Returns `InvalidLimits` unless capacity is 1..=1024 and deadline is 1ms..=60s.
    pub fn new(capacity: usize, deadline: Duration) -> Result<Self, Error> {
        if !(1..=1024).contains(&capacity)
            || !(Duration::from_millis(1)..=Duration::from_secs(60)).contains(&deadline)
        {
            return Err(Error::InvalidLimits);
        }
        Ok(Self { capacity, deadline })
    }
}

/// Specific transport failures; a failed write may already have sent a prefix.
#[derive(Debug, Error)]
pub enum Error {
    /// Queue or deadline settings are outside supported bounds.
    #[error("invalid transport limits")]
    InvalidLimits,
    /// A write would exceed the byte bound.
    #[error("write exceeds {CHUNK_LIMIT} bytes")]
    OversizedWrite,
    /// The consumer stopped accepting events.
    #[error("connection consumer closed")]
    ConsumerClosed,
    /// The configured deadline elapsed.
    #[error("deadline elapsed during {0}")]
    Deadline(&'static str),
    /// An operating system operation failed at a named endpoint.
    #[error("{operation} at {endpoint}: {source}")]
    Io {
        /// Operation that failed.
        operation: &'static str,
        /// Configured endpoint, never payload data.
        endpoint: String,
        /// Original operating system error.
        source: io::Error,
    },
}

/// Ordered events from one connection; receive instants use a monotonic clock.
#[derive(Debug)]
pub enum Event {
    /// The connection is open; no protocol handshake is implied.
    Opened,
    /// Exact bytes in read order, independent of protocol message boundaries.
    Received {
        /// Instant immediately after the read completed.
        at: Instant,
        /// At most `CHUNK_LIMIT` exact bytes.
        bytes: Vec<u8>,
    },
    /// The peer closed its byte stream cleanly.
    Disconnected,
}

/// Dialog endpoint with bounded queues. Drop it to cancel the I/O owner.
pub struct Connection {
    events: mpsc::Receiver<Event>,
    writes: mpsc::Sender<Vec<u8>>,
    limits: Limits,
}

impl Connection {
    /// Waits for the next event within the configured deadline.
    ///
    /// # Errors
    /// Returns a deadline error if no event arrives in time.
    pub async fn receive(&mut self) -> Result<Option<Event>, Error> {
        tokio::time::timeout(self.limits.deadline, self.events.recv())
            .await
            .map_err(|_| Error::Deadline("receive"))
    }

    /// Queues bounded bytes; success promises queue acceptance, not peer receipt.
    /// Cancellation before acceptance queues nothing. Never automatically retry an uncertain write.
    ///
    /// # Errors
    /// Rejects oversized bytes, a closed owner, or queue backpressure exceeding the deadline.
    pub async fn write(&self, bytes: Vec<u8>) -> Result<(), Error> {
        if bytes.len() > CHUNK_LIMIT {
            return Err(Error::OversizedWrite);
        }
        tokio::time::timeout(self.limits.deadline, self.writes.send(bytes))
            .await
            .map_err(|_| Error::Deadline("queue write"))?
            .map_err(|_| Error::ConsumerClosed)
    }
}

/// I/O endpoint consumed by exactly one owner future.
/// Dropping the owner closes both queues; queued writes are never replayed.
pub struct IoSide {
    events: mpsc::Sender<Event>,
    writes: mpsc::Receiver<Vec<u8>>,
    limits: Limits,
}

/// Creates bounded queues without starting or detaching any task.
#[must_use]
pub fn connection(limits: Limits) -> (Connection, IoSide) {
    let (events_tx, events_rx) = mpsc::channel(limits.capacity);
    let (writes_tx, writes_rx) = mpsc::channel(limits.capacity);
    (
        Connection {
            events: events_rx,
            writes: writes_tx,
            limits,
        },
        IoSide {
            events: events_tx,
            writes: writes_rx,
            limits,
        },
    )
}

impl IoSide {
    async fn emit(&self, event: Event) -> Result<(), Error> {
        tokio::time::timeout(self.limits.deadline, self.events.send(event))
            .await
            .map_err(|_| Error::Deadline("event backpressure"))?
            .map_err(|_| Error::ConsumerClosed)
    }
}
pub mod folder;
pub mod recording;
pub mod serial;
