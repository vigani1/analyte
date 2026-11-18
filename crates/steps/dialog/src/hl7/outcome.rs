//! Defines stored HL7 order and display outcome codes, fixed and with detail.

codes! {
    /// Stored HL7 order and display outcome codes.
    Code {
        Cr => "HL7-CR",
        DsrDisconnect => "HL7-DSR-disconnect",
        DsrSessionError => "HL7-DSR-session-error",
        Ok => "HL7-OK",
        ApplicationNegative => "HL7-application-negative",
        OrderDisconnect => "HL7-order-disconnect",
        OrderNotReported => "HL7-order-not-reported",
        OrderResponseTimeout => "HL7-order-response-timeout",
        OrderSessionError => "HL7-order-session-error",
        PlacerIdentityTooLong => "HL7-placer-identity-too-long",
    }
}

/// Stored codes that carry diagnostic text; the caller bounds the text before storing it.
pub enum Detail {
    /// Stored as `HL7-order-preflight:<text>`.
    OrderPreflight(String),
    /// Stored as `HL7-invalid-order-response:<text>`.
    InvalidOrderResponse(String),
    /// Stored as `HL7-UA:<text>`.
    Ua(String),
    /// Stored as `HL7-UC:<text>`.
    Uc(String),
    /// Stored as `HL7-display-mapping: <text>`.
    DisplayMapping(String),
}
impl std::fmt::Display for Detail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OrderPreflight(detail) => write!(f, "HL7-order-preflight:{detail}"),
            Self::InvalidOrderResponse(detail) => write!(f, "HL7-invalid-order-response:{detail}"),
            Self::Ua(detail) => write!(f, "HL7-UA:{detail}"),
            Self::Uc(detail) => write!(f, "HL7-UC:{detail}"),
            Self::DisplayMapping(detail) => write!(f, "HL7-display-mapping: {detail}"),
        }
    }
}
