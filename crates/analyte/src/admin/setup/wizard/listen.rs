//! Runs one bounded listen-only connection test that counts bytes but never sends.
//!
//! The test opens a serial port, accepts one TCP connection or connects to one
//! address, and only reads. It keeps at most [`Limits::bytes`] bytes in memory for
//! at most [`Limits::duration`]; then the bytes are dropped. Nothing is stored,
//! logged or written to the peer. The bytes may hold patient data, so they never
//! leave this owner: the page sees only their count, the framing characters
//! among them and the guess of their protocol family ([`super::verdict`]),
//! which outlive them. The one exception is [`Session::start_showing`]: the
//! `machines test --show-bytes` command, run by a person at the lab PC who
//! asked for them, gets the kept bytes once when the test ends, to print to
//! that person's console only. When the machine connects to this PC, the
//! address of the device that connected is kept too, so the setup can offer
//! to accept only that device (ADR 0026); an address is not patient data.
use super::verdict::{self, Guess};
use analyte_transport::serial;
use std::{
    io,
    net::SocketAddr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::{io::AsyncReadExt as _, sync::Notify};

/// Time and size bounds of one test (AGENTS.md rule 7).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Limits {
    /// How long the test runs and its bytes are kept.
    pub(crate) duration: Duration,
    /// Most bytes kept; reading stops at this size.
    pub(crate) bytes: usize,
}

impl Limits {
    /// 60 seconds and 64 KiB.
    pub(crate) const STANDARD: Self = Self {
        duration: Duration::from_secs(60),
        bytes: 65_536,
    };
}

/// Opens a serial port by name; production passes the operating system's.
pub(crate) type Open =
    Arc<dyn Fn(&str, &serial::Settings) -> io::Result<Box<dyn serial::Port>> + Send + Sync>;

/// What the test listens on.
pub(crate) enum Target {
    /// A serial port by name, with the definition's line settings.
    Serial {
        name: String,
        settings: serial::Settings,
        open: Open,
    },
    /// Listen on this address and accept one connection.
    TcpServer(SocketAddr),
    /// Connect to this address.
    TcpClient(SocketAddr),
}

/// Why a test ended; carries no received bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum End {
    TimeLimit,
    Stopped,
    /// The machine closed the connection.
    Closed,
    /// Opening or reading failed; the text names the OS error, never data.
    Failed(String),
}

/// A view of the test for the page: what was observed, never the bytes.
pub(crate) struct View {
    pub(crate) label: String,
    /// When the test started.
    pub(crate) started: jiff::Timestamp,
    pub(crate) remaining: Duration,
    /// When the port opened or the connection was made; none before.
    pub(crate) connected: Option<jiff::Timestamp>,
    /// The address of the device that connected, in its plain form, when
    /// Analyte listens; none before a connection and for other targets.
    pub(crate) peer: Option<std::net::IpAddr>,
    /// The machine closed the connection; the bytes stay until the test ends.
    pub(crate) closed: bool,
    /// The bytes kept so far, for this module's tests of the bounds; the page never sees them.
    #[cfg(test)]
    pub(crate) bytes: Vec<u8>,
    /// Bytes seen so far, including any past the size bound.
    pub(crate) seen: usize,
    pub(crate) size_limit_reached: bool,
    pub(crate) end: Option<End>,
    /// What the first bytes looked like; kept after the bytes are dropped.
    pub(crate) guess: Guess,
    /// Each control character seen in the kept bytes, in the order first
    /// seen; kept after the bytes are dropped. They frame a message and hold none of it.
    pub(crate) framing: Vec<u8>,
}

struct Capture {
    connected: Option<jiff::Timestamp>,
    peer: Option<std::net::IpAddr>,
    closed: bool,
    bytes: Vec<u8>,
    seen: usize,
    end: Option<End>,
    guess: Guess,
    framing: Vec<u8>,
    /// Where the kept bytes go when the test ends, instead of being dropped;
    /// only for [`Session::start_showing`].
    handover: Option<tokio::sync::oneshot::Sender<Vec<u8>>>,
}

impl Capture {
    /// Keeps bytes up to the bound; returns false once the bound is reached.
    fn take(&mut self, chunk: &[u8], limit: usize) -> bool {
        self.seen = self.seen.saturating_add(chunk.len());
        let room = limit.saturating_sub(self.bytes.len());
        let kept = chunk.get(..room.min(chunk.len())).unwrap_or_default();
        // At most 33 entries: the C0 controls and DEL.
        for &byte in kept {
            if (byte < 0x20 || byte == 0x7f) && !self.framing.contains(&byte) {
                self.framing.push(byte);
            }
        }
        self.bytes.extend_from_slice(kept);
        self.guess = verdict::classify(&self.bytes);
        self.bytes.len() < limit
    }

    fn connect(&mut self) {
        self.connected.get_or_insert_with(jiff::Timestamp::now);
    }

    fn finish(&mut self, end: End) {
        // Discard the bytes when the test ends; only the count remains.
        let bytes = std::mem::take(&mut self.bytes);
        if let Some(handover) = self.handover.take() {
            // A receiver that is gone no longer wants them; they drop here.
            let _ = handover.send(bytes);
        }
        self.end.get_or_insert(end);
    }
}

/// The one running or finished test. Dropping it stops the test and drops the bytes.
pub(crate) struct Session {
    label: String,
    started: jiff::Timestamp,
    deadline: Instant,
    limits: Limits,
    capture: Arc<Mutex<Capture>>,
    stop: Arc<Notify>,
    owner: tokio::task::JoinHandle<()>,
}

impl Drop for Session {
    fn drop(&mut self) {
        // The serial reader sees its stop flag within one read timeout.
        self.stop.notify_one();
        self.owner.abort();
    }
}

/// Serial reads wait at most this long, so a stop takes effect quickly.
pub(crate) const SERIAL_READ: Duration = Duration::from_millis(200);

impl Session {
    /// Starts the test in the background; it ends by itself after `limits.duration`.
    pub(crate) fn start(label: String, target: Target, limits: Limits) -> Self {
        Self::begin(label, target, limits, None)
    }

    /// Starts the test like [`Session::start`], and hands its kept bytes (at
    /// most `limits.bytes`) to the receiver once when it ends, instead of
    /// dropping them. Only for a person at the console who asked to see them.
    pub(crate) fn start_showing(
        label: String,
        target: Target,
        limits: Limits,
    ) -> (Self, tokio::sync::oneshot::Receiver<Vec<u8>>) {
        let (handover, bytes) = tokio::sync::oneshot::channel();
        (Self::begin(label, target, limits, Some(handover)), bytes)
    }

    fn begin(
        label: String,
        target: Target,
        limits: Limits,
        handover: Option<tokio::sync::oneshot::Sender<Vec<u8>>>,
    ) -> Self {
        let deadline = Instant::now()
            .checked_add(limits.duration)
            .unwrap_or_else(Instant::now);
        let capture = Arc::new(Mutex::new(Capture {
            connected: None,
            peer: None,
            closed: false,
            bytes: Vec::new(),
            seen: 0,
            end: None,
            guess: Guess::Nothing,
            framing: Vec::new(),
            handover,
        }));
        let stop = Arc::new(Notify::new());
        let owner = tokio::spawn(run(
            target,
            limits,
            tokio::time::Instant::from_std(deadline),
            capture.clone(),
            stop.clone(),
        ));
        Self {
            label,
            started: jiff::Timestamp::now(),
            deadline,
            limits,
            capture,
            stop,
            owner,
        }
    }

    /// Ends the test now and drops its bytes.
    pub(crate) fn stop(&self) {
        self.stop.notify_one();
        if let Ok(mut capture) = self.capture.lock() {
            capture.finish(End::Stopped);
        }
    }

    /// Copies the current state for display; the bytes stay here.
    pub(crate) fn view(&self) -> View {
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        let unavailable = Capture {
            connected: None,
            peer: None,
            closed: false,
            bytes: Vec::new(),
            seen: 0,
            end: Some(End::Failed("test state unavailable".into())),
            guess: Guess::Nothing,
            framing: Vec::new(),
            handover: None,
        };
        let capture = self.capture.lock();
        let capture = capture.as_deref().unwrap_or(&unavailable);
        View {
            label: self.label.clone(),
            started: self.started,
            remaining,
            connected: capture.connected,
            peer: capture.peer,
            closed: capture.closed,
            size_limit_reached: capture.seen >= self.limits.bytes,
            #[cfg(test)]
            bytes: capture.bytes.clone(),
            seen: capture.seen,
            end: capture.end.clone(),
            guess: capture.guess,
            framing: capture.framing.clone(),
        }
    }
}

/// Reads until the deadline or a stop, then drops the bytes. A closed connection
/// or a full buffer keeps the bytes visible until then; a failure ends at once.
async fn run(
    target: Target,
    limits: Limits,
    deadline: tokio::time::Instant,
    capture: Arc<Mutex<Capture>>,
    stop: Arc<Notify>,
) {
    let reading = read(target, limits, deadline, capture.clone());
    tokio::pin!(reading);
    let mut reading_done = false;
    let end = loop {
        tokio::select! {
            () = tokio::time::sleep_until(deadline) => break End::TimeLimit,
            () = stop.notified() => break End::Stopped,
            early = &mut reading, if !reading_done => {
                reading_done = true;
                match early {
                    Some(End::Closed) => {
                        if let Ok(mut capture) = capture.lock() {
                            capture.closed = true;
                        }
                    }
                    Some(end) => break end,
                    None => {}
                }
            }
        }
    };
    if let Ok(mut capture) = capture.lock() {
        capture.finish(end);
    }
}

/// Reads from the target until it closes, fails or the size bound is reached.
/// Returns `None` when the bound was reached, so the deadline decides the end.
async fn read(
    target: Target,
    limits: Limits,
    deadline: tokio::time::Instant,
    capture: Arc<Mutex<Capture>>,
) -> Option<End> {
    let failed = |action: &str, error: &io::Error| Some(End::Failed(format!("{action}: {error}")));
    let mut peer = None;
    let mut stream = match target {
        Target::Serial {
            name,
            settings,
            open,
        } => return read_serial(name, settings, open, limits, capture).await,
        Target::TcpServer(address) => {
            let listener = match tokio::net::TcpListener::bind(address).await {
                Ok(listener) => listener,
                Err(error) => return failed("listen", &error),
            };
            match tokio::time::timeout_at(deadline, listener.accept()).await {
                Ok(Ok((stream, from))) => {
                    peer = Some(from.ip().to_canonical());
                    stream
                }
                Ok(Err(error)) => return failed("accept", &error),
                Err(_) => return Some(End::TimeLimit),
            }
            // The listener drops here, so a second connection is refused.
        }
        Target::TcpClient(address) => {
            match tokio::time::timeout_at(deadline, tokio::net::TcpStream::connect(address)).await {
                Ok(Ok(stream)) => stream,
                Ok(Err(error)) => return failed("connect", &error),
                Err(_) => return Some(End::TimeLimit),
            }
        }
    };
    if let Ok(mut capture) = capture.lock() {
        capture.connect();
        capture.peer = peer;
    }
    let mut buffer = [0_u8; 4096];
    loop {
        match stream.read(&mut buffer).await {
            Ok(0) => return Some(End::Closed),
            Ok(count) => {
                let chunk = buffer.get(..count).unwrap_or_default();
                let more = capture
                    .lock()
                    .is_ok_and(|mut capture| capture.take(chunk, limits.bytes));
                if !more {
                    return None;
                }
            }
            Err(error) => return failed("read", &error),
        }
    }
}

/// Reads a serial port on a blocking thread owned by this test; it never writes.
async fn read_serial(
    name: String,
    settings: serial::Settings,
    open: Open,
    limits: Limits,
    capture: Arc<Mutex<Capture>>,
) -> Option<End> {
    // Set when this future is dropped, so the thread ends within one read timeout.
    struct Halt(Arc<AtomicBool>);
    impl Drop for Halt {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Release);
        }
    }
    let halt = Halt(Arc::new(AtomicBool::new(false)));
    let halted = halt.0.clone();
    let reader = tokio::task::spawn_blocking(move || {
        let mut port = match open(&name, &settings) {
            Ok(port) => port,
            Err(error) => return Some(End::Failed(format!("open {name}: {error}"))),
        };
        if let Ok(mut capture) = capture.lock() {
            capture.connect();
        }
        let mut buffer = [0_u8; 4096];
        while !halted.load(Ordering::Acquire) {
            match port.read(&mut buffer) {
                Ok(0) => return Some(End::Closed),
                Ok(count) => {
                    let chunk = buffer.get(..count).unwrap_or_default();
                    let more = capture
                        .lock()
                        .is_ok_and(|mut capture| capture.take(chunk, limits.bytes));
                    if !more {
                        return None;
                    }
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                    ) => {}
                Err(error) => return Some(End::Failed(format!("read {name}: {error}"))),
            }
        }
        Some(End::Stopped)
    });
    let result = reader.await;
    drop(halt);
    result.unwrap_or_else(|error| Some(End::Failed(format!("serial reader: {error}"))))
}

#[cfg(test)]
mod tests {
    use super::{End, Guess, Limits, Session, Target};
    use std::{
        io,
        sync::{Arc, Mutex},
        time::Duration,
    };
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    const SHORT: Limits = Limits {
        duration: Duration::from_millis(1500),
        bytes: 16,
    };

    async fn until(session: &Session, ready: impl Fn(&super::View) -> bool) -> super::View {
        for _ in 0..200 {
            let view = session.view();
            if ready(&view) {
                return view;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        session.view()
    }

    /// Reads what the test sent to the peer until it closes or `wait` passes.
    async fn sent_to_peer(peer: &mut tokio::net::TcpStream, wait: Duration) -> io::Result<usize> {
        let mut received = Vec::new();
        if let Ok(result) = tokio::time::timeout(wait, peer.read_to_end(&mut received)).await {
            result?;
        }
        Ok(received.len())
    }

    // Owner request 2026-11-18 (setup page): the test connects as a client,
    // shows what the machine sends and writes zero bytes to it.
    #[tokio::test]
    async fn client_test_shows_bytes_and_sends_nothing() -> Result<(), crate::Error> {
        let machine = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let session = Session::start(
            "test".into(),
            Target::TcpClient(machine.local_addr()?),
            Limits {
                duration: Duration::from_secs(10),
                bytes: 1024,
            },
        );
        let (mut peer, _) =
            tokio::time::timeout(Duration::from_secs(5), machine.accept()).await??;
        peer.write_all(b"\x05H|\\^&\r").await?;
        let view = until(&session, |view| view.bytes.len() == 7).await;
        assert_eq!(view.bytes, b"\x05H|\\^&\r");
        assert!(view.connected.is_some());
        assert_eq!(view.framing, [0x05, b'\r']);
        session.stop();
        drop(session);
        assert_eq!(sent_to_peer(&mut peer, Duration::from_secs(5)).await?, 0);
        Ok(())
    }

    // Owner request 2026-11-19 (item 5): the protocol guess outlives the bytes,
    // so the verdict still shows once the test has dropped them. Owner decision
    // 2026-11-20 (admin restyle b4): so do the framing characters, the only
    // part of the stream the page shows.
    #[tokio::test]
    async fn guess_is_kept_after_the_bytes_are_dropped() -> Result<(), crate::Error> {
        let machine = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let session = Session::start(
            "test".into(),
            Target::TcpClient(machine.local_addr()?),
            SHORT,
        );
        let (mut peer, _) =
            tokio::time::timeout(Duration::from_secs(5), machine.accept()).await??;
        peer.write_all(b"\x05\x021H|").await?;
        let view = until(&session, |view| view.bytes.len() == 5).await;
        let astm = Guess::Family(super::verdict::Family::Astm);
        assert_eq!(view.guess, astm);
        session.stop();
        let stopped = session.view();
        assert!(stopped.bytes.is_empty());
        assert_eq!(stopped.guess, astm);
        assert_eq!(stopped.framing, [0x05, 0x02]);
        assert_eq!(stopped.seen, 5);
        assert_eq!(sent_to_peer(&mut peer, Duration::from_secs(5)).await?, 0);
        Ok(())
    }

    // Owner request 2026-11-18 (setup page): as a server the test accepts one
    // connection, reads it and writes zero bytes.
    #[tokio::test]
    async fn server_test_accepts_one_connection_and_sends_nothing() -> Result<(), crate::Error> {
        let reserved = std::net::TcpListener::bind("127.0.0.1:0")?;
        let address = reserved.local_addr()?;
        drop(reserved);
        let session = Session::start("test".into(), Target::TcpServer(address), SHORT);
        let mut peer = None;
        for _ in 0..100 {
            if let Ok(stream) = tokio::net::TcpStream::connect(address).await {
                peer = Some(stream);
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let mut peer = peer.ok_or("test listener never opened")?;
        peer.write_all(b"MSH|^~\\&|").await?;
        let view = until(&session, |view| view.seen == 9).await;
        assert_eq!(view.bytes, b"MSH|^~\\&|");
        // ADR 0026 decision 2: the test keeps who connected, for step 4.
        assert_eq!(view.peer, Some(std::net::IpAddr::from([127, 0, 0, 1])));
        assert_eq!(sent_to_peer(&mut peer, Duration::from_secs(5)).await?, 0);
        Ok(())
    }

    // AGENTS.md rule 7: the test keeps at most its size bound, stops reading
    // there, and drops every byte when its time bound passes.
    #[tokio::test]
    async fn test_is_bounded_in_size_and_time_and_then_discards() -> Result<(), crate::Error> {
        let machine = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let started = std::time::Instant::now();
        let session = Session::start(
            "test".into(),
            Target::TcpClient(machine.local_addr()?),
            SHORT,
        );
        let (mut peer, _) =
            tokio::time::timeout(Duration::from_secs(5), machine.accept()).await??;
        peer.write_all(&[b'x'; 100]).await?;
        let view = until(&session, |view| view.size_limit_reached).await;
        assert_eq!(view.bytes.len(), SHORT.bytes);
        assert!(view.size_limit_reached);
        let ended = until(&session, |view| view.end.is_some()).await;
        assert_eq!(ended.end, Some(End::TimeLimit));
        assert!(ended.bytes.is_empty(), "bytes kept after the test ended");
        assert!(started.elapsed() < Duration::from_secs(5));
        assert_eq!(sent_to_peer(&mut peer, Duration::from_secs(5)).await?, 0);
        Ok(())
    }

    /// A fake serial port: scripted input, recorded writes.
    struct Port {
        input: Vec<u8>,
        written: Arc<Mutex<Vec<u8>>>,
    }

    impl io::Read for Port {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            if self.input.is_empty() {
                std::thread::sleep(Duration::from_millis(10));
                return Err(io::ErrorKind::TimedOut.into());
            }
            let count = buffer.len().min(self.input.len());
            buffer
                .get_mut(..count)
                .ok_or_else(|| io::Error::other("buffer"))?
                .copy_from_slice(self.input.drain(..count).as_slice());
            Ok(count)
        }
    }

    impl io::Write for Port {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.written
                .lock()
                .map_err(|_| io::Error::other("poisoned"))?
                .extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    // Owner request 2026-11-18 (setup page): on a serial port the test only
    // reads; the fake port records zero written bytes.
    #[tokio::test]
    async fn serial_test_reads_and_never_writes() -> Result<(), crate::Error> {
        use analyte_transport::serial;
        let written = Arc::new(Mutex::new(Vec::new()));
        let recorded = written.clone();
        let open: super::Open = Arc::new(move |name, _| {
            assert_eq!(name, "COM9");
            Ok(Box::new(Port {
                input: b"\x02ABC\x03".to_vec(),
                written: recorded.clone(),
            }) as Box<dyn serial::Port>)
        });
        let settings = serial::Settings::new(
            serial::Device::Named("COM9".into()),
            9600,
            serial::DataBits::Eight,
            serial::Parity::None,
            serial::StopBits::One,
            serial::FlowControl::None,
            super::SERIAL_READ,
        )?;
        let session = Session::start(
            "COM9".into(),
            Target::Serial {
                name: "COM9".into(),
                settings,
                open,
            },
            SHORT,
        );
        let view = until(&session, |view| view.bytes.len() == 5).await;
        assert_eq!(view.bytes, b"\x02ABC\x03");
        drop(session);
        tokio::time::sleep(Duration::from_millis(400)).await;
        assert!(written.lock().map_err(|_| "poisoned")?.is_empty());
        Ok(())
    }
}
