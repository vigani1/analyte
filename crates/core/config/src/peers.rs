//! Holds the devices a listening machine link accepts connections from (ADR 0026).
use serde::{Deserialize, Serialize};
use std::net::IpAddr;

/// The devices a machine that connects to Analyte (`tcp_server`) may connect
/// from: 1 to [`AllowedPeers::MOST`] plain IP addresses, each written once.
///
/// An IPv4 address is kept in its plain form, so `::ffff:192.168.1.50` (how a
/// dual-stack listener reports it) is the same device as `192.168.1.50`. No
/// hostname (it would need a lookup the lab does not control), no port (an
/// analyzer picks its own source port) and no range (a range lets other
/// devices on the subnet in, which is what the list is there to stop).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Vec<String>", into = "Vec<String>")]
pub struct AllowedPeers(Vec<IpAddr>);

impl AllowedPeers {
    /// Most addresses one machine accepts: enough for an analyzer with a
    /// spare network card or a standby PC, small enough to read on a page.
    pub const MOST: usize = 16;

    /// The list of `addresses`, checked once at the configuration boundary.
    ///
    /// # Errors
    /// Refuses an empty or overlong list, a wildcard, multicast or broadcast
    /// address, and an address written twice (in either IPv4 form).
    pub fn new(addresses: Vec<IpAddr>) -> Result<Self, &'static str> {
        if addresses.is_empty() || addresses.len() > Self::MOST {
            return Err("allowed_peers lists 1 to 16 IP addresses");
        }
        let mut kept: Vec<IpAddr> = Vec::with_capacity(addresses.len());
        for address in addresses {
            let address = address.to_canonical();
            let broadcast = matches!(address, IpAddr::V4(v4) if v4.is_broadcast());
            if address.is_unspecified() || address.is_multicast() || broadcast {
                return Err(
                    "allowed_peers names single devices: no 0.0.0.0, ::, multicast or broadcast address",
                );
            }
            if kept.contains(&address) {
                return Err("allowed_peers lists each address once");
            }
            kept.push(address);
        }
        Ok(Self(kept))
    }

    /// Parses addresses as written in a site file or typed on the setup page.
    ///
    /// # Errors
    /// Refuses text that is not exactly one IP address (no name, port, range
    /// or surrounding spaces), and what [`AllowedPeers::new`] refuses.
    pub fn parse<S: AsRef<str>>(texts: &[S]) -> Result<Self, &'static str> {
        if texts.len() > Self::MOST {
            return Err("allowed_peers lists 1 to 16 IP addresses");
        }
        let addresses = texts
            .iter()
            .map(|text| {
                text.as_ref().parse::<IpAddr>().map_err(|_| {
                    "allowed_peers holds IP addresses such as 192.168.1.50, without a name, port or range"
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Self::new(addresses)
    }

    /// Whether a connection from `peer` is accepted; the IPv4-mapped IPv6
    /// form of an allowed IPv4 address is accepted too.
    #[must_use]
    pub fn allows(&self, peer: IpAddr) -> bool {
        self.0.contains(&peer.to_canonical())
    }

    /// The addresses, in the order written.
    #[must_use]
    pub fn addresses(&self) -> &[IpAddr] {
        &self.0
    }
}

impl TryFrom<Vec<String>> for AllowedPeers {
    type Error = &'static str;
    fn try_from(texts: Vec<String>) -> Result<Self, Self::Error> {
        Self::parse(&texts)
    }
}

impl From<AllowedPeers> for Vec<String> {
    fn from(peers: AllowedPeers) -> Self {
        peers.0.iter().map(ToString::to_string).collect()
    }
}

impl std::fmt::Display for AllowedPeers {
    /// The addresses separated by ", ", as pages and the setup form show them.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (index, address) in self.0.iter().enumerate() {
            if index > 0 {
                f.write_str(", ")?;
            }
            write!(f, "{address}")?;
        }
        Ok(())
    }
}
