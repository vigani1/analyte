//! Owns each machine's in-memory link light: link kind, state, last data and last change.
//!
//! The machine runtime is the only writer; the admin page only subscribes. The status
//! never holds bytes, sample IDs or peer addresses (safety rule 10).
use analyte_config::{Stack, TcpRole};
use analyte_transport::Activity;
use jiff::Timestamp;
use tokio::sync::watch;

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
}

impl Status {
    pub(crate) const fn link(&self) -> Link {
        self.link
    }

    /// The operator-facing state. A serial port can be open with no cable or machine
    /// attached, so serial says only whether the port is open, never "Connected".
    pub(crate) const fn state(&self) -> &'static str {
        let open = self.open > 0;
        match self.link {
            Link::TcpServer | Link::TcpClient if open => "Connected",
            Link::TcpServer if self.ready => "Waiting",
            Link::TcpServer => "Not listening",
            Link::TcpClient => "Not connected (retrying)",
            Link::Serial if open => "Port open",
            Link::Serial => "Port not open",
            Link::Folder if self.ready => "Watching",
            Link::Folder => "Not watching",
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
}

/// The runtime's writing end. Cloning shares the same status.
#[derive(Clone, Debug)]
pub(crate) struct Reporter(watch::Sender<Status>);

impl Reporter {
    pub(crate) fn new(link: Link) -> Self {
        Self(watch::Sender::new(Status {
            link,
            ready: false,
            open: 0,
            changed_at: Timestamp::now(),
            last_data: None,
        }))
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
        }
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
}
