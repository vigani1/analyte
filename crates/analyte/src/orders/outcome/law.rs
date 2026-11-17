//! Defines stored IHE LAW order outcome codes, fixed and with detail.

/// Stable stored codes; spellings are compatibility data, never derived from Debug.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LawEnd {
    Cr,
    Ok,
    OrlItemMissing,
    OrlLate,
    OrlTimeout,
    Ua,
    Uc,
    Disconnect,
    IdleAdmissionFailed,
    IndependentAdmissionFailed,
    IndependentExchangeEnded,
    QueryIssuanceUncertain,
    IndependentQueueDeadline,
    IndependentShutdown,
    ResponseShutdown,
    NegativeQueryOrl,
    QueryPreparationFailed,
    SendPreparationFailed,
    SessionError,
}
impl LawEnd {
    /// Returns the exact historical spelling retained in the protected store.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Cr => "LAW-CR",
            Self::Ok => "LAW-OK",
            Self::OrlItemMissing => "LAW-ORL-item-missing",
            Self::OrlLate => "LAW-ORL-late",
            Self::OrlTimeout => "LAW-ORL-timeout",
            Self::Ua => "LAW-UA",
            Self::Uc => "LAW-UC",
            Self::Disconnect => "LAW-disconnect",
            Self::IdleAdmissionFailed => "LAW-idle-admission-failed",
            Self::IndependentAdmissionFailed => "LAW-independent-admission-failed",
            Self::IndependentExchangeEnded => "LAW-independent-exchange-ended",
            Self::QueryIssuanceUncertain => "LAW-independent-query-response-issuance-uncertain",
            Self::IndependentQueueDeadline => "LAW-independent-queue-deadline",
            Self::IndependentShutdown => "LAW-independent-shutdown",
            Self::ResponseShutdown => "LAW-independent-shutdown-before-response",
            Self::NegativeQueryOrl => "LAW-negative-query-ORL",
            Self::QueryPreparationFailed => "LAW-query-preparation-failed",
            Self::SendPreparationFailed => "LAW-send-preparation-failed",
            Self::SessionError => "LAW-session-error",
        }
    }
}
impl std::fmt::Display for LawEnd {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Stored codes that carry diagnostic text; the caller bounds the text before storing it.
pub(crate) enum LawDetail {
    InvalidOrl(String),
    OrderPreflight(String),
    IndependentFailure(String),
    Application(analyte_hl7::AckCode),
}
impl std::fmt::Display for LawDetail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidOrl(detail) => write!(f, "LAW-invalid-ORL:{detail}"),
            Self::OrderPreflight(detail) => write!(f, "LAW-order-preflight:{detail}"),
            Self::IndependentFailure(detail) => write!(f, "LAW-independent-failure:{detail}"),
            Self::Application(code) => write!(
                f,
                "LAW-ORL-application-{}",
                crate::orders::acceptance::acceptance(*code)
            ),
        }
    }
}
