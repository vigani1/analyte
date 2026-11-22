//! Validates serial line settings and opens a port; USB lookup and the port owner live in children.

use crate::Error;
/// Character width and stop bits, converted from their configured counts by the
/// serial library's own checked conversions.
pub use serialport::{DataBits, StopBits};
use std::{
    io::{self, Read, Write},
    time::Duration,
};

mod lookup;
mod owner;
#[cfg(test)]
mod tests;

pub use lookup::{LIST_LIMIT, Listed, Lookup, list, locate, same_usb_serial};
pub(crate) use owner::emit;
pub use owner::run;

/// The standard line speeds, in bits per second, that a configuration may
/// choose: the rates both Windows and the POSIX serial interface name, from
/// 300 to 115200. Analyzer LIS settings screens offer these.
pub const BAUD_RATES: [u32; 10] = [
    300, 600, 1200, 2400, 4800, 9600, 19200, 38400, 57600, 115_200,
];

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

impl Device {
    /// The configured text: the port name or the USB serial number.
    #[must_use]
    pub fn text(&self) -> &str {
        match self {
            Self::Named(text) | Self::UsbSerial(text) => text,
        }
    }
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
        let text = device.text();
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
