//! Validated serial settings and a replaceable blocking port boundary.

use crate::Error;
use serialport::{DataBits, FlowControl, Parity, StopBits};
use std::{
    io::{self, Read, Write},
    time::Duration,
};

/// A blocking port used only by its dedicated owner thread.
/// Implementations must enforce the configured timeout on reads and writes.
pub trait Port: Read + Write + Send {}

impl<T: Read + Write + Send> Port for T {}

/// Serial settings that never search for or substitute another device name.
#[derive(Clone, Debug)]
pub struct Settings {
    name: String,
    baud: u32,
    bits: DataBits,
    parity: Parity,
    stops: StopBits,
    flow: FlowControl,
    timeout: Duration,
}

impl Settings {
    /// Validates a fixed port name, nonzero baud rate and 1ms..=1s I/O timeout.
    ///
    /// # Errors
    /// Rejects empty, oversized or NUL-containing names and invalid numeric limits.
    pub fn new(
        name: String,
        baud: u32,
        bits: DataBits,
        parity: Parity,
        stops: StopBits,
        flow: FlowControl,
        timeout: Duration,
    ) -> Result<Self, Error> {
        if name.is_empty()
            || name.len() > 1024
            || name.contains('\0')
            || baud == 0
            || !(Duration::from_millis(1)..=Duration::from_secs(1)).contains(&timeout)
        {
            return Err(Error::InvalidLimits);
        }
        Ok(Self {
            name,
            baud,
            bits,
            parity,
            stops,
            flow,
            timeout,
        })
    }

    /// Opens exactly the configured device with bounded blocking operations.
    ///
    /// # Errors
    /// Preserves the serial library's error with the configured port name.
    pub fn open(&self) -> Result<Box<dyn serialport::SerialPort>, Error> {
        serialport::new(&self.name, self.baud)
            .data_bits(self.bits)
            .parity(self.parity)
            .stop_bits(self.stops)
            .flow_control(self.flow)
            .timeout(self.timeout)
            .open()
            .map_err(|source| Error::Io {
                operation: "open serial",
                endpoint: self.name.clone(),
                source: io::Error::other(source),
            })
    }
}

/// Runs a replaceable blocking serial port on a dedicated, scoped thread.
/// The caller owns this call; dropping the dialog cancels it within the port timeout
/// plus the event deadline. The factory must bound open time and use only the fixed name.
/// Reopening is deliberately left to the host: queued commands must never cross sessions.
///
/// # Errors
/// Reports open, read, write, queue and thread failures. Partial writes are uncertain.
pub fn run<F>(name: &str, side: crate::IoSide, open: F) -> Result<(), Error>
where
    F: FnOnce() -> Result<Box<dyn Port>, Error> + Send,
{
    std::thread::scope(|scope| {
        scope
            .spawn(move || pump(name, side, open()?))
            .join()
            .map_err(|_| Error::Io {
                operation: "join serial owner",
                endpoint: name.into(),
                source: io::Error::other("serial owner thread panicked"),
            })?
    })
}

fn emit(side: &crate::IoSide, mut event: crate::Event) -> Result<(), Error> {
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
            return Err(Error::Deadline("serial event backpressure"));
        }
        std::thread::park_timeout(Duration::from_millis(1));
    }
}

fn pump(name: &str, mut side: crate::IoSide, mut port: Box<dyn Port>) -> Result<(), Error> {
    emit(&side, crate::Event::Opened)?;
    let mut buffer = [0_u8; crate::CHUNK_LIMIT];
    loop {
        if side.events.is_closed() {
            return Ok(());
        }
        match side.writes.try_recv() {
            Ok(bytes) => port.write_all(&bytes).map_err(|source| Error::Io {
                operation: "serial write (outcome uncertain)",
                endpoint: name.into(),
                source,
            })?,
            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => return Ok(()),
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {}
        }
        match port.read(&mut buffer) {
            Ok(0) => {
                emit(&side, crate::Event::Disconnected)?;
                return Ok(());
            }
            Ok(count) => {
                let bytes = buffer.get(..count).ok_or(Error::OversizedWrite)?.to_vec();
                emit(
                    &side,
                    crate::Event::Received {
                        at: std::time::Instant::now(),
                        bytes,
                    },
                )?;
            }
            Err(error)
                if error.kind() == io::ErrorKind::TimedOut
                    || error.kind() == io::ErrorKind::WouldBlock => {}
            Err(source) => {
                return Err(Error::Io {
                    operation: "serial read (device may have vanished)",
                    endpoint: name.into(),
                    source,
                });
            }
        }
    }
}
