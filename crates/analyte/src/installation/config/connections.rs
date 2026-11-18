//! Refuses two machines on one connection: a serial port, a USB serial number, a network address or a folder.
//!
//! Two machines on one connection would race for the same bytes, so the program
//! refuses it at load time, and the setup page checks it before writing.
use super::{Error, Machine};
use crate::machine::link::status::Link;
use analyte_config::Endpoint;

/// What two links have in common, for the refusal message, or `None` when they
/// can run side by side. A port given by name and one given by USB serial number
/// can only be compared against the live port list, which the setup page does.
pub(crate) fn shared(left: (Link, &Endpoint), right: (Link, &Endpoint)) -> Option<String> {
    match (left, right) {
        ((Link::Serial, Endpoint::Fixed(a)), (Link::Serial, Endpoint::Fixed(b))) => {
            same_port_name(a, b).then(|| format!("serial port {a}"))
        }
        ((Link::Serial, Endpoint::UsbSerial(a)), (Link::Serial, Endpoint::UsbSerial(b))) => a
            .as_str()
            .eq_ignore_ascii_case(b.as_str())
            .then(|| format!("USB serial number {}", a.as_str())),
        ((Link::TcpServer, a), (Link::TcpServer, b)) => {
            listeners_overlap(a, b).then(|| format!("network address {}", text(a)))
        }
        ((Link::TcpClient, Endpoint::Fixed(a)), (Link::TcpClient, Endpoint::Fixed(b))) => {
            (a == b).then(|| format!("network address {a}"))
        }
        ((Link::Folder, Endpoint::Fixed(a)), (Link::Folder, Endpoint::Fixed(b))) => {
            (a == b).then(|| format!("folder {a}"))
        }
        ((Link::Serial | Link::TcpServer | Link::TcpClient | Link::Folder, _), _) => None,
    }
}

/// Windows port names ignore case (`COM3` is `com3`); device paths elsewhere do not.
fn same_port_name(left: &str, right: &str) -> bool {
    if cfg!(windows) {
        left.eq_ignore_ascii_case(right)
    } else {
        left == right
    }
}

fn text(endpoint: &Endpoint) -> &str {
    match endpoint {
        Endpoint::Fixed(text) => text,
        Endpoint::UsbSerial(number) => number.as_str(),
    }
}

/// Two socket addresses on the same port, where either listens on every address.
/// Port 0 asks the system for a free port, so it never collides.
pub(crate) fn listeners_overlap(left: &Endpoint, right: &Endpoint) -> bool {
    let (Endpoint::Fixed(left), Endpoint::Fixed(right)) = (left, right) else {
        return false;
    };
    match (
        left.parse::<std::net::SocketAddr>(),
        right.parse::<std::net::SocketAddr>(),
    ) {
        (Ok(left), Ok(right)) => {
            left.port() != 0
                && left.port() == right.port()
                && (left.ip() == right.ip()
                    || left.ip().is_unspecified()
                    || right.ip().is_unspecified())
        }
        (Err(_), _) | (_, Err(_)) => left == right,
    }
}

/// The link and endpoint a loaded machine opens.
pub(crate) fn of(machine: &Machine) -> (Link, &Endpoint) {
    let effective = machine.snapshot().effective();
    (Link::of(effective.stack()), &effective.site().endpoint)
}

/// Refuses the first pair of machines that share a connection.
pub(super) fn check(machines: &[Machine]) -> Result<(), Error> {
    for (index, machine) in machines.iter().enumerate() {
        for earlier in machines.iter().take(index) {
            if let Some(what) = shared(of(earlier), of(machine)) {
                return Err(Error::SharedConnection {
                    first: earlier.id().as_str().into(),
                    second: machine.id().as_str().into(),
                    what,
                });
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use analyte_config::UsbSerial;

    fn fixed(text: &str) -> Endpoint {
        Endpoint::Fixed(text.into())
    }

    // Owner request 2026-11-18 (add machines): two machines cannot share a serial
    // port, a USB serial number, a listening address, a machine address or a folder.
    #[test]
    fn two_machines_cannot_share_a_connection() -> Result<(), crate::Error> {
        let usb = |n: &str| UsbSerial::new(n.into()).map(Endpoint::UsbSerial);
        let pairs = [
            (
                (Link::Serial, fixed("COM3")),
                (Link::Serial, fixed("COM3")),
                true,
            ),
            (
                (Link::Serial, fixed("COM3")),
                (Link::Serial, fixed("COM4")),
                false,
            ),
            (
                (Link::Serial, usb("A10K3XYZ")?),
                (Link::Serial, usb("a10k3xyz")?),
                true,
            ),
            (
                (Link::Serial, usb("A10K3XYZ")?),
                (Link::Serial, usb("B20")?),
                false,
            ),
            (
                (Link::TcpServer, fixed("0.0.0.0:5600")),
                (Link::TcpServer, fixed("10.0.0.2:5600")),
                true,
            ),
            (
                (Link::TcpServer, fixed("10.0.0.1:5600")),
                (Link::TcpServer, fixed("10.0.0.2:5600")),
                false,
            ),
            (
                (Link::TcpServer, fixed("127.0.0.1:0")),
                (Link::TcpServer, fixed("127.0.0.1:0")),
                false,
            ),
            (
                (Link::TcpClient, fixed("10.0.0.9:5600")),
                (Link::TcpClient, fixed("10.0.0.9:5600")),
                true,
            ),
            (
                (Link::TcpServer, fixed("10.0.0.9:5600")),
                (Link::TcpClient, fixed("10.0.0.9:5600")),
                false,
            ),
            (
                (Link::Folder, fixed("/srv/in")),
                (Link::Folder, fixed("/srv/in")),
                true,
            ),
        ];
        for ((left_link, left), (right_link, right), expected) in &pairs {
            assert_eq!(
                shared((*left_link, left), (*right_link, right)).is_some(),
                *expected,
                "{left:?} {right:?}"
            );
        }
        assert_eq!(
            shared(
                (Link::Serial, &usb("A10K3XYZ")?),
                (Link::Serial, &usb("A10K3XYZ")?)
            ),
            Some("USB serial number A10K3XYZ".into())
        );
        Ok(())
    }
}
