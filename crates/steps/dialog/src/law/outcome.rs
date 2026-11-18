//! Defines stored IHE LAW order outcome codes, fixed and with detail, and acceptance spellings.
use analyte_hl7::AckCode;

codes! {
    /// Stored IHE LAW order outcome codes.
    Code {
        Cr => "LAW-CR",
        Ok => "LAW-OK",
        OrlItemMissing => "LAW-ORL-item-missing",
        OrlLate => "LAW-ORL-late",
        OrlTimeout => "LAW-ORL-timeout",
        Ua => "LAW-UA",
        Uc => "LAW-UC",
        Disconnect => "LAW-disconnect",
        IdleAdmissionFailed => "LAW-idle-admission-failed",
        IndependentAdmissionFailed => "LAW-independent-admission-failed",
        IndependentExchangeEnded => "LAW-independent-exchange-ended",
        QueryIssuanceUncertain => "LAW-independent-query-response-issuance-uncertain",
        IndependentQueueDeadline => "LAW-independent-queue-deadline",
        IndependentShutdown => "LAW-independent-shutdown",
        ResponseShutdown => "LAW-independent-shutdown-before-response",
        NegativeQueryOrl => "LAW-negative-query-ORL",
        QueryPreparationFailed => "LAW-query-preparation-failed",
        SendPreparationFailed => "LAW-send-preparation-failed",
        SessionError => "LAW-session-error",
    }
}

/// Stored codes that carry diagnostic text; the caller bounds the text before storing it.
pub enum Detail {
    /// Stored as `LAW-invalid-ORL:<text>`.
    InvalidOrl(String),
    /// Stored as `LAW-order-preflight:<text>`.
    OrderPreflight(String),
    /// Stored as `LAW-independent-failure:<text>`.
    IndependentFailure(String),
    /// Stored as `LAW-ORL-application-<acceptance>`.
    Application(AckCode),
}
impl std::fmt::Display for Detail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidOrl(detail) => write!(f, "LAW-invalid-ORL:{detail}"),
            Self::OrderPreflight(detail) => write!(f, "LAW-order-preflight:{detail}"),
            Self::IndependentFailure(detail) => write!(f, "LAW-independent-failure:{detail}"),
            Self::Application(code) => write!(f, "LAW-ORL-application-{}", acceptance(*code)),
        }
    }
}

/// Writes the stored acceptance spelling; these are not HL7 wire codes.
#[must_use]
pub const fn acceptance(code: AckCode) -> &'static str {
    match code {
        AckCode::Aa => "Aa",
        AckCode::Ae => "Ae",
        AckCode::Ar => "Ar",
        AckCode::Ca => "Ca",
        AckCode::Ce => "Ce",
        AckCode::Cr => "Cr",
    }
}
