//! Lists this PC's IPv4 addresses with their network adapter's name, so setup can show which address an analyzer should send to.
//!
//! Read-only and bounded: it asks the operating system once per page, keeps at
//! most [`LIST_LIMIT`] addresses and shortens long adapter names. Loopback,
//! self-assigned (169.254.x.x), unspecified and multicast addresses are left
//! out, since an analyzer on the lab network cannot reach them, and so are
//! adapters the system reports as down.
use std::{io, net::Ipv4Addr};

/// Most addresses listed; a PC beside the analyzers has a handful.
pub(crate) const LIST_LIMIT: usize = 32;

/// Adapter names are shortened to this many characters for display.
const NAME_LIMIT: usize = 64;

/// One IPv4 address of this PC and the adapter that holds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Address {
    pub(crate) ip: Ipv4Addr,
    /// The adapter's name as the system shows it, such as "Ethernet".
    pub(crate) adapter: String,
}

/// The system's addresses an analyzer could send to, sorted by adapter name.
///
/// # Errors
/// Returns the operating system's listing failure.
pub(crate) fn list() -> io::Result<Vec<Address>> {
    use if_addrs::{IfAddr, IfOperStatus};
    let found = if_addrs::get_if_addrs()?;
    Ok(pick(found.into_iter().filter_map(|interface| {
        let down = matches!(
            interface.oper_status,
            IfOperStatus::Down | IfOperStatus::NotPresent | IfOperStatus::LowerLayerDown
        );
        match interface.addr {
            IfAddr::V4(v4) if !down => Some((interface.name, v4.ip)),
            IfAddr::V4(_) | IfAddr::V6(_) => None,
        }
    })))
}

/// Keeps the addresses another machine on the network can reach, once each,
/// sorted by adapter name then address, at most [`LIST_LIMIT`].
fn pick(found: impl IntoIterator<Item = (String, Ipv4Addr)>) -> Vec<Address> {
    let mut kept: Vec<Address> = found
        .into_iter()
        .filter(|(_, ip)| {
            !(ip.is_loopback()
                || ip.is_link_local()
                || ip.is_unspecified()
                || ip.is_multicast()
                || ip.is_broadcast())
        })
        .map(|(name, ip)| Address {
            ip,
            adapter: shorten(name.trim()),
        })
        .collect();
    kept.sort_by(|a, b| a.adapter.cmp(&b.adapter).then(a.ip.cmp(&b.ip)));
    kept.dedup();
    kept.truncate(LIST_LIMIT);
    kept
}

/// The name, or "Unnamed adapter", cut to [`NAME_LIMIT`] characters.
fn shorten(name: &str) -> String {
    if name.is_empty() {
        return "Unnamed adapter".into();
    }
    match name.char_indices().nth(NAME_LIMIT) {
        Some((at, _)) => format!("{}…", name.get(..at).unwrap_or(name)),
        None => name.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::{Address, LIST_LIMIT, pick};
    use std::net::Ipv4Addr;

    fn found(name: &str, ip: [u8; 4]) -> (String, Ipv4Addr) {
        (name.to_owned(), Ipv4Addr::from(ip))
    }

    // Owner request 2026-11-21 (network setup): step 2 lists only addresses an
    // analyzer on the lab network can send to, each once, with its adapter.
    #[test]
    fn keeps_reachable_addresses_once_sorted_by_adapter() {
        let kept = pick([
            found("Wi-Fi", [10, 0, 0, 7]),
            found("Loopback Pseudo-Interface 1", [127, 0, 0, 1]),
            found("Ethernet", [192, 168, 1, 20]),
            found("Ethernet 2", [169, 254, 3, 9]),
            found("Ethernet", [192, 168, 1, 20]),
            found("odd", [0, 0, 0, 0]),
            found("odd", [224, 0, 0, 1]),
            found("odd", [255, 255, 255, 255]),
        ]);
        assert_eq!(
            kept,
            [
                Address {
                    ip: Ipv4Addr::new(192, 168, 1, 20),
                    adapter: "Ethernet".into()
                },
                Address {
                    ip: Ipv4Addr::new(10, 0, 0, 7),
                    adapter: "Wi-Fi".into()
                },
            ]
        );
    }

    // AGENTS.md rule 7 (bound everything): the list and each name are bounded.
    #[test]
    fn list_and_names_are_bounded() {
        let many = (0..100_u8).map(|n| found("Ethernet", [10, 1, 0, n]));
        assert_eq!(pick(many).len(), LIST_LIMIT);
        let long = "x".repeat(500);
        let kept = pick([found(&long, [10, 0, 0, 1]), found("  ", [10, 0, 0, 2])]);
        let names: Vec<&str> = kept.iter().map(|a| a.adapter.as_str()).collect();
        assert_eq!(names, ["Unnamed adapter", &format!("{}…", "x".repeat(64))]);
    }
}
