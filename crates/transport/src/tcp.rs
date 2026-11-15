//! TCP byte pumping. The caller owns and cancels each future; no tasks are detached.

use crate::{CHUNK_LIMIT, Error, Event, IoSide};
use std::{net::SocketAddr, time::Instant};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

/// Connects to an explicit peer within the configured deadline, then pumps bytes.
/// Cancellation drops the socket; any write in progress has an uncertain outcome.
///
/// # Errors
/// Reports connect, read, write, consumer, and deadline failures without retrying bytes.
pub async fn client(address: SocketAddr, side: IoSide) -> Result<(), Error> {
    let stream = tokio::time::timeout(side.limits.deadline, TcpStream::connect(address))
        .await
        .map_err(|_| Error::Deadline("connect"))?
        .map_err(|source| Error::Io {
            operation: "connect",
            endpoint: address.to_string(),
            source,
        })?;
    pump(stream, side).await
}

/// Binds only the explicit address supplied by the host.
///
/// # Errors
/// Returns the original bind failure with address context.
pub async fn bind(address: SocketAddr) -> Result<TcpListener, Error> {
    TcpListener::bind(address)
        .await
        .map_err(|source| Error::Io {
            operation: "bind",
            endpoint: address.to_string(),
            source,
        })
}

/// Pumps an accepted or connected socket into bounded queues in read order.
/// The host owns this future, including cancellation and its returned failure.
/// A backpressure timeout stops reading and reports failure, never silently skips bytes.
///
/// # Errors
/// Reports socket failures, queue closure, and elapsed operation deadlines.
pub async fn pump(mut stream: TcpStream, mut side: IoSide) -> Result<(), Error> {
    let endpoint = stream
        .peer_addr()
        .map_err(|source| Error::Io {
            operation: "peer address",
            endpoint: "TCP".into(),
            source,
        })?
        .to_string();
    side.emit(Event::Opened).await?;
    let mut buffer = [0_u8; CHUNK_LIMIT];
    loop {
        tokio::select! {
            biased;
            () = side.events.closed() => return Ok(()),
            write = side.writes.recv() => {
                let Some(bytes) = write else { return Ok(()); };
                tokio::time::timeout(side.limits.deadline, stream.write_all(&bytes)).await.map_err(|_| Error::Deadline("TCP write (outcome uncertain)"))?.map_err(|source| Error::Io { operation: "write", endpoint: endpoint.clone(), source })?;
            }
            read = tokio::time::timeout(side.limits.deadline, stream.read(&mut buffer)) => {
                let count = read.map_err(|_| Error::Deadline("TCP read"))?.map_err(|source| Error::Io { operation: "read", endpoint: endpoint.clone(), source })?;
                if count == 0 { side.emit(Event::Disconnected).await?; return Ok(()); }
                let bytes = buffer.get(..count).ok_or(Error::OversizedWrite)?.to_vec();
                side.emit(Event::Received { at: Instant::now(), bytes }).await?;
            }
        }
    }
}
