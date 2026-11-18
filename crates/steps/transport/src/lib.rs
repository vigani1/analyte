//! Reads and writes bounded transport bytes and recordings without interpreting protocols.
#![forbid(unsafe_code)]

use std::{
    io,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use thiserror::Error;
use tokio::sync::mpsc;

mod deadline;
pub use deadline::DeadlineKind;
mod evidence;
pub mod folder;
pub mod recording;
pub mod serial;
pub mod tcp;

/// Maximum bytes in a read, recording record or queued write.
pub const CHUNK_LIMIT: usize = 4096;

/// Validated queue and operation bounds.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    capacity: usize,
    deadline: Duration,
}

impl Limits {
    /// Accepts capacities 1..=1024 and deadlines 1ms..=60s.
    /// # Errors
    /// Rejects bounds outside the supported range.
    pub fn new(capacity: usize, deadline: Duration) -> Result<Self, Error> {
        if !(1..=1024).contains(&capacity)
            || !(Duration::from_millis(1)..=Duration::from_secs(60)).contains(&deadline)
        {
            return Err(Error::InvalidLimits);
        }
        Ok(Self { capacity, deadline })
    }
}

/// Specific failures; interrupted writes may already have sent a prefix.
#[derive(Debug, Error)]
pub enum Error {
    /// Invalid queue, retry or deadline settings.
    #[error("invalid transport limits")]
    InvalidLimits,
    /// A write exceeds the byte limit.
    #[error("write exceeds {CHUNK_LIMIT} bytes")]
    OversizedWrite,
    /// The consumer has closed.
    #[error("connection consumer closed")]
    ConsumerClosed,
    /// The connection is not currently open.
    #[error("connection is offline")]
    Offline,
    /// An operation exceeded its deadline.
    #[error("deadline elapsed during {0}")]
    Deadline(DeadlineKind),
    /// An I/O failure retains the endpoint and original error.
    #[error("{operation} at {endpoint}: {source}")]
    Io {
        /// Failed operation.
        operation: &'static str,
        /// Configured endpoint.
        endpoint: String,
        /// Original error.
        source: io::Error,
    },
}

/// Nonclinical reasons carried on the ordered event stream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reason {
    /// Queue or operation bounds were invalid.
    InvalidLimits,
    /// A write exceeded the transport byte limit.
    OversizedWrite,
    /// The connection was not open.
    Offline,
    /// Peer ended the stream.
    Disconnect,
    /// Host requested shutdown.
    Cancelled,
    /// Operating system error, retaining endpoint, operation and error kind.
    Io {
        /// Affected endpoint.
        endpoint: String,
        /// Failed operation.
        operation: &'static str,
        /// Portable operating system classification.
        kind: io::ErrorKind,
    },
    /// The fixed serial device is no longer available.
    PortVanished(String),
    /// Server refused an excess connection.
    LimitReached,
    /// An operation deadline elapsed.
    Deadline(DeadlineKind),
}

impl Error {
    pub(crate) fn reason(&self) -> Reason {
        match self {
            Self::Io {
                endpoint,
                operation,
                source,
            } => Reason::Io {
                endpoint: endpoint.clone(),
                operation,
                kind: source.kind(),
            },
            Self::Deadline(operation) => Reason::Deadline(*operation),
            Self::ConsumerClosed => Reason::Cancelled,
            Self::InvalidLimits => Reason::InvalidLimits,
            Self::OversizedWrite => Reason::OversizedWrite,
            Self::Offline => Reason::Offline,
        }
    }
}

/// Events in I/O order. Failed and closed events never imply durable receipt.
#[derive(Debug)]
pub enum Event {
    /// A session is open; no protocol handshake is implied.
    Opened {
        /// Monotonically increasing identity within this connection owner.
        session: u64,
    },
    /// Exact incoming bytes and monotonic read-completion instant.
    Received {
        /// Monotonic receive time.
        at: Instant,
        /// At most `CHUNK_LIMIT` bytes.
        bytes: Vec<u8>,
    },
    /// A session closed for the stated reason.
    Closed(Reason),
    /// A connection attempt or session failed for the stated reason.
    Failed(Reason),
    /// The optional recorder stopped; transport continues without a tap.
    RecordingFailed(recording::Failure),
}

/// Cloneable cancellation shared by an owner and its host.
#[derive(Clone, Default)]
pub struct Cancellation {
    requested: Arc<AtomicBool>,
    notification: Arc<tokio::sync::Notify>,
}

impl Cancellation {
    /// Requests cancellation without waiting or detaching work.
    pub fn cancel(&self) {
        self.requested.store(true, Ordering::Release);
        self.notification.notify_waiters();
    }

    /// Reports whether shutdown has been requested.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.requested.load(Ordering::Acquire)
    }

    pub(crate) async fn cancelled(&self) {
        // Register before checking the flag so cancellation cannot fall between them.
        let notified = self.notification.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        if !self.is_cancelled() {
            notified.await;
        }
    }
}

struct Write {
    session: u64,
    bytes: Vec<u8>,
}

/// One item on a connection's ordered outgoing queue.
enum Outgoing {
    /// Bytes for one session.
    Bytes(Write),
    /// Confirms that every earlier item for `session` has been handed to the
    /// operating system. A stale session's marker is dropped unanswered.
    Flush {
        session: u64,
        sent: tokio::sync::oneshot::Sender<()>,
    },
}

impl IoSide {
    /// Takes the next outgoing bytes for the current session, answering flush
    /// markers on the way. Because the queue is ordered and each write completes
    /// before the next item is taken, an answered marker proves every earlier
    /// write left this owner.
    fn next_write(&self, outgoing: Outgoing) -> Option<Write> {
        match outgoing {
            Outgoing::Bytes(write) if write.session == self.generation => Some(write),
            Outgoing::Bytes(_) => None,
            Outgoing::Flush { session, sent } => {
                if session == self.generation {
                    // A closed receiver means the flusher stopped waiting at its own
                    // deadline and already reports that outcome as uncertain.
                    let _ = sent.send(());
                }
                None
            }
        }
    }
}

/// Dialog endpoint with bounded queues. Drop cancels its I/O owner.
pub struct Connection {
    observed_session: u64,
    events: mpsc::Receiver<Event>,
    writes: mpsc::Sender<Outgoing>,
    limits: Limits,
    session: Arc<AtomicU64>,
    cancel: Cancellation,
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

impl Connection {
    /// Queues all bytes in bounded chunks, optionally within one absolute deadline.
    /// With no overall deadline, the configured deadline applies separately to each chunk.
    /// Cancellation or failure may leave a queued or transmitted prefix; never retry
    /// an order implicitly. Success means queued, not acknowledged by the peer.
    /// # Errors
    /// Preserves per-chunk transport errors and reports the overall write deadline.
    pub async fn write_all(
        &self,
        bytes: &[u8],
        until: Option<tokio::time::Instant>,
    ) -> Result<(), Error> {
        let write = async {
            for chunk in bytes.chunks(CHUNK_LIMIT) {
                self.write(chunk.to_vec()).await?;
            }
            Ok(())
        };
        match until {
            Some(until) => tokio::time::timeout_at(until, write)
                .await
                .map_err(|_| Error::Deadline(DeadlineKind::WriteAll))?,
            None => write.await,
        }
    }

    /// Returns a handle that can request shutdown while keeping events readable.
    #[must_use]
    pub fn cancellation(&self) -> Cancellation {
        self.cancel.clone()
    }

    /// Receives the next ordered event within the configured deadline.
    /// # Errors
    /// Reports a receive deadline, without cancelling the connection.
    pub async fn receive(&mut self) -> Result<Option<Event>, Error> {
        let event = tokio::time::timeout(self.limits.deadline, self.events.recv())
            .await
            .map_err(|_| Error::Deadline(DeadlineKind::Receive))?;
        match &event {
            Some(Event::Opened { session }) => self.observed_session = *session,
            Some(Event::Closed(_)) | None => self.observed_session = 0,
            Some(Event::Received { .. } | Event::Failed(_) | Event::RecordingFailed(_)) => {}
        }
        Ok(event)
    }

    /// Queues bytes for the current session only. Success means queue acceptance.
    /// Writes from an earlier session are rejected by the owner, never replayed.
    /// # Errors
    /// Reports offline, oversized, closed or backpressured writes.
    pub async fn write(&self, bytes: Vec<u8>) -> Result<(), Error> {
        if bytes.len() > CHUNK_LIMIT {
            return Err(Error::OversizedWrite);
        }
        let session = self.open_session()?;
        tokio::time::timeout(
            self.limits.deadline,
            self.writes.send(Outgoing::Bytes(Write { session, bytes })),
        )
        .await
        .map_err(|_| Error::Deadline(DeadlineKind::QueueWrite))?
        .map_err(|_| Error::ConsumerClosed)
    }

    /// Waits, within the configured deadline, until the I/O owner has handed every
    /// write queued before this call to the operating system. Call it before
    /// cancelling, so a final reply such as a refusal is not discarded with the
    /// queue. It does not prove the peer received the bytes.
    /// # Errors
    /// `Offline` when the session is not open, or ends before its queued writes
    /// were sent; a flush deadline when the owner did not reach them in time, which
    /// leaves their outcome uncertain.
    pub async fn flush(&self) -> Result<(), Error> {
        let session = self.open_session()?;
        let (sent, confirmed) = tokio::sync::oneshot::channel();
        tokio::time::timeout(self.limits.deadline, async {
            self.writes
                .send(Outgoing::Flush { session, sent })
                .await
                .map_err(|_| Error::Offline)?;
            confirmed.await.map_err(|_| Error::Offline)
        })
        .await
        .map_err(|_| Error::Deadline(DeadlineKind::Flush))?
    }

    /// The observed session, if the I/O owner still serves it.
    fn open_session(&self) -> Result<u64, Error> {
        let session = self.observed_session;
        if session == 0 || session != self.session.load(Ordering::Acquire) {
            return Err(Error::Offline);
        }
        Ok(session)
    }
}

/// I/O endpoint consumed by one caller-owned future or dedicated thread.
pub struct IoSide {
    events: mpsc::Sender<Event>,
    writes: mpsc::Receiver<Outgoing>,
    limits: Limits,
    session: Arc<AtomicU64>,
    generation: u64,
    cancel: Cancellation,
    tap: Option<recording::Tap>,
    recorder: Option<Box<dyn FnMut() -> Option<recording::Tap> + Send + Sync>>,
    settings: String,
}

/// Creates bounded queues without starting a task.
#[must_use]
pub fn connection(limits: Limits) -> (Connection, IoSide) {
    let (events_tx, events_rx) = mpsc::channel(limits.capacity);
    let (writes_tx, writes_rx) = mpsc::channel(limits.capacity);
    let session = Arc::new(AtomicU64::new(0));
    let cancel = Cancellation::default();
    (
        Connection {
            observed_session: 0,
            events: events_rx,
            writes: writes_tx,
            limits,
            session: session.clone(),
            cancel: cancel.clone(),
        },
        IoSide {
            events: events_tx,
            writes: writes_rx,
            limits,
            session,
            generation: 0,
            cancel,
            tap: None,
            recorder: None,
            settings: String::new(),
        },
    )
}

impl IoSide {
    /// Installs a nonblocking recorder. Settings are supplied by each transport, not payloads.
    #[must_use]
    pub fn with_recording(mut self, tap: recording::Tap) -> Self {
        self.tap = Some(tap);
        self
    }

    /// Creates a fresh optional recorder at each successful connection, including reconnects.
    #[must_use]
    pub fn with_recording_factory(
        mut self,
        factory: impl FnMut() -> Option<recording::Tap> + Send + Sync + 'static,
    ) -> Self {
        self.recorder = Some(Box::new(factory));
        self
    }

    fn start(&mut self, settings: String) -> Result<(), Error> {
        if let Some(factory) = &mut self.recorder {
            self.tap = None;
            self.tap = factory();
        }
        self.settings = settings;
        self.generation = self.generation.checked_add(1).ok_or(Error::InvalidLimits)?;
        self.session.store(self.generation, Ordering::Release);
        Ok(())
    }

    fn stop(&self) {
        self.session.store(0, Ordering::Release);
    }

    fn record_event(&mut self, event: &Event) -> Option<Event> {
        let (kind, at, bytes) = match event {
            Event::Opened { .. } => (
                recording::Kind::Open,
                Instant::now(),
                self.settings.as_bytes().to_vec(),
            ),
            Event::Received { at, bytes } => (recording::Kind::In, *at, bytes.clone()),
            Event::Closed(reason) | Event::Failed(reason) => (
                recording::Kind::Close,
                Instant::now(),
                reason.to_string().into_bytes(),
            ),
            Event::RecordingFailed(_) => return None,
        };
        self.record(kind, at, &bytes)
    }

    fn record(&mut self, kind: recording::Kind, at: Instant, bytes: &[u8]) -> Option<Event> {
        if let Some(tap) = &self.tap
            && let Err(error) = tap.record(kind, at, bytes)
        {
            self.tap = None;
            return Some(Event::RecordingFailed(error));
        }
        None
    }

    async fn send(&self, event: Event) -> Result<(), Error> {
        tokio::time::timeout(self.limits.deadline, self.events.send(event))
            .await
            .map_err(|_| Error::Deadline(DeadlineKind::EventBackpressure))?
            .map_err(|_| Error::ConsumerClosed)
    }

    async fn emit(&mut self, event: Event) -> Result<(), Error> {
        let recording_error = self.record_event(&event);
        self.send(event).await?;
        if let Some(error) = recording_error {
            self.send(error).await?;
        }
        Ok(())
    }

    async fn check_recording(&mut self) -> Result<(), Error> {
        if let Some(failure) = self.tap.as_ref().and_then(recording::Tap::failure) {
            self.tap = None;
            self.send(Event::RecordingFailed(failure)).await?;
        }
        Ok(())
    }
    async fn outgoing(&mut self, bytes: &[u8]) -> Result<(), Error> {
        if let Some(event) = self.record(recording::Kind::Out, Instant::now(), bytes) {
            self.send(event).await?;
        }
        Ok(())
    }
}

/// Validated exponential retry delay with bounded deterministic jitter.
/// Seed it separately per connection; jitter is scheduling noise, not cryptography.
#[derive(Clone, Debug)]
pub struct Backoff {
    initial: Duration,
    cap: Duration,
    current: Duration,
    seed: u64,
}

impl Backoff {
    /// Accepts 1ms..=60s delays with initial no greater than cap.
    /// # Errors
    /// Rejects zero, inverted or excessive delays.
    pub fn new(initial: Duration, cap: Duration, seed: u64) -> Result<Self, Error> {
        if initial < Duration::from_millis(1) || initial > cap || cap > Duration::from_secs(60) {
            return Err(Error::InvalidLimits);
        }
        Ok(Self {
            initial,
            cap,
            current: initial,
            seed,
        })
    }

    pub(crate) fn reset(&mut self) {
        self.current = self.initial;
    }

    pub(crate) fn delay(&mut self) -> Duration {
        self.seed = self.seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let factor = u32::try_from(self.seed % 501)
            .unwrap_or_default()
            .saturating_add(500);
        let delay = self
            .current
            .saturating_mul(factor)
            .checked_div(1000)
            .unwrap_or(self.cap);
        self.current = self.current.saturating_mul(2).min(self.cap);
        delay.max(Duration::from_millis(1)).min(self.cap)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn idle_cancellation_wait_is_event_driven_and_wakes_all_waiters() {
        use std::sync::atomic::AtomicUsize;
        use std::task::{Context, Poll, Wake, Waker};
        struct Counter(AtomicUsize);
        impl Wake for Counter {
            fn wake(self: Arc<Self>) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
            fn wake_by_ref(self: &Arc<Self>) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }
        let signal = Cancellation::default();
        let counter = Arc::new(Counter(AtomicUsize::new(0)));
        let waker = Waker::from(counter.clone());
        let mut first = Box::pin(signal.cancelled());
        let mut second = Box::pin(signal.cancelled());
        assert_eq!(
            first.as_mut().poll(&mut Context::from_waker(&waker)),
            Poll::Pending
        );
        assert_eq!(
            second.as_mut().poll(&mut Context::from_waker(&waker)),
            Poll::Pending
        );
        tokio::time::advance(Duration::from_secs(60)).await;
        assert_eq!(counter.0.load(Ordering::SeqCst), 0);
        signal.clone().cancel();
        assert_eq!(counter.0.load(Ordering::SeqCst), 2);
        assert_eq!(
            first.as_mut().poll(&mut Context::from_waker(&waker)),
            Poll::Ready(())
        );
        assert_eq!(
            second.as_mut().poll(&mut Context::from_waker(&waker)),
            Poll::Ready(())
        );
        signal.cancelled().await;
    }

    #[tokio::test]
    async fn writes_require_observed_current_session() -> Result<(), Error> {
        let (mut dialog, mut side) = connection(Limits::new(4, Duration::from_secs(1))?);
        side.start("fake transport".into())?;
        side.emit(Event::Opened {
            session: side.generation,
        })
        .await?;
        assert!(matches!(dialog.write(vec![1]).await, Err(Error::Offline)));
        assert!(matches!(
            dialog.receive().await?,
            Some(Event::Opened { session: 1 })
        ));
        dialog.write(vec![2]).await?;
        side.stop();
        side.start("reopened fake transport".into())?;
        assert!(matches!(dialog.write(vec![3]).await, Err(Error::Offline)));
        side.emit(Event::Opened {
            session: side.generation,
        })
        .await?;
        assert!(matches!(
            dialog.receive().await?,
            Some(Event::Opened { session: 2 })
        ));
        dialog.write(vec![4]).await?;
        Ok(())
    }

    // Safety rule 7 (ADR 0003)
    #[tokio::test]
    async fn write_all_chunks_and_keeps_prefix_on_deadline() -> Result<(), Error> {
        let (mut dialog, mut side) = connection(Limits::new(4, Duration::from_secs(1))?);
        side.start("synthetic".into())?;
        side.emit(Event::Opened {
            session: side.generation,
        })
        .await?;
        let _ = dialog.receive().await?;
        let bytes: Vec<_> = (0..CHUNK_LIMIT * 2 + 7)
            .map(|n| n.to_le_bytes()[0])
            .collect();
        dialog.write_all(&bytes, None).await?;
        let mut recovered = Vec::new();
        while let Ok(Outgoing::Bytes(write)) = side.writes.try_recv() {
            assert!(write.bytes.len() <= CHUNK_LIMIT);
            recovered.extend(write.bytes);
        }
        assert_eq!(recovered, bytes);

        let (mut dialog, mut side) = connection(Limits::new(1, Duration::from_secs(1))?);
        side.start("synthetic".into())?;
        side.emit(Event::Opened {
            session: side.generation,
        })
        .await?;
        let _ = dialog.receive().await?;
        let until = tokio::time::Instant::now() + Duration::from_millis(20);
        assert!(matches!(
            dialog.write_all(&bytes, Some(until)).await,
            Err(Error::Deadline(DeadlineKind::WriteAll))
        ));
        let Ok(Outgoing::Bytes(prefix)) = side.writes.try_recv() else {
            return Err(Error::ConsumerClosed);
        };
        assert_eq!(Some(prefix.bytes.as_slice()), bytes.get(..CHUNK_LIMIT));
        assert!(side.writes.try_recv().is_err());
        Ok(())
    }

    #[test]
    fn backoff_is_capped_jittered_and_resets() -> Result<(), Error> {
        let mut retry = Backoff::new(Duration::from_millis(10), Duration::from_millis(40), 3)?;
        let first = retry.delay();
        assert!(first >= Duration::from_millis(5) && first <= Duration::from_millis(10));
        let delays: Vec<_> = (0..50).map(|_| retry.delay()).collect();
        assert!(
            delays
                .iter()
                .all(|delay| *delay <= Duration::from_millis(40)
                    && *delay >= Duration::from_millis(1))
        );
        assert!(delays.windows(2).any(|pair| pair.first() != pair.last()));
        retry.reset();
        assert!(retry.delay() <= Duration::from_millis(10));
        Ok(())
    }
}
