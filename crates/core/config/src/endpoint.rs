//! Holds where a site's machine link connects: a fixed endpoint or a USB serial adapter.
use serde::{Deserialize, Serialize};

/// Longest accepted USB serial number, in characters.
const USB_SERIAL_CHARACTERS: usize = 128;

/// Where the site's machine link connects. The site file sets exactly one form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Endpoint {
    /// Port name, numeric socket address or absolute folder path, used exactly as written.
    Fixed(String),
    /// The serial port of the one USB adapter reporting this serial number, looked
    /// up again every time the port opens.
    UsbSerial(UsbSerial),
}

impl Endpoint {
    /// Builds the endpoint from the two site keys; exactly one must be set.
    pub(crate) fn choose(
        endpoint: Option<String>,
        usb_serial: Option<UsbSerial>,
        path: &str,
    ) -> Result<Self, crate::Error> {
        match (endpoint, usb_serial) {
            (Some(endpoint), None) => Ok(Self::Fixed(endpoint)),
            (None, Some(serial)) => Ok(Self::UsbSerial(serial)),
            (Some(_), Some(_)) => Err(crate::invalid(
                path,
                "usb_serial",
                "set either endpoint or usb_serial, not both",
            )),
            (None, None) => Err(crate::invalid(
                path,
                "endpoint",
                "set endpoint (a port name, socket address or folder) or usb_serial (a USB adapter serial number)",
            )),
        }
    }

    /// Returns the fixed endpoint text.
    ///
    /// # Errors
    /// Returns [`NotFixed`] for a USB serial number; validation allows that form
    /// only on serial links, so TCP and folder links never see it.
    pub fn fixed(&self) -> Result<&str, NotFixed> {
        match self {
            Self::Fixed(endpoint) => Ok(endpoint),
            Self::UsbSerial(_) => Err(NotFixed),
        }
    }
}

/// The serial device a serial link opens: the fixed port name, or the one
/// adapter reporting the USB serial number.
impl From<&Endpoint> for analyte_transport::serial::Device {
    fn from(endpoint: &Endpoint) -> Self {
        match endpoint {
            Endpoint::Fixed(name) => Self::Named(name.clone()),
            Endpoint::UsbSerial(serial) => Self::UsbSerial(serial.as_str().to_owned()),
        }
    }
}

/// The site names a USB serial adapter where a fixed endpoint is needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the site names a USB serial adapter, which only a serial link can open")]
pub struct NotFixed;

/// A USB adapter serial number, kept exactly as written: 1 to 128 characters,
/// without control characters or surrounding spaces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct UsbSerial(String);

impl UsbSerial {
    /// Validates a serial number once at the configuration boundary.
    ///
    /// # Errors
    /// Refuses empty or overlong text, control characters and surrounding spaces.
    pub fn new(serial: String) -> Result<Self, &'static str> {
        if serial.is_empty()
            || serial.chars().count() > USB_SERIAL_CHARACTERS
            || serial.chars().any(char::is_control)
            || serial.trim() != serial
        {
            return Err(
                "usb_serial must be 1 to 128 characters without control characters or surrounding spaces",
            );
        }
        Ok(Self(serial))
    }

    /// Borrows the serial number as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for UsbSerial {
    type Error = &'static str;
    fn try_from(serial: String) -> Result<Self, Self::Error> {
        Self::new(serial)
    }
}

impl From<UsbSerial> for String {
    fn from(serial: UsbSerial) -> Self {
        serial.0
    }
}
