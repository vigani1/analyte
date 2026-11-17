//! Names transport deadline operations with stable display spellings.
/// Operation whose deadline elapsed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeadlineKind {
    /// Deadline for connect.
    Connect,
    /// Deadline for bind.
    Bind,
    /// Deadline for TCP read.
    TcpRead,
    /// Deadline for TCP write (outcome uncertain).
    TcpWrite,
    /// Deadline for server notification.
    ServerNotification,
    /// Deadline for server shutdown.
    ServerShutdown,
    /// Deadline for serial event backpressure.
    SerialBackpressure,
    /// Deadline for serial write (outcome uncertain).
    SerialWrite,
    /// Deadline for write all.
    WriteAll,
    /// Deadline for receive.
    Receive,
    /// Deadline for queue write.
    QueueWrite,
    /// Deadline for event backpressure.
    EventBackpressure,
    /// Deadline for confirming queued writes were sent (outcome uncertain).
    Flush,
}
impl DeadlineKind {
    /// Stable operation text for display and retained evidence.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Connect => "connect",
            Self::Bind => "bind",
            Self::TcpRead => "TCP read",
            Self::TcpWrite => "TCP write (outcome uncertain)",
            Self::ServerNotification => "server notification",
            Self::ServerShutdown => "server shutdown",
            Self::SerialBackpressure => "serial event backpressure",
            Self::SerialWrite => "serial write (outcome uncertain)",
            Self::WriteAll => "write all",
            Self::Receive => "receive",
            Self::QueueWrite => "queue write",
            Self::EventBackpressure => "event backpressure",
            Self::Flush => "flush (outcome uncertain)",
        }
    }
}
impl std::fmt::Display for DeadlineKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
