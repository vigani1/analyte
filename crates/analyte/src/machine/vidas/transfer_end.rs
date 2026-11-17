//! Defines stable vendor transfer evidence codes independently of Rust variant names.

/// Stable stored codes; spellings are compatibility data, never derived from Debug.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VidasEnd {
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
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::AcknowledgmentTimeout => {
                crate::orders::outcome::Code::VidasAcknowledgmentTimeout.as_str()
            }
            Self::CommitTimeout => crate::orders::outcome::Code::VidasCommitTimeout.as_str(),
            Self::ControlSuffixMismatch => {
                crate::orders::outcome::Code::VidasControlSuffixMismatch.as_str()
            }
            Self::FramingError => crate::orders::outcome::Code::VidasFramingError.as_str(),
            Self::InputFailed => crate::orders::outcome::Code::VidasInputFailed.as_str(),
            Self::InterbyteTimeout => crate::orders::outcome::Code::VidasInterbyteTimeout.as_str(),
            Self::SessionTimeout => crate::orders::outcome::Code::VidasSessionTimeout.as_str(),
            Self::TransferCancelled => {
                crate::orders::outcome::Code::VidasTransferCancelled.as_str()
            }
            Self::TransferConfirmed => {
                crate::orders::outcome::Code::VidasTransferConfirmed.as_str()
            }
            Self::TransferDisconnected => {
                crate::orders::outcome::Code::VidasTransferDisconnected.as_str()
            }
            Self::TransferError => crate::orders::outcome::Code::VidasTransferError.as_str(),
            Self::TransferRefusedOrUnexpected => {
                crate::orders::outcome::Code::VidasTransferRefusedOrUnexpected.as_str()
            }
            Self::WriteFailed => crate::orders::outcome::Code::VidasWriteFailed.as_str(),
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
