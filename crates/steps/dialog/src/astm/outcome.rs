//! Defines stored ASTM order transfer outcome codes.

codes! {
    /// Stored ASTM order transfer outcome codes.
    Code {
        Confirmed => "ASTM-transfer-confirmed",
        ReplacementUnverified => "ASTM-transfer-confirmed-replacement-unverified",
        Aborted => "ASTM-transfer-aborted",
        Timeout => "ASTM-transfer-timeout",
        Interrupted => "ASTM-transfer-interrupted",
        Incomplete => "ASTM-transfer-incomplete",
    }
}
