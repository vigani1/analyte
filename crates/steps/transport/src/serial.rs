//! Validates serial settings, finds USB adapters by serial number and supervises a blocking port.

use crate::Error;
/// Character width and stop bits, converted from their configured counts by the
/// serial library's own checked conversions.
pub use serialport::{DataBits, StopBits};
use std::{
    io::{self, Read, Write},
    time::Duration,
};

/// A blocking port used only by its dedicated owner thread.
/// Implementations must enforce the configured timeout on reads and writes.
pub trait Port: Read + Write + Send {}

impl<T: Read + Write + Send> Port for T {}

/// Parity bit on each character.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Parity {
    /// No parity bit.
    None,
    /// An odd count of one bits.
    Odd,
    /// An even count of one bits.
    Even,
}

/// Flow control on the line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlowControl {
    /// No flow control.
    None,
    /// Hardware handshake lines.
    Hardware,
    /// XON/XOFF bytes.
    Software,
}

// The one mapping from line settings to the serial library.
impl From<Parity> for serialport::Parity {
    fn from(parity: Parity) -> Self {
        match parity {
            Parity::None => Self::None,
            Parity::Odd => Self::Odd,
            Parity::Even => Self::Even,
        }
    }
}
impl From<FlowControl> for serialport::FlowControl {
    fn from(flow: FlowControl) -> Self {
        match flow {
            FlowControl::None => Self::None,
            FlowControl::Hardware => Self::Hardware,
            FlowControl::Software => Self::Software,
        }
    }
}

/// Which serial device to open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Device {
    /// This exact port name on every open (`COM9`, `/dev/ttyUSB0`).
    Named(String),
    /// The one port whose USB adapter reports this serial number, looked up
    /// again before every open. No match or several matches open nothing.
    UsbSerial(String),
}

/// A serial port the operating system lists, with its USB serial number if any.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listed {
    /// Port name to open.
    pub name: String,
    /// Serial number the USB adapter reports; `None` for other ports.
    pub usb_serial: Option<String>,
}

/// Most listed ports a lookup considers; a longer list is refused, never cut.
pub const LIST_LIMIT: usize = 1024;

/// Lists the serial ports the operating system reports.
///
/// Windows reads the serial number from each port's USB hardware ID. Linux reads
/// `/sys/class/tty` and the USB device's `serial` file, so no system library is
/// needed. An adapter without its own serial number reports none, or on Windows
/// a number Windows made up that can change with the USB socket.
///
/// # Errors
/// Returns the listing failure, or `InvalidData` when more than [`LIST_LIMIT`]
/// ports are listed.
pub fn list() -> io::Result<Vec<Listed>> {
    // The serial library panics instead of failing when this folder is missing.
    #[cfg(target_os = "linux")]
    if !std::path::Path::new("/sys/class/tty").is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "/sys/class/tty is missing",
        ));
    }
    let ports = serialport::available_ports().map_err(io_error)?;
    if ports.len() > LIST_LIMIT {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("more than {LIST_LIMIT} serial ports listed"),
        ));
    }
    Ok(ports
        .into_iter()
        .map(|port| Listed {
            name: port.port_name,
            usb_serial: match port.port_type {
                serialport::SerialPortType::UsbPort(usb) => usb.serial_number,
                serialport::SerialPortType::PciPort
                | serialport::SerialPortType::BluetoothPort
                | serialport::SerialPortType::Unknown => None,
            },
        })
        .collect())
}

/// Keeps the serial library's error with its portable kind; a missing device is `NotFound`.
fn io_error(source: serialport::Error) -> io::Error {
    io::Error::new(
        match source.kind() {
            serialport::ErrorKind::NoDevice => io::ErrorKind::NotFound,
            serialport::ErrorKind::Io(kind) => kind,
            serialport::ErrorKind::InvalidInput => io::ErrorKind::InvalidInput,
            serialport::ErrorKind::Unknown => io::ErrorKind::Other,
        },
        source,
    )
}

/// Why a USB serial number named no single port.
#[derive(Debug, thiserror::Error)]
pub enum Lookup {
    /// No listed port reports the serial number.
    #[error("no USB serial adapter with serial number {serial} is connected")]
    NotConnected {
        /// Configured serial number.
        serial: String,
    },
    /// Several listed ports report the serial number; none is opened.
    #[error(
        "{} USB serial ports report serial number {serial} ({}); none was opened",
        ports.len(),
        ports.join(", ")
    )]
    Ambiguous {
        /// Configured serial number.
        serial: String,
        /// Every matching port name.
        ports: Vec<String>,
    },
    /// The operating system could not list its serial ports.
    #[error("listing serial ports to find USB serial number {serial} failed: {source}")]
    Listing {
        /// Configured serial number.
        serial: String,
        /// Original listing error.
        source: io::Error,
    },
}

impl Lookup {
    /// The reason carried on the event stream for this failed lookup.
    #[must_use]
    pub fn reason(&self) -> crate::Reason {
        match self {
            Self::NotConnected { serial } => crate::Reason::UsbSerialNotConnected(serial.clone()),
            Self::Ambiguous { serial, ports } => crate::Reason::UsbSerialAmbiguous {
                serial: serial.clone(),
                ports: ports.clone(),
            },
            Self::Listing { serial, source } => crate::Reason::Io {
                endpoint: serial.clone(),
                operation: "list serial ports",
                kind: source.kind(),
            },
        }
    }
}

/// Finds the port name to open for `device`; lists ports only for a USB serial number.
///
/// Serial numbers compare without ASCII letter case, because Windows reports the
/// letters of a USB hardware ID in upper case. Several matches are refused, so
/// this never chooses between two adapters.
///
/// # Errors
/// Returns [`Lookup`] when no port, several ports or no listing is available.
pub fn locate(
    device: &Device,
    list: impl FnOnce() -> io::Result<Vec<Listed>>,
) -> Result<String, Lookup> {
    let serial = match device {
        Device::Named(name) => return Ok(name.clone()),
        Device::UsbSerial(serial) => serial,
    };
    let listed = list().map_err(|source| Lookup::Listing {
        serial: serial.clone(),
        source,
    })?;
    let mut ports: Vec<String> = listed
        .into_iter()
        .filter(|port| {
            port.usb_serial
                .as_deref()
                .is_some_and(|reported| reported.eq_ignore_ascii_case(serial))
        })
        .map(|port| port.name)
        .collect();
    ports.sort();
    ports.dedup();
    match <[String; 1]>::try_from(ports) {
        Ok([port]) => Ok(port),
        Err(ports) if ports.is_empty() => Err(Lookup::NotConnected {
            serial: serial.clone(),
        }),
        Err(ports) => Err(Lookup::Ambiguous {
            serial: serial.clone(),
            ports,
        }),
    }
}

/// Serial line settings and the device they open.
#[derive(Clone, Debug)]
pub struct Settings {
    device: Device,
    baud: u32,
    bits: DataBits,
    parity: Parity,
    stops: StopBits,
    flow: FlowControl,
    timeout: Duration,
}

/// The protected transport-open record for one opened port; a compatibility format.
struct Opened<'a> {
    settings: &'a Settings,
    name: &'a str,
}

impl std::fmt::Display for Opened<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let settings = self.settings;
        f.write_str("Settings { name: ")?;
        analyte_model::quoted(f, self.name)?;
        if let Device::UsbSerial(serial) = &settings.device {
            f.write_str(", usb_serial: ")?;
            analyte_model::quoted(f, serial)?;
        }
        let bits = match settings.bits {
            DataBits::Five => "Five",
            DataBits::Six => "Six",
            DataBits::Seven => "Seven",
            DataBits::Eight => "Eight",
        };
        let parity = match settings.parity {
            Parity::None => "None",
            Parity::Odd => "Odd",
            Parity::Even => "Even",
        };
        let stops = match settings.stops {
            StopBits::One => "One",
            StopBits::Two => "Two",
        };
        let flow = match settings.flow {
            FlowControl::None => "None",
            FlowControl::Software => "Software",
            FlowControl::Hardware => "Hardware",
        };
        write!(
            f,
            ", baud: {}, bits: {bits}, parity: {parity}, stops: {stops}, flow: {flow}, timeout: ",
            settings.baud
        )?;
        if settings.timeout == Duration::from_secs(1) {
            f.write_str("1s")?;
        } else {
            // Settings validates 1ms..=1s; retain fractional milliseconds exactly.
            let millis = settings.timeout.as_millis();
            let fraction = settings.timeout.subsec_nanos() % 1_000_000;
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
        let settings = Settings::new(
            Device::Named("COM1".into()),
            9600,
            bits,
            parity,
            stops,
            flow,
            timeout,
        )?;
        let opened = Opened {
            settings: &settings,
            name: "COM1",
        };
        assert_eq!(opened.to_string(), expected);
    }
    Ok(())
}

// Plan §11
#[test]
fn usb_open_record_names_port_and_serial() -> Result<(), Error> {
    let settings = Settings::new(
        Device::UsbSerial("A10K3XYZ".into()),
        9600,
        DataBits::Eight,
        Parity::None,
        StopBits::One,
        FlowControl::None,
        Duration::from_secs(1),
    )?;
    let opened = Opened {
        settings: &settings,
        name: "COM12",
    };
    assert_eq!(
        opened.to_string(),
        "Settings { name: \"COM12\", usb_serial: \"A10K3XYZ\", baud: 9600, bits: Eight, parity: None, stops: One, flow: None, timeout: 1s }"
    );
    Ok(())
}

impl Settings {
    /// Validates the device text, a nonzero baud rate and a 1ms..=1s I/O timeout.
    ///
    /// # Errors
    /// Rejects empty, oversized or NUL-containing device text and invalid numeric limits.
    pub fn new(
        device: Device,
        baud: u32,
        bits: DataBits,
        parity: Parity,
        stops: StopBits,
        flow: FlowControl,
        timeout: Duration,
    ) -> Result<Self, Error> {
        let (Device::Named(text) | Device::UsbSerial(text)) = &device;
        if text.is_empty()
            || text.len() > 1024
            || text.contains('\0')
            || baud == 0
            || !(Duration::from_millis(1)..=Duration::from_secs(1)).contains(&timeout)
        {
            return Err(Error::InvalidLimits);
        }
        Ok(Self {
            device,
            baud,
            bits,
            parity,
            stops,
            flow,
            timeout,
        })
    }

    /// Returns the device every open attempt looks for.
    #[must_use]
    pub fn device(&self) -> &Device {
        &self.device
    }

    /// Opens the named port with these line settings and bounded blocking operations.
    ///
    /// # Errors
    /// Returns the serial library's error with its portable kind; a missing
    /// device is `NotFound`.
    pub fn open(&self, name: &str) -> io::Result<Box<dyn Port>> {
        serialport::new(name, self.baud)
            .data_bits(self.bits)
            .parity(self.parity.into())
            .stop_bits(self.stops)
            .flow_control(self.flow.into())
            .timeout(self.timeout)
            .open()
            .map(|port| Box::new(port) as Box<dyn Port>)
            .map_err(io_error)
    }
}

/// Runs reconnect supervision on a dedicated, scoped blocking thread.
///
/// Before every open, `locate` names the port for the configured device: a
/// named port as written, a USB serial number by a fresh lookup. A failed lookup
/// is a `Failed` event and waits the same backoff as a failed open. `open` then
/// opens that port name. Production passes [`locate`] over [`list`] and
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
