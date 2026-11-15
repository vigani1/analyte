//! Caps concurrent HTTP connections and their entire wire lifetime.
use std::{
    future::Future as _,
    io,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::{TcpListener, TcpStream},
    sync::{OwnedSemaphorePermit, Semaphore},
    time::Sleep,
};

pub(super) struct Listener {
    pub(super) socket: TcpListener,
    pub(super) slots: Arc<Semaphore>,
}

pub(super) struct Stream {
    socket: TcpStream,
    _slot: OwnedSemaphorePermit,
    expires: Pin<Box<Sleep>>,
}

impl Stream {
    fn expired(&mut self, cx: &mut Context<'_>) -> bool {
        self.expires.as_mut().poll(cx).is_ready()
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
                    return (
                        Stream {
                            socket,
                            _slot: slot,
                            expires: Box::pin(tokio::time::sleep(Duration::from_secs(2))),
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
                "admin connection lifetime exceeded",
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
                "admin connection lifetime exceeded",
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
