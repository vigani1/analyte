//! Defines stored HL7 order and display outcome codes, fixed and with detail.

/// Stable stored codes; spellings are compatibility data, never derived from Debug.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Hl7End {
    Cr,
    DsrDisconnect,
    DsrSessionError,
    Ok,
    ApplicationNegative,
    OrderDisconnect,
    OrderNotReported,
    OrderResponseTimeout,
    OrderSessionError,
    PlacerIdentityTooLong,
}
impl Hl7End {
    /// Returns the exact historical spelling retained in the protected store.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Cr => "HL7-CR",
            Self::DsrDisconnect => "HL7-DSR-disconnect",
            Self::DsrSessionError => "HL7-DSR-session-error",
            Self::Ok => "HL7-OK",
            Self::ApplicationNegative => "HL7-application-negative",
            Self::OrderDisconnect => "HL7-order-disconnect",
            Self::OrderNotReported => "HL7-order-not-reported",
            Self::OrderResponseTimeout => "HL7-order-response-timeout",
            Self::OrderSessionError => "HL7-order-session-error",
            Self::PlacerIdentityTooLong => "HL7-placer-identity-too-long",
        }
    }
}
impl std::fmt::Display for Hl7End {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Stored codes that carry diagnostic text; the caller bounds the text before storing it.
pub(crate) enum Hl7Detail {
    OrderPreflight(String),
    InvalidOrderResponse(String),
    Ua(String),
    Uc(String),
    DisplayMapping(String),
}
impl std::fmt::Display for Hl7Detail {
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
