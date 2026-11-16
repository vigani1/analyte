//! Defines stable vendor transfer evidence codes independently of Rust variant names.

/// Stable stored codes; spellings are compatibility data, never derived from Debug.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum VidasEnd {
    AcknowledgmentTimeout,
    CommitTimeout,
    ControlSuffixMismatch,
    FramingError,
    InputFailed,
    InterbyteTimeout,
    SessionTimeout,
    TransferCancelled,
    TransferConfirmed,
    TransferDisconnected,
    TransferError,
    TransferRefusedOrUnexpected,
    WriteFailed,
}
impl VidasEnd {
    /// Returns the exact historical spelling retained in the protected store.
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::AcknowledgmentTimeout => "VIDAS-acknowledgment-timeout",
            Self::CommitTimeout => "VIDAS-commit-timeout",
            Self::ControlSuffixMismatch => "VIDAS-control-suffix-mismatch",
            Self::FramingError => "VIDAS-framing-error",
            Self::InputFailed => "VIDAS-input-failed",
            Self::InterbyteTimeout => "VIDAS-interbyte-timeout",
            Self::SessionTimeout => "VIDAS-session-timeout",
            Self::TransferCancelled => "VIDAS-transfer-cancelled",
            Self::TransferConfirmed => "VIDAS-transfer-confirmed",
            Self::TransferDisconnected => "VIDAS-transfer-disconnected",
            Self::TransferError => "VIDAS-transfer-error",
            Self::TransferRefusedOrUnexpected => "VIDAS-transfer-refused-or-unexpected",
            Self::WriteFailed => "VIDAS-write-failed",
        }
    }
}
impl std::fmt::Display for VidasEnd {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
#[test]
fn vidas_stored_spellings_are_compatible() {
    for (code, expected) in [
        (
            VidasEnd::AcknowledgmentTimeout,
            "VIDAS-acknowledgment-timeout",
        ),
        (VidasEnd::CommitTimeout, "VIDAS-commit-timeout"),
        (
            VidasEnd::ControlSuffixMismatch,
            "VIDAS-control-suffix-mismatch",
        ),
        (VidasEnd::FramingError, "VIDAS-framing-error"),
        (VidasEnd::InputFailed, "VIDAS-input-failed"),
        (VidasEnd::InterbyteTimeout, "VIDAS-interbyte-timeout"),
        (VidasEnd::SessionTimeout, "VIDAS-session-timeout"),
        (VidasEnd::TransferCancelled, "VIDAS-transfer-cancelled"),
        (VidasEnd::TransferConfirmed, "VIDAS-transfer-confirmed"),
        (
            VidasEnd::TransferDisconnected,
            "VIDAS-transfer-disconnected",
        ),
        (VidasEnd::TransferError, "VIDAS-transfer-error"),
        (
            VidasEnd::TransferRefusedOrUnexpected,
            "VIDAS-transfer-refused-or-unexpected",
        ),
        (VidasEnd::WriteFailed, "VIDAS-write-failed"),
    ] {
        assert_eq!(code.as_str(), expected);
        assert_eq!(code.to_string(), expected);
    }
}

/// Stable stored codes; spellings are compatibility data, never derived from Debug.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum AutolyserEnd {
    InputFailed,
    StorageFailed,
    StorageTimeout,
    TransferCancelled,
    TransferDisconnected,
    TransferError,
    TransferTimeout,
    WriteFailed,
}
impl AutolyserEnd {
    /// Returns the exact historical spelling retained in the protected store.
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::InputFailed => "Autolyser-input-failed",
            Self::StorageFailed => "Autolyser-storage-failed",
            Self::StorageTimeout => "Autolyser-storage-timeout",
            Self::TransferCancelled => "Autolyser-transfer-cancelled",
            Self::TransferDisconnected => "Autolyser-transfer-disconnected",
            Self::TransferError => "Autolyser-transfer-error",
            Self::TransferTimeout => "Autolyser-transfer-timeout",
            Self::WriteFailed => "Autolyser-write-failed",
        }
    }
}
impl std::fmt::Display for AutolyserEnd {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
#[test]
fn autolyser_stored_spellings_are_compatible() {
    for (code, expected) in [
        (AutolyserEnd::InputFailed, "Autolyser-input-failed"),
        (AutolyserEnd::StorageFailed, "Autolyser-storage-failed"),
        (AutolyserEnd::StorageTimeout, "Autolyser-storage-timeout"),
        (
            AutolyserEnd::TransferCancelled,
            "Autolyser-transfer-cancelled",
        ),
        (
            AutolyserEnd::TransferDisconnected,
            "Autolyser-transfer-disconnected",
        ),
        (AutolyserEnd::TransferError, "Autolyser-transfer-error"),
        (AutolyserEnd::TransferTimeout, "Autolyser-transfer-timeout"),
        (AutolyserEnd::WriteFailed, "Autolyser-write-failed"),
    ] {
        assert_eq!(code.as_str(), expected);
        assert_eq!(code.to_string(), expected);
    }
}
