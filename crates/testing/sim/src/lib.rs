//! Supplies exact-byte synthetic peers without production analyzer or clinical decisions.
#![forbid(unsafe_code)]
pub use analyte_transport::recording::Exception;
use analyte_transport::recording::{Direction, Event, Link};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
/// Replay failure. Byte diagnostics are protected test output, never ordinary logs.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Exact-byte comparison failed.
    #[error("event {event}: expected {expected}, actual {actual}")]
    Mismatch {
        /// Event index.
        event: usize,
        /// Expected hex bytes.
        expected: String,
        /// Actual hex bytes.
        actual: String,
    },
    /// Configured deadline expired.
    #[error("event {0}: protocol/config deadline expired")]
    Deadline(usize),
    /// Transport failed at the indicated event.
    #[error("event {event}: {source}")]
    Io {
        /// Event index.
        event: usize,
        /// Original failure.
        source: std::io::Error,
    },
    /// Recording cannot be replayed on this seam.
    #[error("event {0}: invalid exception, deadline, or unsupported link transition")]
    Invalid(usize),
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
/// Plays one connected machine conversation on TCP or a bounded serial test seam.
/// `deadline` must come from the protocol/config, never recorded time gaps.
/// Cancellation may leave a written prefix; discard the connection, never retry blindly.
/// Link connect is descriptive; disconnect shuts down writes; serial break is rejected.
/// # Errors
/// Reports event-indexed mismatches, deadline failures, invalid exceptions and I/O errors.
pub async fn replay<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut S,
    events: &[Event],
    exceptions: &[Exception],
    deadline: Duration,
) -> Result<(), Error> {
    if deadline.is_zero() {
        return Err(Error::Invalid(0));
    }
    for exception in exceptions {
        if exception.name.is_empty()
            || exception.bytes.is_empty()
            || !events.get(exception.event).is_some_and(|e| {
                e.direction == Direction::HostToMachine && exception.bytes.end <= e.bytes.len()
            })
        {
            return Err(Error::Invalid(exception.event));
        }
    }
    for (index, event) in events.iter().enumerate() {
        let operation = async {
            match event.link {
                Some(Link::Connect) => return Ok(()),
                Some(Link::Disconnect) => {
                    return stream.shutdown().await.map_err(|source| Error::Io {
                        event: index,
                        source,
                    });
                }
                Some(Link::Break) => return Err(Error::Invalid(index)),
                None => (),
            }
            match event.direction {
                Direction::MachineToHost => {
                    stream
                        .write_all(&event.bytes)
                        .await
                        .map_err(|source| Error::Io {
                            event: index,
                            source,
                        })
                }
                Direction::HostToMachine => {
                    let mut actual = vec![0; event.bytes.len()];
                    stream
                        .read_exact(&mut actual)
                        .await
                        .map_err(|source| Error::Io {
                            event: index,
                            source,
                        })?;
                    let equal =
                        actual
                            .iter()
                            .zip(&event.bytes)
                            .enumerate()
                            .all(|(offset, (a, b))| {
                                a == b
                                    || exceptions
                                        .iter()
                                        .any(|e| e.event == index && e.bytes.contains(&offset))
                            });
                    if !equal {
                        return Err(Error::Mismatch {
                            event: index,
                            expected: hex(&event.bytes),
                            actual: hex(&actual),
                        });
                    }
                    Ok(())
                }
            }
        };
        tokio::time::timeout(deadline, operation)
            .await
            .map_err(|_| Error::Deadline(index))??;
    }
    Ok(())
}
