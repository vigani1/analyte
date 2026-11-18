//! Defines stored VIDAS order transfer outcome codes.

codes! {
    /// Stored VIDAS order transfer outcome codes.
    Code {
        AcknowledgmentTimeout => "VIDAS-acknowledgment-timeout",
        CommitTimeout => "VIDAS-commit-timeout",
        ControlSuffixMismatch => "VIDAS-control-suffix-mismatch",
        FramingError => "VIDAS-framing-error",
        InputFailed => "VIDAS-input-failed",
        InterbyteTimeout => "VIDAS-interbyte-timeout",
        SessionTimeout => "VIDAS-session-timeout",
        TransferCancelled => "VIDAS-transfer-cancelled",
        TransferConfirmed => "VIDAS-transfer-confirmed",
        TransferDisconnected => "VIDAS-transfer-disconnected",
        TransferError => "VIDAS-transfer-error",
        TransferRefusedOrUnexpected => "VIDAS-transfer-refused-or-unexpected",
        WriteFailed => "VIDAS-write-failed",
    }
}
