//! Defines stable stored order and transfer outcome codes independently of variant names.
/// Compatibility spellings retained in stored evidence and operator records.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Code {
    AstmTransferConfirmed,
    AstmTransferAborted,
    AstmTransferTimeout,
    AstmTransferInterrupted,
    AstmTransferIncomplete,

    AutolyserParkedAck,
    AutolyserParkedBusy,
    AutolyserParkedConfig,
    AutolyserParkedIncomplete,
    AutolyserParkedStale,
    AutolyserParkedStorage,
    AutolyserParkedTime,
    Hl7Cr,
    Hl7DsrDisconnect,
    Hl7DsrSessionError,
    Hl7Ok,
    Hl7ApplicationNegative,
    Hl7OrderDisconnect,
    Hl7OrderNotReported,
    Hl7OrderResponseTimeout,
    Hl7OrderSessionError,
    Hl7PlacerIdentityTooLong,
    LawCr,
    LawOk,
    LawOrlItemMissing,
    LawOrlLate,
    LawOrlTimeout,
    LawUa,
    LawUc,
    LawDisconnect,
    LawIdleAdmissionFailed,
    LawIndependentAdmissionFailed,
    LawIndependentExchangeEnded,
    LawIndependentQueryResponseIssuanceUncertain,
    LawIndependentQueueDeadline,
    LawIndependentShutdown,
    LawIndependentShutdownBeforeResponse,
    LawNegativeQueryOrl,
    LawQueryPreparationFailed,
    LawSendPreparationFailed,
    LawSessionError,
}
impl Code {
    /// Returns the explicit historical spelling; never derive evidence from Debug.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::AstmTransferConfirmed => "ASTM-transfer-confirmed",
            Self::AstmTransferAborted => "ASTM-transfer-aborted",
            Self::AstmTransferTimeout => "ASTM-transfer-timeout",
            Self::AstmTransferInterrupted => "ASTM-transfer-interrupted",
            Self::AstmTransferIncomplete => "ASTM-transfer-incomplete",

            Self::AutolyserParkedAck => "Autolyser-parked-ack",
            Self::AutolyserParkedBusy => "Autolyser-parked-busy",
            Self::AutolyserParkedConfig => "Autolyser-parked-config",
            Self::AutolyserParkedIncomplete => "Autolyser-parked-incomplete",
            Self::AutolyserParkedStale => "Autolyser-parked-stale",
            Self::AutolyserParkedStorage => "Autolyser-parked-storage",
            Self::AutolyserParkedTime => "Autolyser-parked-time",
            Self::Hl7Cr => "HL7-CR",
            Self::Hl7DsrDisconnect => "HL7-DSR-disconnect",
            Self::Hl7DsrSessionError => "HL7-DSR-session-error",
            Self::Hl7Ok => "HL7-OK",
            Self::Hl7ApplicationNegative => "HL7-application-negative",
            Self::Hl7OrderDisconnect => "HL7-order-disconnect",
            Self::Hl7OrderNotReported => "HL7-order-not-reported",
            Self::Hl7OrderResponseTimeout => "HL7-order-response-timeout",
            Self::Hl7OrderSessionError => "HL7-order-session-error",
            Self::Hl7PlacerIdentityTooLong => "HL7-placer-identity-too-long",
            Self::LawCr => "LAW-CR",
            Self::LawOk => "LAW-OK",
            Self::LawOrlItemMissing => "LAW-ORL-item-missing",
            Self::LawOrlLate => "LAW-ORL-late",
            Self::LawOrlTimeout => "LAW-ORL-timeout",
            Self::LawUa => "LAW-UA",
            Self::LawUc => "LAW-UC",
            Self::LawDisconnect => "LAW-disconnect",
            Self::LawIdleAdmissionFailed => "LAW-idle-admission-failed",
            Self::LawIndependentAdmissionFailed => "LAW-independent-admission-failed",
            Self::LawIndependentExchangeEnded => "LAW-independent-exchange-ended",
            Self::LawIndependentQueryResponseIssuanceUncertain => {
                "LAW-independent-query-response-issuance-uncertain"
            }
            Self::LawIndependentQueueDeadline => "LAW-independent-queue-deadline",
            Self::LawIndependentShutdown => "LAW-independent-shutdown",
            Self::LawIndependentShutdownBeforeResponse => {
                "LAW-independent-shutdown-before-response"
            }
            Self::LawNegativeQueryOrl => "LAW-negative-query-ORL",
            Self::LawQueryPreparationFailed => "LAW-query-preparation-failed",
            Self::LawSendPreparationFailed => "LAW-send-preparation-failed",
            Self::LawSessionError => "LAW-session-error",
        }
    }
}

#[test]
fn outcome_spellings_are_compatible() {
    for (code, expected) in [
        (Code::AstmTransferConfirmed, "ASTM-transfer-confirmed"),
        (Code::AstmTransferAborted, "ASTM-transfer-aborted"),
        (Code::AstmTransferTimeout, "ASTM-transfer-timeout"),
        (Code::AstmTransferInterrupted, "ASTM-transfer-interrupted"),
        (Code::AstmTransferIncomplete, "ASTM-transfer-incomplete"),
        (Code::AutolyserParkedAck, "Autolyser-parked-ack"),
        (Code::AutolyserParkedBusy, "Autolyser-parked-busy"),
        (Code::AutolyserParkedConfig, "Autolyser-parked-config"),
        (
            Code::AutolyserParkedIncomplete,
            "Autolyser-parked-incomplete",
        ),
        (Code::AutolyserParkedStale, "Autolyser-parked-stale"),
        (Code::AutolyserParkedStorage, "Autolyser-parked-storage"),
        (Code::AutolyserParkedTime, "Autolyser-parked-time"),
        (Code::Hl7Cr, "HL7-CR"),
        (Code::Hl7DsrDisconnect, "HL7-DSR-disconnect"),
        (Code::Hl7DsrSessionError, "HL7-DSR-session-error"),
        (Code::Hl7Ok, "HL7-OK"),
        (Code::Hl7ApplicationNegative, "HL7-application-negative"),
        (Code::Hl7OrderDisconnect, "HL7-order-disconnect"),
        (Code::Hl7OrderNotReported, "HL7-order-not-reported"),
        (Code::Hl7OrderResponseTimeout, "HL7-order-response-timeout"),
        (Code::Hl7OrderSessionError, "HL7-order-session-error"),
        (
            Code::Hl7PlacerIdentityTooLong,
            "HL7-placer-identity-too-long",
        ),
        (Code::LawCr, "LAW-CR"),
        (Code::LawOk, "LAW-OK"),
        (Code::LawOrlItemMissing, "LAW-ORL-item-missing"),
        (Code::LawOrlLate, "LAW-ORL-late"),
        (Code::LawOrlTimeout, "LAW-ORL-timeout"),
        (Code::LawUa, "LAW-UA"),
        (Code::LawUc, "LAW-UC"),
        (Code::LawDisconnect, "LAW-disconnect"),
        (Code::LawIdleAdmissionFailed, "LAW-idle-admission-failed"),
        (
            Code::LawIndependentAdmissionFailed,
            "LAW-independent-admission-failed",
        ),
        (
            Code::LawIndependentExchangeEnded,
            "LAW-independent-exchange-ended",
        ),
        (
            Code::LawIndependentQueryResponseIssuanceUncertain,
            "LAW-independent-query-response-issuance-uncertain",
        ),
        (
            Code::LawIndependentQueueDeadline,
            "LAW-independent-queue-deadline",
        ),
        (Code::LawIndependentShutdown, "LAW-independent-shutdown"),
        (
            Code::LawIndependentShutdownBeforeResponse,
            "LAW-independent-shutdown-before-response",
        ),
        (Code::LawNegativeQueryOrl, "LAW-negative-query-ORL"),
        (
            Code::LawQueryPreparationFailed,
            "LAW-query-preparation-failed",
        ),
        (
            Code::LawSendPreparationFailed,
            "LAW-send-preparation-failed",
        ),
        (Code::LawSessionError, "LAW-session-error"),
    ] {
        assert_eq!(code.as_str(), expected);
    }
}
