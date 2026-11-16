//! Shared transport settings and conversions at the config boundary.
use super::*;

/// TCP connection direction; cannot select serial or folder transport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TcpRole {
    /// Connect to the configured endpoint.
    Client,
    /// Listen on the configured endpoint.
    Server,
}

/// Transport for a stack supporting either serial or TCP, never a watched folder.
#[derive(Debug, Clone)]
pub struct StreamTransport {
    pub(super) kind: StreamKind,
    pub(super) operation_timeout_ms: u32,
}

/// Read-only physical stream selection; the owning transport validates its settings.
#[derive(Debug, Clone)]
pub enum StreamKind {
    /// Installed serial settings.
    Serial(Serial),
    /// TCP connection direction.
    Tcp(TcpRole),
}

impl StreamTransport {
    /// Borrows the installed physical stream selection.
    #[must_use]
    pub fn kind(&self) -> &StreamKind {
        &self.kind
    }
    /// Returns the validated, nonzero external-operation deadline.
    #[must_use]
    pub fn operation_timeout_ms(&self) -> u32 {
        self.operation_timeout_ms
    }
}

/// Validated TCP settings without serial or folder alternatives.
#[derive(Debug, Clone)]
pub struct TcpTransport {
    /// Connection direction.
    pub(super) role: TcpRole,
    /// Deadline for external operations.
    pub(super) operation_timeout_ms: u32,
}

pub(super) fn tcp(transport: &Transport) -> TcpTransport {
    let role = match transport.kind {
        TransportKind::TcpClient => TcpRole::Client,
        TransportKind::TcpServer => TcpRole::Server,
        TransportKind::Serial | TransportKind::WatchedFolder => {
            unreachable!("validated TCP stack requires TCP transport")
        }
    };
    TcpTransport {
        role,
        operation_timeout_ms: transport.operation_timeout_ms,
    }
}

#[expect(
    clippy::expect_used,
    reason = "private conversion runs only after validate_machine proves required sections and options"
)]
pub(super) fn stream(transport: &Transport) -> StreamTransport {
    let kind = match transport.kind {
        TransportKind::Serial => StreamKind::Serial(
            transport
                .serial
                .clone()
                .expect("validated serial transport requires settings"),
        ),
        TransportKind::TcpClient | TransportKind::TcpServer => StreamKind::Tcp(tcp(transport).role),
        TransportKind::WatchedFolder => {
            unreachable!("validated stream stack excludes watched folders")
        }
    };
    StreamTransport {
        kind,
        operation_timeout_ms: transport.operation_timeout_ms,
    }
}

pub(super) fn tcp_kind(role: TcpRole) -> TransportKind {
    match role {
        TcpRole::Client => TransportKind::TcpClient,
        TcpRole::Server => TransportKind::TcpServer,
    }
}

pub(super) fn stream_kind(transport: &StreamTransport) -> TransportKind {
    match transport.kind() {
        StreamKind::Serial(_) => TransportKind::Serial,
        StreamKind::Tcp(role) => tcp_kind(*role),
    }
}

pub(super) fn stream_timeout(transport: &StreamTransport) -> u32 {
    transport.operation_timeout_ms()
}

impl TcpTransport {
    /// Returns the validated role setting.
    #[must_use]
    pub fn role(&self) -> TcpRole {
        self.role
    }
    /// Returns the validated operation timeout ms setting.
    #[must_use]
    pub fn operation_timeout_ms(&self) -> u32 {
        self.operation_timeout_ms
    }
}

/// Transport-specific settings for one record protocol. Folder pickup has no wire dialog.
#[derive(Debug, Clone)]
pub enum RecordTransport<S, F> {
    /// Serial or TCP framing and conversation settings.
    Stream(S),
    /// Complete-file handoff settings, without wire acknowledgments.
    Folder(F),
}
