//! Owns TCP sessions and supervises reconnects and listeners.

use crate::{
    Backoff, CHUNK_LIMIT, Cancellation, Connection, Error, Event, IoSide, Limits, Reason,
    connection,
};
use std::{
    net::SocketAddr,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::mpsc,
    task::JoinSet,
};

/// Connects and reconnects only to the explicit peer. Writes never cross sessions.
/// Drop the future or cancel the dialog to close the active socket and stop retrying.
/// # Errors
/// Returns event backpressure or consumer closure; attempt failures are ordered events.
pub async fn client(address: SocketAddr, side: IoSide, retry: Backoff) -> Result<(), Error> {
    run_client(address, side, retry, Reconnect::AfterClose).await
}

/// Retries connection attempts, then pumps exactly one successful TCP session.
/// After closure the caller owns protocol cleanup before starting another session.
/// Dropping the future or cancelling closes the socket and stops connection attempts.
/// # Errors
/// Returns event backpressure or consumer closure; attempt failures are ordered events.
pub async fn client_session(
    address: SocketAddr,
    side: IoSide,
    retry: Backoff,
) -> Result<(), Error> {
    run_client(address, side, retry, Reconnect::CallerOwned).await
}

enum Reconnect {
    AfterClose,
    CallerOwned,
}

async fn run_client(
    address: SocketAddr,
    mut side: IoSide,
    mut retry: Backoff,
    reconnect: Reconnect,
) -> Result<(), Error> {
    let cancel = side.cancel.clone();
    loop {
        let attempt = tokio::select! {
            () = cancel.cancelled() => break,
            result = tokio::time::timeout(side.limits.deadline, TcpStream::connect(address)) => result,
        };
        match attempt {
            Ok(Ok(stream)) => {
                retry.reset();
                session(stream, &mut side).await?;
                if cancel.is_cancelled() {
                    return Ok(());
                }
                match reconnect {
                    Reconnect::AfterClose => (),
                    Reconnect::CallerOwned => return Ok(()),
                }
            }
            Ok(Err(source)) => {
                side.emit(Event::Failed(Reason::Io {
                    endpoint: address.to_string(),
                    operation: "connect",
                    kind: source.kind(),
                }))
                .await?;
            }
            Err(_) => {
                side.emit(Event::Failed(Reason::Deadline(
                    crate::DeadlineKind::Connect,
                )))
                .await?;
            }
        }
        tokio::select! { () = cancel.cancelled() => break, () = tokio::time::sleep(retry.delay()) => {} }
    }
    side.stop();
    side.emit(Event::Closed(Reason::Cancelled)).await
}

/// Binds an explicit address, with no implicit wildcard listener.
/// # Errors
/// Preserves the bind failure with its address.
pub async fn bind(address: SocketAddr) -> Result<TcpListener, Error> {
    tokio::time::timeout(Duration::from_secs(5), TcpListener::bind(address))
        .await
        .map_err(|_| Error::Deadline(crate::DeadlineKind::Bind))?
        .map_err(|source| Error::Io {
            operation: "bind",
            endpoint: address.to_string(),
            source,
        })
}

/// Pumps one socket; closure, failure and cancellation are ordered after received bytes.
/// Cancellation of an in-progress write leaves its outcome uncertain.
/// # Errors
/// Reports event delivery failure; socket failures are also delivered as events.
pub async fn pump(stream: TcpStream, mut side: IoSide) -> Result<(), Error> {
    session(stream, &mut side).await
}

async fn session(stream: TcpStream, side: &mut IoSide) -> Result<(), Error> {
    let result = run_session(stream, side).await;
    side.stop();
    match result {
        Err(Error::ConsumerClosed) => Ok(()),
        result => result,
    }
}

async fn run_session(stream: TcpStream, side: &mut IoSide) -> Result<(), Error> {
    let endpoint = stream
        .peer_addr()
        .map_err(|source| Error::Io {
            operation: "peer address",
            endpoint: "TCP".into(),
            source,
        })?
        .to_string();
    side.start(format!(
        "TCP peer={endpoint} local={}",
        crate::evidence::local_address(stream.local_addr())
    ))?;
    if side.cancel.is_cancelled() {
        side.stop();
        return side.emit(Event::Closed(Reason::Cancelled)).await;
    }
    side.emit(Event::Opened {
        session: side.generation,
    })
    .await?;
    let cancel = side.cancel.clone();
    let result = tokio::select! {
        biased;
        () = cancel.cancelled() => Ok(Reason::Cancelled),
        result = transfer(stream, side, &endpoint) => result,
    };
    side.stop();
    match result {
        Ok(reason) => side.emit(Event::Closed(reason)).await,
        Err(error) => {
            side.emit(Event::Failed(error.reason())).await?;
            side.emit(Event::Closed(error.reason())).await
        }
    }
}

async fn transfer(
    mut stream: TcpStream,
    side: &mut IoSide,
    endpoint: &str,
) -> Result<Reason, Error> {
    let mut buffer = [0_u8; CHUNK_LIMIT];
    let mut silent_since = Instant::now();
    loop {
        side.check_recording().await?;
        if side.peer_silent(&mut silent_since) {
            return Err(Error::Deadline(crate::DeadlineKind::TcpRead));
        }
        tokio::select! {
            write = side.writes.recv() => {
                let Some(write) = write else { return Ok(Reason::Cancelled); };
                let Some(write) = side.next_write(write) else { continue; };
                // Each successful write is recorded separately, so a later failure cannot hide a prefix.
                let mut remaining = write.bytes.as_slice();
                let deadline = tokio::time::Instant::now().checked_add(side.limits.deadline).ok_or(Error::InvalidLimits)?;
                while !remaining.is_empty() {
                    let count = tokio::time::timeout_at(deadline,
                        stream.write(remaining)).await.map_err(|_| Error::Deadline(crate::DeadlineKind::TcpWrite))?.map_err(|source| Error::Io { operation: "write",
                        endpoint: endpoint.into(),
                        source })?;
                                            if count == 0 { return Err(Error::Io { operation: "write",
                        endpoint: endpoint.into(),
                        source: std::io::ErrorKind::WriteZero.into() });
                        }
                    let sent = remaining.get(..count).ok_or(Error::OversizedWrite)?;
                    side.outgoing(sent).await?;
                    remaining = remaining.get(count..).ok_or(Error::OversizedWrite)?;
                }
            }
            read = stream.read(&mut buffer) => {
                let count = read.map_err(|source| Error::Io { operation: "read", endpoint: endpoint.into(), source })?;
                if count == 0 { return Ok(Reason::Disconnect); }
                let at = Instant::now();
                silent_since = at;
                let bytes = buffer.get(..count).ok_or(Error::OversizedWrite)?.to_vec();
                side.emit(Event::Received { at, bytes }).await?;
            }
            () = tokio::time::sleep(Duration::from_millis(2)) => {
                side.check_recording().await?;
                if side.peer_silent(&mut silent_since) { return Err(Error::Deadline(crate::DeadlineKind::TcpRead)); }
            }
        }
    }
}

impl IoSide {
    /// Whether the peer has been silent for the whole deadline while it held the turn.
    /// While the host holds received bytes it has not finished, the peer is waiting
    /// for a reply, so the silence clock restarts instead of ending the session.
    fn peer_silent(&self, since: &mut Instant) -> bool {
        if self.turn.host_holds() {
            *since = Instant::now();
            return false;
        }
        since.elapsed() >= self.limits.deadline
    }
}

/// Server lifecycle events, bounded independently from each connection's byte stream.
pub enum ServerEvent {
    /// A newly accepted connection with its own ordered event stream.
    Accepted {
        /// Remote address.
        peer: SocketAddr,
        /// Dialog endpoint owned by the host.
        connection: Connection,
    },
    /// The socket was closed immediately because the configured limit was reached.
    Refused {
        /// Refused peer.
        peer: SocketAddr,
        /// Explicit failure event with `LimitReached` reason.
        event: Event,
    },
}

/// Runs a bounded server with exactly one owned task per accepted connection.
/// The host supplies an already explicitly bound listener and bounded notification channel.
/// Cancellation closes the listener, cancels all sessions and joins them within the operation bound.
/// Dropping this future aborts the owned task set; no child task is detached.
/// # Errors
/// Rejects invalid limits and reports accept, event backpressure, or task failures.
pub async fn server(
    listener: TcpListener,
    maximum: usize,
    limits: Limits,
    events: mpsc::Sender<ServerEvent>,
    cancel: Cancellation,
) -> Result<(), Error> {
    server_recorded(listener, maximum, limits, events, cancel, |_| None).await
}

/// Runs a server with a separate optional recording tap for each peer.
/// The factory must return promptly; workers remain owned by the host.
/// # Errors
/// Reports the same validation, accept, queue and task failures as `server`.
pub async fn server_recorded<F>(
    listener: TcpListener,
    maximum: usize,
    limits: Limits,
    events: mpsc::Sender<ServerEvent>,
    cancel: Cancellation,
    mut recorder: F,
) -> Result<(), Error>
where
    F: FnMut(SocketAddr) -> Option<crate::recording::Tap>,
{
    if !(1..=1024).contains(&maximum) {
        return Err(Error::InvalidLimits);
    }
    let mut tasks = JoinSet::new();
    let children = Cancellation::default();
    let result = async {
        loop {
            tokio::select! {
                biased;
                () = cancel.cancelled() => return Ok(()),
                done = tasks.join_next(), if !tasks.is_empty() => {
                    done.ok_or(Error::ConsumerClosed)?.map_err(|source| Error::Io { operation: "join TCP owner",
                        endpoint: "server".into(),
                        source: std::io::Error::other(source) })??;
                                        }
                accepted = listener.accept() => {
                    let (stream, peer) = accepted.map_err(|source| Error::Io { operation: "accept", endpoint: "server".into(), source })?;
                    let notification = if tasks.len() >= maximum {
                        drop(stream);
                        ServerEvent::Refused { peer, event: Event::Failed(Reason::LimitReached) }
                    } else {
                        let (connection, mut side) = connection(limits);
                        side.tap = recorder(peer);
                        let shutdown = children.clone();
                        tasks.spawn(async move {
                            let own = side.cancel.clone();
                            // Propagate server shutdown through the same session cancellation path.
                            tokio::select! {
                                result = session(stream, &mut side) => result,
                                () = shutdown.cancelled() => { own.cancel();
                                    side.stop();
                                    side.emit(Event::Closed(Reason::Cancelled)).await }
                            }
                        });
                        ServerEvent::Accepted { peer, connection }
                    };
                    tokio::time::timeout(limits.deadline,
                        events.send(notification))
                        .await
                        .map_err(|_| Error::Deadline(crate::DeadlineKind::ServerNotification))?
                        .map_err(|_| Error::ConsumerClosed)?;
                                        }
            }
        }
    }.await;
    drop(listener);
    children.cancel();
    let shutdown = tokio::time::timeout(limits.deadline.saturating_mul(2), async {
        while let Some(task) = tasks.join_next().await {
            task.map_err(|source| Error::Io {
                operation: "join TCP owner",
                endpoint: "server".into(),
                source: std::io::Error::other(source),
            })??;
        }
        Ok::<(), Error>(())
    })
    .await
    .map_err(|_| Error::Deadline(crate::DeadlineKind::ServerShutdown))?;
    result.and(shutdown)
}
