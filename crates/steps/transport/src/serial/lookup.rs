//! Lists serial ports and finds the one port a USB serial number names.

use super::{Device, io_error};
use std::io;

/// A serial port the operating system lists, with its USB serial number if any.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listed {
    /// Port name to open.
    pub name: String,
    /// Serial number the USB adapter reports; `None` for other ports.
    pub usb_serial: Option<String>,
    /// What the adapter calls itself (product, else maker); `None` when unknown.
    pub description: Option<String>,
}

impl Listed {
    /// The adapter's own factory serial number, which stays the same in every USB
    /// socket. A Windows-made serial number (it contains `&`, such as
    /// `6&2A9C1F&0&1`) is not one: it can change with the socket, so it is `None`.
    #[must_use]
    pub fn factory_serial(&self) -> Option<&str> {
        self.usb_serial
            .as_deref()
            .filter(|serial| !serial.is_empty() && !serial.contains('&'))
    }
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
        .map(|port| {
            let (usb_serial, description) = match port.port_type {
                serialport::SerialPortType::UsbPort(usb) => {
                    (usb.serial_number, usb.product.or(usb.manufacturer))
                }
                serialport::SerialPortType::PciPort => (None, Some("PCI serial port".into())),
                serialport::SerialPortType::BluetoothPort => {
                    (None, Some("Bluetooth serial port".into()))
                }
                serialport::SerialPortType::Unknown => (None, None),
            };
            Listed {
                name: port.port_name,
                usb_serial,
                description,
            }
        })
        .collect())
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
