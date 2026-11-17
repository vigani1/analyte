//! Validates serial settings and supplies a replaceable blocking port.

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

// Display is a compatibility format for protected transport-open recordings.
impl std::fmt::Display for Settings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Settings { name: ")?;
        analyte_model::quoted(f, &self.name)?;
        let bits = match self.bits {
            DataBits::Five => "Five",
            DataBits::Six => "Six",
            DataBits::Seven => "Seven",
            DataBits::Eight => "Eight",
        };
        let parity = match self.parity {
            Parity::None => "None",
            Parity::Odd => "Odd",
            Parity::Even => "Even",
        };
        let stops = match self.stops {
            StopBits::One => "One",
            StopBits::Two => "Two",
        };
        let flow = match self.flow {
            FlowControl::None => "None",
            FlowControl::Software => "Software",
            FlowControl::Hardware => "Hardware",
        };
        write!(
            f,
            ", baud: {}, bits: {bits}, parity: {parity}, stops: {stops}, flow: {flow}, timeout: ",
            self.baud
        )?;
        if self.timeout == Duration::from_secs(1) {
            f.write_str("1s")?;
        } else {
            // Settings validates 1ms..=1s; retain fractional milliseconds exactly.
            let millis = self.timeout.as_millis();
            let fraction = self.timeout.subsec_nanos() % 1_000_000;
            if fraction == 0 {
                write!(f, "{millis}ms")?;
            } else {
                write!(
                    f,
                    "{millis}.{}ms",
                    format!("{fraction:06}").trim_end_matches('0')
                )?;
            }
        }
        f.write_str(" }")
    }
}

#[test]
fn serial_recording_spellings_are_compatible() -> Result<(), Error> {
    for (bits, parity, stops, flow, timeout, expected) in [
        (
            DataBits::Five,
            Parity::None,
            StopBits::One,
            FlowControl::None,
            Duration::from_millis(1),
            "Settings { name: \"COM1\", baud: 9600, bits: Five, parity: None, stops: One, flow: None, timeout: 1ms }",
        ),
        (
            DataBits::Six,
            Parity::Odd,
            StopBits::Two,
            FlowControl::Software,
            Duration::from_micros(1500),
            "Settings { name: \"COM1\", baud: 9600, bits: Six, parity: Odd, stops: Two, flow: Software, timeout: 1.5ms }",
        ),
        (
            DataBits::Seven,
            Parity::Even,
            StopBits::One,
            FlowControl::Hardware,
            Duration::from_nanos(1_000_001),
            "Settings { name: \"COM1\", baud: 9600, bits: Seven, parity: Even, stops: One, flow: Hardware, timeout: 1.000001ms }",
        ),
        (
            DataBits::Eight,
            Parity::None,
            StopBits::One,
            FlowControl::None,
            Duration::from_secs(1),
            "Settings { name: \"COM1\", baud: 9600, bits: Eight, parity: None, stops: One, flow: None, timeout: 1s }",
        ),
    ] {
        assert_eq!(
            Settings::new("COM1".into(), 9600, bits, parity, stops, flow, timeout)?.to_string(),
            expected
        );
    }
    Ok(())
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

    /// Returns the fixed device name used for every reopen attempt.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
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
                source: io::Error::new(
                    match source.kind() {
                        serialport::ErrorKind::NoDevice => io::ErrorKind::NotFound,
                        serialport::ErrorKind::Io(kind) => kind,
                        serialport::ErrorKind::InvalidInput => io::ErrorKind::InvalidInput,
                        serialport::ErrorKind::Unknown => io::ErrorKind::Other,
                    },
                    source,
                ),
            })
    }
}

/// Runs reconnect supervision on a dedicated, scoped blocking thread.
/// Only the configured settings reach the factory; no port discovery or substitution occurs.
/// The factory and fake ports must honor the settings timeout. Drop/cancel the dialog
/// to stop; no queued write is carried into the next session.
/// # Errors
/// Reports event delivery and thread failure; serial failures are ordered events.
pub fn run<F>(
    settings: &Settings,
    mut side: crate::IoSide,
    mut retry: crate::Backoff,
    mut open: F,
) -> Result<(), Error>
where
    F: FnMut(&Settings) -> Result<Box<dyn Port>, io::Error> + Send,
{
    std::thread::scope(|scope| {
        scope
            .spawn(move || {
                while !side.cancel.is_cancelled() {
                    match open(settings) {
                        Ok(mut port) => {
                            retry.reset();
                            side.start(format!("serial {settings}"))?;
                            let session = side.generation;
                            emit(&mut side, crate::Event::Opened { session })?;
                            let reason = transfer(&mut side, port.as_mut(), &settings.name);
                            side.stop();
                            match reason {
                                Ok(reason) => emit(&mut side, crate::Event::Closed(reason))?,
                                Err(error) => {
                                    let reason = if let Error::Io { source, .. } = &error {
                                        if matches!(
                                            source.kind(),
                                            io::ErrorKind::NotFound
                                                | io::ErrorKind::BrokenPipe
                                                | io::ErrorKind::NotConnected
                                        ) {
                                            crate::Reason::PortVanished(settings.name.clone())
                                        } else {
                                            error.reason()
                                        }
                                    } else {
                                        error.reason()
                                    };
                                    emit(&mut side, crate::Event::Failed(reason.clone()))?;
                                    emit(&mut side, crate::Event::Closed(reason))?;
                                }
                            }
                        }
                        Err(source) => {
                            let reason = if source.kind() == io::ErrorKind::NotFound {
                                crate::Reason::PortVanished(settings.name.clone())
                            } else {
                                crate::Reason::Io {
                                    endpoint: settings.name.clone(),
                                    operation: "open serial",
                                    kind: source.kind(),
                                }
                            };
                            emit(&mut side, crate::Event::Failed(reason))?;
                        }
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
                endpoint: settings.name.clone(),
                source: io::Error::other("serial owner thread panicked"),
            })?
    })
}

/// Opens and supervises the fixed operating system serial device.
/// # Errors
/// Reports the same owner and event errors as `run`; device errors are events.
pub fn run_system(
    settings: &Settings,
    side: crate::IoSide,
    retry: crate::Backoff,
) -> Result<(), Error> {
    run(settings, side, retry, |settings| {
        settings
            .open()
            .map(|port| Box::new(port) as Box<dyn Port>)
            .map_err(|error| match error {
                Error::Io { source, .. } => source,
                Error::InvalidLimits
                | Error::OversizedWrite
                | Error::ConsumerClosed
                | Error::Offline
                | Error::Deadline(_) => io::Error::other(error),
            })
    })
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
                if write.session != side.generation {
                    continue;
                }
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
