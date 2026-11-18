//! Defines stored ASTM order transfer outcome codes.

codes! {
    /// Stored ASTM order transfer outcome codes.
    Code {
        Confirmed => "ASTM-transfer-confirmed",
        Aborted => "ASTM-transfer-aborted",
        Timeout => "ASTM-transfer-timeout",
        Interrupted => "ASTM-transfer-interrupted",
        Incomplete => "ASTM-transfer-incomplete",
    }
}
