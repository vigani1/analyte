//! Writes stable transport close evidence with one fallback for unnamed external I/O kinds.
use crate::Reason;
use std::{
    borrow::Cow,
    fmt::{self, Write},
    io,
};

#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "external ErrorKind has unstable variants that stable Rust cannot name"
)]
pub(crate) fn io_kind(kind: io::ErrorKind) -> Cow<'static, str> {
    match kind {
        io::ErrorKind::NotFound => Cow::Borrowed("NotFound"),
        io::ErrorKind::PermissionDenied => Cow::Borrowed("PermissionDenied"),
        io::ErrorKind::ConnectionRefused => Cow::Borrowed("ConnectionRefused"),
        io::ErrorKind::ConnectionReset => Cow::Borrowed("ConnectionReset"),
        io::ErrorKind::HostUnreachable => Cow::Borrowed("HostUnreachable"),
        io::ErrorKind::NetworkUnreachable => Cow::Borrowed("NetworkUnreachable"),
        io::ErrorKind::ConnectionAborted => Cow::Borrowed("ConnectionAborted"),
        io::ErrorKind::NotConnected => Cow::Borrowed("NotConnected"),
        io::ErrorKind::AddrInUse => Cow::Borrowed("AddrInUse"),
        io::ErrorKind::AddrNotAvailable => Cow::Borrowed("AddrNotAvailable"),
        io::ErrorKind::NetworkDown => Cow::Borrowed("NetworkDown"),
        io::ErrorKind::BrokenPipe => Cow::Borrowed("BrokenPipe"),
        io::ErrorKind::AlreadyExists => Cow::Borrowed("AlreadyExists"),
        io::ErrorKind::WouldBlock => Cow::Borrowed("WouldBlock"),
        io::ErrorKind::NotADirectory => Cow::Borrowed("NotADirectory"),
        io::ErrorKind::IsADirectory => Cow::Borrowed("IsADirectory"),
        io::ErrorKind::DirectoryNotEmpty => Cow::Borrowed("DirectoryNotEmpty"),
        io::ErrorKind::ReadOnlyFilesystem => Cow::Borrowed("ReadOnlyFilesystem"),
        io::ErrorKind::StaleNetworkFileHandle => Cow::Borrowed("StaleNetworkFileHandle"),
        io::ErrorKind::InvalidInput => Cow::Borrowed("InvalidInput"),
        io::ErrorKind::InvalidData => Cow::Borrowed("InvalidData"),
        io::ErrorKind::TimedOut => Cow::Borrowed("TimedOut"),
        io::ErrorKind::WriteZero => Cow::Borrowed("WriteZero"),
        io::ErrorKind::StorageFull => Cow::Borrowed("StorageFull"),
        io::ErrorKind::NotSeekable => Cow::Borrowed("NotSeekable"),
        io::ErrorKind::QuotaExceeded => Cow::Borrowed("QuotaExceeded"),
        io::ErrorKind::FileTooLarge => Cow::Borrowed("FileTooLarge"),
        io::ErrorKind::ResourceBusy => Cow::Borrowed("ResourceBusy"),
        io::ErrorKind::ExecutableFileBusy => Cow::Borrowed("ExecutableFileBusy"),
        io::ErrorKind::Deadlock => Cow::Borrowed("Deadlock"),
        io::ErrorKind::CrossesDevices => Cow::Borrowed("CrossesDevices"),
        io::ErrorKind::TooManyLinks => Cow::Borrowed("TooManyLinks"),
        io::ErrorKind::InvalidFilename => Cow::Borrowed("InvalidFilename"),
        io::ErrorKind::ArgumentListTooLong => Cow::Borrowed("ArgumentListTooLong"),
        io::ErrorKind::Interrupted => Cow::Borrowed("Interrupted"),
        io::ErrorKind::Unsupported => Cow::Borrowed("Unsupported"),
        io::ErrorKind::UnexpectedEof => Cow::Borrowed("UnexpectedEof"),
        io::ErrorKind::OutOfMemory => Cow::Borrowed("OutOfMemory"),
        io::ErrorKind::Other => Cow::Borrowed("Other"),
        // ErrorKind is external and non-exhaustive. Stable Rust cannot name its
        // unstable variants; preserve their historical spelling rather than lose evidence.
        other => Cow::Owned(format!("{other:?}")),
    }
}

impl fmt::Display for Reason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disconnect => f.write_str("Disconnect"),
            Self::Cancelled => f.write_str("Cancelled"),
            Self::LimitReached => f.write_str("LimitReached"),
            Self::Io {
                endpoint,
                operation,
                kind,
            } => {
                f.write_str("Io { endpoint: ")?;
                quoted(f, endpoint)?;
                f.write_str(", operation: ")?;
                quoted(f, operation)?;
                write!(f, ", kind: {} }}", io_kind(*kind))
            }
            Self::PortVanished(port) => {
                f.write_str("PortVanished(")?;
                quoted(f, port)?;
                f.write_char(')')
            }
            Self::Deadline(operation) => {
                f.write_str("Deadline(")?;
                quoted(f, operation)?;
                f.write_char(')')
            }
        }
    }
}

fn quoted(f: &mut fmt::Formatter<'_>, value: &str) -> fmt::Result {
    f.write_char('"')?;
    for character in value.chars() {
        match character {
            '\'' => f.write_char(character)?,
            other => {
                for escaped in other.escape_debug() {
                    f.write_char(escaped)?;
                }
            }
        }
    }
    f.write_char('"')
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn io_kind_evidence_spellings_are_compatible() {
        for (kind, expected) in [
            (io::ErrorKind::NotFound, "NotFound"),
            (io::ErrorKind::PermissionDenied, "PermissionDenied"),
            (io::ErrorKind::ConnectionRefused, "ConnectionRefused"),
            (io::ErrorKind::ConnectionReset, "ConnectionReset"),
            (io::ErrorKind::HostUnreachable, "HostUnreachable"),
            (io::ErrorKind::NetworkUnreachable, "NetworkUnreachable"),
            (io::ErrorKind::ConnectionAborted, "ConnectionAborted"),
            (io::ErrorKind::NotConnected, "NotConnected"),
            (io::ErrorKind::AddrInUse, "AddrInUse"),
            (io::ErrorKind::AddrNotAvailable, "AddrNotAvailable"),
            (io::ErrorKind::NetworkDown, "NetworkDown"),
            (io::ErrorKind::BrokenPipe, "BrokenPipe"),
            (io::ErrorKind::AlreadyExists, "AlreadyExists"),
            (io::ErrorKind::WouldBlock, "WouldBlock"),
            (io::ErrorKind::NotADirectory, "NotADirectory"),
            (io::ErrorKind::IsADirectory, "IsADirectory"),
            (io::ErrorKind::DirectoryNotEmpty, "DirectoryNotEmpty"),
            (io::ErrorKind::ReadOnlyFilesystem, "ReadOnlyFilesystem"),
            (
                io::ErrorKind::StaleNetworkFileHandle,
                "StaleNetworkFileHandle",
            ),
            (io::ErrorKind::InvalidInput, "InvalidInput"),
            (io::ErrorKind::InvalidData, "InvalidData"),
            (io::ErrorKind::TimedOut, "TimedOut"),
            (io::ErrorKind::WriteZero, "WriteZero"),
            (io::ErrorKind::StorageFull, "StorageFull"),
            (io::ErrorKind::NotSeekable, "NotSeekable"),
            (io::ErrorKind::QuotaExceeded, "QuotaExceeded"),
            (io::ErrorKind::FileTooLarge, "FileTooLarge"),
            (io::ErrorKind::ResourceBusy, "ResourceBusy"),
            (io::ErrorKind::ExecutableFileBusy, "ExecutableFileBusy"),
            (io::ErrorKind::Deadlock, "Deadlock"),
            (io::ErrorKind::CrossesDevices, "CrossesDevices"),
            (io::ErrorKind::TooManyLinks, "TooManyLinks"),
            (io::ErrorKind::InvalidFilename, "InvalidFilename"),
            (io::ErrorKind::ArgumentListTooLong, "ArgumentListTooLong"),
            (io::ErrorKind::Interrupted, "Interrupted"),
            (io::ErrorKind::Unsupported, "Unsupported"),
            (io::ErrorKind::UnexpectedEof, "UnexpectedEof"),
            (io::ErrorKind::OutOfMemory, "OutOfMemory"),
            (io::ErrorKind::Other, "Other"),
        ] {
            assert_eq!(io_kind(kind), expected);
        }
    }
    #[test]
    fn transport_reason_evidence_spellings_are_compatible() {
        for (reason, expected) in [
            (Reason::Disconnect, "Disconnect"),
            (Reason::Cancelled, "Cancelled"),
            (Reason::LimitReached, "LimitReached"),
            (
                Reason::PortVanished("COM1".into()),
                "PortVanished(\"COM1\")",
            ),
            (Reason::Deadline("write all"), "Deadline(\"write all\")"),
            (
                Reason::Io {
                    endpoint: "COM1".into(),
                    operation: "read",
                    kind: io::ErrorKind::TimedOut,
                },
                "Io { endpoint: \"COM1\", operation: \"read\", kind: TimedOut }",
            ),
            (
                Reason::PortVanished("a'\n\t\r\0\\\"?".into()),
                "PortVanished(\"a'\\n\\t\\r\\0\\\\\\\"?\")",
            ),
        ] {
            assert_eq!(reason.to_string(), expected);
        }
    }
}
