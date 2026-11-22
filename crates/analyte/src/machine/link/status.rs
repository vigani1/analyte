//! Owns each machine's in-memory link light: link kind, state, last data, last change and refused devices.
//!
//! The machine runtime is the only writer; the admin page only subscribes. The status
//! never holds bytes or sample IDs (safety rule 10). The only address it holds is
//! that of a device refused by the machine's `allowed_peers`, so an operator can
//! find it (ADR 0026); an accepted peer's address is never kept here.
use analyte_config::{Stack, TcpRole};
use analyte_transport::Activity;
use jiff::{SignedDuration, Timestamp};
use std::net::IpAddr;
use tokio::sync::watch;

/// How many refused devices a status keeps, newest first (AGENTS.md rule 7).
pub(crate) const REFUSALS_KEPT: usize = 5;

/// A refused device is logged again only after this long, so a device retrying
/// every second cannot flood the log; every refusal is still counted.
const LOG_AGAIN_AFTER: SignedDuration = SignedDuration::from_secs(60);

/// How the machine is attached, as configured.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Link {
    TcpServer,
    TcpClient,
    Serial,
    Folder,
}

impl Link {
    /// The link the machine runtime opens for this stack.
    pub(crate) fn of(stack: &Stack) -> Self {
        use crate::machine::protocols::Link as Runtime;
        match crate::machine::protocols::transport(stack) {
            _ if crate::machine::independent_law(stack).is_some() => Self::TcpServer,
            Runtime::Serial(_) => Self::Serial,
            Runtime::Tcp {
                role: TcpRole::Server,
                ..
            } => Self::TcpServer,
            Runtime::Tcp {
                role: TcpRole::Client,
                ..
            } => Self::TcpClient,
            Runtime::Folder { .. } => Self::Folder,
        }
    }

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::TcpServer => "TCP server",
            Self::TcpClient => "TCP client",
            Self::Serial => "Serial",
            Self::Folder => "Watched folder",
        }
    }
}

/// The colour of a link light: on, waiting for the machine, or off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Light {
    /// Connected, port open or folder watched.
    Live,
    /// Listening; the machine has not connected.
    Waiting,
    /// Not connected, not listening, port not open or folder not watched.
    Off,
}

/// One machine's link as last observed by its runtime.
#[derive(Clone, Debug)]
pub(crate) struct Status {
    link: Link,
    /// A TCP listener is bound or a folder watcher runs.
    ready: bool,
    /// Sessions open now: TCP peers (including independent LAW outbound transactions)
    /// or the serial port.
    open: u32,
    changed_at: Timestamp,
    last_data: Option<Timestamp>,
    /// Devices refused since start, newest first, at most [`REFUSALS_KEPT`].
    refused: Vec<Refusal>,
    /// Present only on a record-only machine: what it recorded since start (ADR 0034).
    recording: Option<Tally>,
}

/// A record-only machine's counts since start; the bytes are in the store.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Tally {
    /// Complete messages kept (an ASTM transfer, an HL7 message, a VIDAS
    /// upload, an Autolyser report).
    pub(crate) messages: u64,
    /// Exchanges that failed framing, a checksum or a deadline; their bytes
    /// are kept too.
    pub(crate) failures: u64,
}

/// Connections refused from one device not on the machine's `allowed_peers`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Refusal {
    /// The refused device's address, in its plain form.
    pub(crate) peer: IpAddr,
    /// When its latest connection was refused.
    pub(crate) at: Timestamp,
    /// How many of its connections were refused since start.
    pub(crate) count: u64,
    /// When its refusal was last logged.
    logged: Timestamp,
}

impl Status {
    pub(crate) const fn link(&self) -> Link {
        self.link
    }

    /// The operator-facing state. A serial port can be open with no cable or machine
    /// attached, so serial says only whether the port is open, never "Connected".
    pub(crate) const fn state(&self) -> &'static str {
        self.reading().0
    }

    /// The colour of the state's light on the admin page.
    pub(crate) const fn light(&self) -> Light {
        self.reading().1
    }

    const fn reading(&self) -> (&'static str, Light) {
        let open = self.open > 0;
        match self.link {
            Link::TcpServer | Link::TcpClient if open => ("Connected", Light::Live),
            Link::TcpServer if self.ready => ("Waiting", Light::Waiting),
            Link::TcpServer => ("Not listening", Light::Off),
            Link::TcpClient => ("Not connected (retrying)", Light::Off),
            Link::Serial if open => ("Port open", Light::Live),
            Link::Serial => ("Port not open", Light::Off),
            Link::Folder if self.ready => ("Watching", Light::Live),
            Link::Folder => ("Not watching", Light::Off),
        }
    }

    /// When the state last changed (service start for an unchanged state).
    pub(crate) const fn changed_at(&self) -> Timestamp {
        self.changed_at
    }

    /// When the runtime last read data from the machine since start, if ever.
    pub(crate) const fn last_data(&self) -> Option<Timestamp> {
        self.last_data
    }

    /// The devices refused since start, the most recently refused first.
    pub(crate) fn refused(&self) -> &[Refusal] {
        &self.refused
    }

    /// What a record-only machine recorded since start; none for a normal one.
    pub(crate) const fn recording(&self) -> Option<Tally> {
        self.recording
    }
}

/// The runtime's writing end. Cloning shares the same status.
#[derive(Clone, Debug)]
pub(crate) struct Reporter(watch::Sender<Status>);

impl Reporter {
    pub(crate) fn new(link: Link) -> Self {
        Self::with(link, None)
    }

    /// The status of a record-only machine, which also counts what it records.
    pub(crate) fn recording(link: Link) -> Self {
        Self::with(link, Some(Tally::default()))
    }

    fn with(link: Link, recording: Option<Tally>) -> Self {
        Self(watch::Sender::new(Status {
            link,
            ready: false,
            open: 0,
            changed_at: Timestamp::now(),
            last_data: None,
            refused: Vec::new(),
            recording,
        }))
    }

    /// Counts one complete message a record-only connection kept.
    pub(crate) fn recorded(&self) {
        self.0.send_modify(|status| {
            if let Some(tally) = &mut status.recording {
                tally.messages = tally.messages.saturating_add(1);
            }
        });
    }

    /// Counts one exchange a record-only connection kept but that failed.
    pub(crate) fn record_failed(&self) {
        self.0.send_modify(|status| {
            if let Some(tally) = &mut status.recording {
                tally.failures = tally.failures.saturating_add(1);
            }
        });
    }

    pub(crate) fn subscribe(&self) -> watch::Receiver<Status> {
        self.0.subscribe()
    }

    /// Marks the listener bound or the watcher running (`true`), or gone (`false`).
    pub(crate) fn ready(&self, ready: bool) {
        self.change(|status| status.ready = ready);
    }

    /// Records that data arrived from the machine now.
    pub(crate) fn received(&self) {
        self.0
            .send_modify(|status| status.last_data = Some(Timestamp::now()));
    }

    /// Records a connection refused from `peer` now. Returns whether it should
    /// be logged: the first refusal from a device, or one a minute after the
    /// last logged one. The oldest device drops out past [`REFUSALS_KEPT`].
    pub(crate) fn refuse(&self, peer: IpAddr) -> Option<u64> {
        let now = Timestamp::now();
        let peer = peer.to_canonical();
        let mut log = None;
        self.0.send_modify(|status| {
            let mut refusal = match status.refused.iter().position(|r| r.peer == peer) {
                Some(index) => {
                    let mut kept = status.refused.remove(index);
                    kept.at = now;
                    kept.count = kept.count.saturating_add(1);
                    kept
                }
                None => Refusal {
                    peer,
                    at: now,
                    count: 1,
                    // Never logged yet: the check below logs it.
                    logged: now.checked_sub(LOG_AGAIN_AFTER).unwrap_or(Timestamp::MIN),
                },
            };
            if now.duration_since(refusal.logged) >= LOG_AGAIN_AFTER {
                refusal.logged = now;
                log = Some(refusal.count);
            }
            status.refused.insert(0, refusal);
            status.refused.truncate(REFUSALS_KEPT);
        });
        log
    }

    /// Follows one I/O side; the transport pairs every opening with one closing.
    pub(crate) fn observer(&self) -> impl Fn(Activity) + Send + Sync + 'static {
        let reporter = self.clone();
        move |activity| match activity {
            Activity::Opened => reporter.change(|s| s.open = s.open.saturating_add(1)),
            Activity::Closed => reporter.change(|s| s.open = s.open.saturating_sub(1)),
            Activity::Received => reporter.received(),
        }
    }

    fn change(&self, update: impl FnOnce(&mut Status)) {
        self.0.send_modify(|status| {
            let before = status.state();
            update(status);
            if status.state() != before {
                status.changed_at = Timestamp::now();
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(link: Link, ready: bool, open: u32) -> Status {
        Status {
            link,
            ready,
            open,
            changed_at: Timestamp::UNIX_EPOCH,
            last_data: None,
            refused: Vec::new(),
            recording: None,
        }
    }

    // ADR 0026 decision 3: refused devices are kept newest first, one entry per
    // device with its count, at most five; a device retrying is logged once a
    // minute, not on every connection.
    #[test]
    fn refused_devices_are_bounded_and_counted() -> Result<(), std::net::AddrParseError> {
        let reporter = Reporter::new(Link::TcpServer);
        let first: IpAddr = "192.168.1.77".parse()?;
        assert_eq!(
            reporter.refuse(first),
            Some(1),
            "the first refusal is logged"
        );
        assert_eq!(
            reporter.refuse(first),
            None,
            "a retry within a minute is not"
        );
        let mapped: IpAddr = "::ffff:192.168.1.77".parse()?;
        assert_eq!(
            reporter.refuse(mapped),
            None,
            "the mapped form is the same device"
        );
        for last in 1..=6_u8 {
            reporter.refuse(IpAddr::from([10, 0, 0, last]));
        }
        let status = reporter.subscribe().borrow().clone();
        let peers: Vec<String> = status
            .refused()
            .iter()
            .map(|r| r.peer.to_string())
            .collect();
        assert_eq!(
            peers,
            ["10.0.0.6", "10.0.0.5", "10.0.0.4", "10.0.0.3", "10.0.0.2"]
        );
        reporter.refuse(IpAddr::from([10, 0, 0, 3]));
        let status = reporter.subscribe().borrow().clone();
        let newest = status.refused().first().copied();
        assert_eq!(
            newest.map(|r| (r.peer.to_string(), r.count)),
            Some(("10.0.0.3".into(), 2))
        );
        assert_eq!(status.refused().len(), REFUSALS_KEPT);
        // A refusal is not a state change: the light and its time stay.
        assert_eq!(status.state(), "Not listening");
        Ok(())
    }

    // Plan §11 admin page: an open serial port cannot prove a cable or machine.
    #[test]
    fn serial_never_claims_connected() {
        for (ready, open) in [(false, 0), (true, 0), (false, 1), (true, 2)] {
            let state = status(Link::Serial, ready, open).state();
            assert!(!state.contains("onnected"), "{state}");
        }
        assert_eq!(status(Link::Serial, false, 1).state(), "Port open");
        assert_eq!(status(Link::Serial, false, 0).state(), "Port not open");
    }

    // Plan §11 admin page: a TCP server waits until a peer connects.
    #[test]
    fn server_waits_until_a_peer_connects() {
        assert_eq!(status(Link::TcpServer, false, 0).state(), "Not listening");
        assert_eq!(status(Link::TcpServer, true, 0).state(), "Waiting");
        assert_eq!(status(Link::TcpServer, true, 2).state(), "Connected");
        assert_eq!(
            status(Link::TcpClient, false, 0).state(),
            "Not connected (retrying)"
        );
        assert_eq!(status(Link::Folder, true, 0).state(), "Watching");
        assert_eq!(status(Link::Folder, false, 0).state(), "Not watching");
    }

    // Owner request 2026-11-19: green for Connected or Port open, amber for
    // Waiting, grey for Not connected or Port not open.
    #[test]
    fn light_colour_follows_the_state() {
        for (link, ready, open, light) in [
            (Link::TcpServer, true, 1, Light::Live),
            (Link::TcpClient, false, 1, Light::Live),
            (Link::Serial, false, 1, Light::Live),
            (Link::Folder, true, 0, Light::Live),
            (Link::TcpServer, true, 0, Light::Waiting),
            (Link::TcpServer, false, 0, Light::Off),
            (Link::TcpClient, true, 0, Light::Off),
            (Link::Serial, true, 0, Light::Off),
            (Link::Folder, false, 0, Light::Off),
        ] {
            assert_eq!(status(link, ready, open).light(), light, "{link:?}");
        }
    }
}
