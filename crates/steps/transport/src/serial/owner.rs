//! Supervises a blocking serial port on its own thread: reconnects, reads, writes and events.

use super::{Device, Lookup, Opened, Port, Settings};
use crate::Error;
use std::{io, time::Duration};

/// Runs reconnect supervision on a dedicated, scoped blocking thread.
///
/// Before every open, `locate` names the port for the configured device: a
/// named port as written, a USB serial number by a fresh lookup. A failed lookup
/// is a `Failed` event and waits the same backoff as a failed open. `open` then
/// opens that port name. Production passes [`locate`](super::locate) over [`list`](super::list) and
/// [`Settings::open`]; tests pass fakes. Ports must honor the settings timeout.
/// Drop or cancel the dialog to stop; no queued write is carried into the next
/// session.
/// # Errors
/// Reports event delivery and thread failure; serial failures are ordered events.
pub fn run<L, F>(
    settings: &Settings,
    mut side: crate::IoSide,
    mut retry: crate::Backoff,
    mut locate: L,
    mut open: F,
) -> Result<(), Error>
where
    L: FnMut(&Device) -> Result<String, Lookup> + Send,
    F: FnMut(&str, &Settings) -> io::Result<Box<dyn Port>> + Send,
{
    std::thread::scope(|scope| {
        scope
            .spawn(move || {
                while !side.cancel.is_cancelled() {
                    match locate(&settings.device) {
                        Ok(name) => attempt(&mut side, &mut retry, settings, &name, &mut open)?,
                        Err(lookup) => emit(&mut side, crate::Event::Failed(lookup.reason()))?,
                    }
                    if side.cancel.is_cancelled() {
                        return Ok(());
                    }
                    let start = std::time::Instant::now();
                    let delay = retry.delay();
                    while start.elapsed() < delay && !side.cancel.is_cancelled() {
                        std::thread::park_timeout(Duration::from_millis(1));
                    }
                }
                emit(&mut side, crate::Event::Closed(crate::Reason::Cancelled))
            })
            .join()
            .map_err(|_| Error::Io {
                operation: "join serial owner",
                endpoint: match &settings.device {
                    Device::Named(text) | Device::UsbSerial(text) => text.clone(),
                },
                source: io::Error::other("serial owner thread panicked"),
            })?
    })
}

/// Opens one located port and serves it until it closes or fails.
fn attempt<F>(
    side: &mut crate::IoSide,
    retry: &mut crate::Backoff,
    settings: &Settings,
    name: &str,
    open: &mut F,
) -> Result<(), Error>
where
    F: FnMut(&str, &Settings) -> io::Result<Box<dyn Port>>,
{
    let mut port = match open(name, settings) {
        Ok(port) => port,
        Err(source) => {
            let reason = if source.kind() == io::ErrorKind::NotFound {
                crate::Reason::PortVanished(name.into())
            } else {
                crate::Reason::Io {
                    endpoint: name.into(),
                    operation: "open serial",
                    kind: source.kind(),
                }
            };
            return emit(side, crate::Event::Failed(reason));
        }
    };
    retry.reset();
    side.start(format!("serial {}", Opened { settings, name }))?;
    let session = side.generation;
    emit(side, crate::Event::Opened { session })?;
    let reason = transfer(side, port.as_mut(), name);
    side.stop();
    match reason {
        Ok(reason) => emit(side, crate::Event::Closed(reason)),
        Err(error) => {
            let reason = match &error {
                Error::Io { source, .. }
                    if matches!(
                        source.kind(),
                        io::ErrorKind::NotFound
                            | io::ErrorKind::BrokenPipe
                            | io::ErrorKind::NotConnected
                    ) =>
                {
                    crate::Reason::PortVanished(name.into())
                }
                Error::Io { .. }
                | Error::InvalidLimits
                | Error::OversizedWrite
                | Error::ConsumerClosed
                | Error::Offline
                | Error::Deadline(_) => error.reason(),
            };
            emit(side, crate::Event::Failed(reason.clone()))?;
            emit(side, crate::Event::Closed(reason))
        }
    }
}

pub(crate) fn emit(side: &mut crate::IoSide, event: crate::Event) -> Result<(), Error> {
    let recording_error = side.record_event(&event);
    send(side, event)?;
    if let Some(error) = recording_error {
        send(side, error)?;
    }
    Ok(())
}

fn send(side: &crate::IoSide, mut event: crate::Event) -> Result<(), Error> {
    let start = std::time::Instant::now();
    loop {
        match side.events.try_send(event) {
            Ok(()) => return Ok(()),
            Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                return Err(Error::ConsumerClosed);
            }
            Err(tokio::sync::mpsc::error::TrySendError::Full(returned)) => event = returned,
        }
        if start.elapsed() >= side.limits.deadline {
            return Err(Error::Deadline(crate::DeadlineKind::SerialBackpressure));
        }
        std::thread::park_timeout(Duration::from_millis(1));
    }
}

fn transfer(
    side: &mut crate::IoSide,
    port: &mut dyn Port,
    name: &str,
) -> Result<crate::Reason, Error> {
    let mut buffer = [0_u8; crate::CHUNK_LIMIT];
    loop {
        if side.cancel.is_cancelled() {
            return Ok(crate::Reason::Cancelled);
        }
        if let Some(failure) = side.tap.as_ref().and_then(crate::recording::Tap::failure) {
            side.tap = None;
            send(side, crate::Event::RecordingFailed(failure))?;
        }
        match side.writes.try_recv() {
            Ok(write) => {
                let Some(write) = side.next_write(write) else {
                    continue;
                };
                let start = std::time::Instant::now();
                let mut remaining = write.bytes.as_slice();
                while !remaining.is_empty() {
                    if side.cancel.is_cancelled() {
                        return Ok(crate::Reason::Cancelled);
                    }
                    if start.elapsed() >= side.limits.deadline {
                        return Err(Error::Deadline(crate::DeadlineKind::SerialWrite));
                    }
                    let count = port.write(remaining).map_err(|source| Error::Io {
                        operation: "serial write",
                        endpoint: name.into(),
                        source,
                    })?;
                    if count == 0 {
                        return Err(Error::Io {
                            operation: "serial write",
                            endpoint: name.into(),
                            source: io::ErrorKind::WriteZero.into(),
                        });
                    }
                    let sent = remaining.get(..count).ok_or(Error::OversizedWrite)?;
                    if let Some(event) =
                        side.record(crate::recording::Kind::Out, std::time::Instant::now(), sent)
                    {
                        send(side, event)?;
                    }
                    remaining = remaining.get(count..).ok_or(Error::OversizedWrite)?;
                }
            }
            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                return Ok(crate::Reason::Cancelled);
            }
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {}
        }
        match port.read(&mut buffer) {
            Ok(0) => return Ok(crate::Reason::Disconnect),
            Ok(count) => {
                let bytes = buffer.get(..count).ok_or(Error::OversizedWrite)?.to_vec();
                emit(
                    side,
                    crate::Event::Received {
                        at: std::time::Instant::now(),
                        bytes,
                    },
                )?;
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                ) => {}
            Err(source) => {
                return Err(Error::Io {
                    operation: "serial read",
                    endpoint: name.into(),
                    source,
                });
            }
        }
    }
}
