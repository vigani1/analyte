//! Caps concurrent HTTP connections and their entire wire lifetime.
use analyte_i18n::text as tr;
use std::{
    future::Future as _,
    io,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    task::{Context, Poll},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::{TcpListener, TcpStream},
    sync::{OwnedSemaphorePermit, Semaphore},
    time::{Instant, Sleep},
};

/// How long one connection may live, from accept to its last byte, unless a
/// request it carries keeps it open longer ([`Lifetime::keep_open_for`]).
pub(in crate::admin) const LIFETIME: Duration = Duration::from_secs(2);

/// The end of one connection's life, shared with the requests it carries.
/// The server hands it to a request that may run longer than a page read (a
/// setup write whose form has passed the session check), so its answer is
/// still written before the connection closes. The end only moves later.
#[derive(Clone)]
pub(in crate::admin) struct Lifetime(Arc<End>);

struct End {
    accepted: Instant,
    /// Microseconds after `accepted`; only ever raised.
    after: AtomicU64,
}

impl Lifetime {
    /// A connection accepted at `accepted` that closes `life` later.
    fn new(accepted: Instant, life: Duration) -> Self {
        Self(Arc::new(End {
            accepted,
            after: AtomicU64::new(micros(life)),
        }))
    }

    /// Keeps the connection open for at least `duration` from now.
    pub(in crate::admin) fn keep_open_for(&self, duration: Duration) {
        let until = Instant::now()
            .saturating_duration_since(self.0.accepted)
            .saturating_add(duration);
        self.0.after.fetch_max(micros(until), Ordering::Relaxed);
    }

    /// When the connection closes; `None` only if that is past what a clock
    /// can hold, which [`Self::keep_open_for`]'s bounded callers never reach.
    fn end(&self) -> Option<Instant> {
        self.0
            .accepted
            .checked_add(Duration::from_micros(self.0.after.load(Ordering::Relaxed)))
    }
}

fn micros(duration: Duration) -> u64 {
    u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
}

impl axum::extract::connect_info::Connected<axum::serve::IncomingStream<'_, Listener>>
    for Lifetime
{
    fn connect_info(stream: axum::serve::IncomingStream<'_, Listener>) -> Self {
        stream.io().lifetime.clone()
    }
}

pub(in crate::admin) struct Listener {
    pub(in crate::admin) socket: TcpListener,
    pub(in crate::admin) slots: Arc<Semaphore>,
}

pub(in crate::admin) struct Stream {
    socket: TcpStream,
    _slot: OwnedSemaphorePermit,
    lifetime: Lifetime,
    expires: Pin<Box<Sleep>>,
}

impl Stream {
    /// The timer fires at the end set at accept; a request may since have
    /// moved the end later, and then the timer follows it.
    fn expired(&mut self, cx: &mut Context<'_>) -> bool {
        while self.expires.as_mut().poll(cx).is_ready() {
            match self.lifetime.end() {
                Some(end) if end > self.expires.deadline() => self.expires.as_mut().reset(end),
                Some(_) | None => return true,
            }
        }
        false
    }
}

impl axum::serve::Listener for Listener {
    type Io = Stream;
    type Addr = std::net::SocketAddr;

    async fn accept(&mut self) -> (Stream, Self::Addr) {
        loop {
            match self.socket.accept().await {
                Ok((socket, address)) => {
                    let Ok(slot) = self.slots.clone().try_acquire_owned() else {
                        drop(socket);
                        continue;
                    };
                    let accepted = Instant::now();
                    return (
                        Stream {
                            socket,
                            _slot: slot,
                            lifetime: Lifetime::new(accepted, LIFETIME),
                            expires: Box::pin(tokio::time::sleep_until(
                                accepted.checked_add(LIFETIME).unwrap_or(accepted),
                            )),
                        },
                        address,
                    );
                }
                Err(error) => {
                    tracing::warn!(error_kind = ?error.kind(), "admin listener accept failed");
                    // Back off after an OS accept error; socket readiness cannot resolve that error.
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        }
    }

    fn local_addr(&self) -> io::Result<Self::Addr> {
        self.socket.local_addr()
    }
}

impl AsyncRead for Stream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if self.expired(cx) {
            return Poll::Ready(Err(io::Error::new(
                io::ErrorKind::TimedOut,
                tr("admin-admin-connection-lifetime-exceeded"),
            )));
        }
        Pin::new(&mut self.socket).poll_read(cx, buffer)
    }
}

impl AsyncWrite for Stream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        if self.expired(cx) {
            return Poll::Ready(Err(io::Error::new(
                io::ErrorKind::TimedOut,
                tr("admin-admin-connection-lifetime-exceeded"),
            )));
        }
        Pin::new(&mut self.socket).poll_write(cx, bytes)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.socket).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.socket).poll_shutdown(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    // Safety rule 7 (ADR 0003)
    #[tokio::test]
    async fn expired_connection_refuses_io_and_releases_slot() -> io::Result<()> {
        let socket = TcpListener::bind("127.0.0.1:0").await?;
        let client = TcpStream::connect(socket.local_addr()?).await?;
        let (peer, _) = socket.accept().await?;
        let slots = Arc::new(Semaphore::new(1));
        let mut stream = Stream {
            socket: peer,
            _slot: slots
                .clone()
                .try_acquire_owned()
                .map_err(io::Error::other)?,
            // Inject an already elapsed deadline; no wall-clock wait determines the result.
            lifetime: Lifetime::new(Instant::now(), Duration::ZERO),
            expires: Box::pin(tokio::time::sleep_until(Instant::now())),
        };
        let mut byte = [0];
        assert_eq!(
            stream
                .read(&mut byte)
                .await
                .expect_err("expired read")
                .kind(),
            io::ErrorKind::TimedOut
        );
        assert_eq!(
            stream
                .write(b"private")
                .await
                .expect_err("expired write")
                .kind(),
            io::ErrorKind::TimedOut
        );
        assert_eq!(slots.available_permits(), 0);
        drop(stream);
        assert_eq!(slots.available_permits(), 1);
        drop(client);
        Ok(())
    }

    // Safety rule 7 (ADR 0003); AGENTS.md "Bound everything": a request may move
    // its connection's end later, and the connection then closes at that end.
    #[tokio::test]
    async fn kept_open_connection_closes_at_its_new_end() -> io::Result<()> {
        let socket = TcpListener::bind("127.0.0.1:0").await?;
        let mut client = TcpStream::connect(socket.local_addr()?).await?;
        let (peer, _) = socket.accept().await?;
        let slots = Arc::new(Semaphore::new(1));
        let accepted = Instant::now();
        let lifetime = Lifetime::new(accepted, Duration::ZERO);
        let mut stream = Stream {
            socket: peer,
            _slot: slots
                .clone()
                .try_acquire_owned()
                .map_err(io::Error::other)?,
            lifetime: lifetime.clone(),
            // The end set at accept has passed; only the request's extension keeps it open.
            expires: Box::pin(tokio::time::sleep_until(accepted)),
        };
        lifetime.keep_open_for(Duration::from_millis(50));
        stream.write_all(b"answer").await?;
        let mut answer = [0; 6];
        client.read_exact(&mut answer).await?;
        assert_eq!(&answer, b"answer");
        let moved = lifetime.end().ok_or_else(|| io::Error::other("end"))?;
        assert!(moved > accepted);
        // A shorter request never moves the end back.
        lifetime.keep_open_for(Duration::ZERO);
        assert_eq!(lifetime.end(), Some(moved));
        tokio::time::sleep_until(moved).await;
        assert_eq!(
            stream
                .write(b"late")
                .await
                .expect_err("closed at the new end")
                .kind(),
            io::ErrorKind::TimedOut
        );
        Ok(())
    }
}
