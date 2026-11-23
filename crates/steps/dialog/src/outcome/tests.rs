//! Pins every stored order outcome spelling; stored records keep their text.
use super::Host as HostEnd;
use crate::astm::outcome::Code as AstmEnd;
use crate::autolyser::outcome::{
    Code as AutolyserEnd, Detail as AutolyserDetail, Interruption, insertion, insertion_rejection,
    parked,
};
use crate::hl7::outcome::{Code as Hl7End, Detail as Hl7Detail};
use crate::law::outcome::{Code as LawEnd, Detail as LawDetail, acceptance};
use crate::vidas::outcome::Code as VidasEnd;
use analyte_hl7::AckCode;

/// One fixed stored code of any protocol, so one table pins them all.
#[derive(Clone, Copy)]
enum Code {
    Host(HostEnd),
    Astm(AstmEnd),
    Autolyser(AutolyserEnd),
    Hl7(Hl7End),
    Law(LawEnd),
    Vidas(VidasEnd),
}

/// Exhaustive on purpose: a new fixed code does not compile until its stored
/// spelling is pinned here, independently of the production table.
const fn pinned(code: Code) -> &'static str {
    match code {
        Code::Host(code) => match code {
            HostEnd::QueryReservationRefused => "query-reservation-refused",
            HostEnd::QueryDeadlineBeforeSend => "query-deadline-before-send",
            HostEnd::HostRestart => "host-restart",
            HostEnd::ConnectionEndedBeforeConfirmation => {
                "connection-ended-before-transfer-confirmation"
            }
        },
        Code::Astm(code) => match code {
            AstmEnd::Confirmed => "ASTM-transfer-confirmed",
            AstmEnd::ReplacementUnverified => "ASTM-transfer-confirmed-replacement-unverified",
            AstmEnd::Aborted => "ASTM-transfer-aborted",
            AstmEnd::Timeout => "ASTM-transfer-timeout",
            AstmEnd::Interrupted => "ASTM-transfer-interrupted",
            AstmEnd::Incomplete => "ASTM-transfer-incomplete",
        },
        Code::Autolyser(code) => match code {
            AutolyserEnd::InputFailed => "Autolyser-input-failed",
            AutolyserEnd::ParkedAck => "Autolyser-parked-ack",
            AutolyserEnd::ParkedBusy => "Autolyser-parked-busy",
            AutolyserEnd::ParkedConfig => "Autolyser-parked-config",
            AutolyserEnd::ParkedIncomplete => "Autolyser-parked-incomplete",
            AutolyserEnd::ParkedStale => "Autolyser-parked-stale",
            AutolyserEnd::ParkedStorage => "Autolyser-parked-storage",
            AutolyserEnd::ParkedTime => "Autolyser-parked-time",
            AutolyserEnd::StorageFailed => "Autolyser-storage-failed",
            AutolyserEnd::StorageTimeout => "Autolyser-storage-timeout",
            AutolyserEnd::TransferCancelled => "Autolyser-transfer-cancelled",
            AutolyserEnd::TransferDisconnected => "Autolyser-transfer-disconnected",
            AutolyserEnd::TransferError => "Autolyser-transfer-error",
            AutolyserEnd::TransferTimeout => "Autolyser-transfer-timeout",
            AutolyserEnd::WriteFailed => "Autolyser-write-failed",
        },
        Code::Hl7(code) => match code {
            Hl7End::Cr => "HL7-CR",
            Hl7End::DsrDisconnect => "HL7-DSR-disconnect",
            Hl7End::DsrSessionError => "HL7-DSR-session-error",
            Hl7End::Ok => "HL7-OK",
            Hl7End::ApplicationNegative => "HL7-application-negative",
            Hl7End::OrderDisconnect => "HL7-order-disconnect",
            Hl7End::OrderNotReported => "HL7-order-not-reported",
            Hl7End::OrderResponseTimeout => "HL7-order-response-timeout",
            Hl7End::OrderSessionError => "HL7-order-session-error",
            Hl7End::PlacerIdentityTooLong => "HL7-placer-identity-too-long",
        },
        Code::Law(code) => match code {
            LawEnd::Cr => "LAW-CR",
            LawEnd::Ok => "LAW-OK",
            LawEnd::OrlItemMissing => "LAW-ORL-item-missing",
            LawEnd::OrlLate => "LAW-ORL-late",
            LawEnd::OrlTimeout => "LAW-ORL-timeout",
            LawEnd::Ua => "LAW-UA",
            LawEnd::Uc => "LAW-UC",
            LawEnd::Disconnect => "LAW-disconnect",
            LawEnd::IdleAdmissionFailed => "LAW-idle-admission-failed",
            LawEnd::IndependentAdmissionFailed => "LAW-independent-admission-failed",
            LawEnd::IndependentExchangeEnded => "LAW-independent-exchange-ended",
            LawEnd::QueryIssuanceUncertain => "LAW-independent-query-response-issuance-uncertain",
            LawEnd::IndependentQueueDeadline => "LAW-independent-queue-deadline",
            LawEnd::IndependentShutdown => "LAW-independent-shutdown",
            LawEnd::ResponseShutdown => "LAW-independent-shutdown-before-response",
            LawEnd::NegativeQueryOrl => "LAW-negative-query-ORL",
            LawEnd::QueryPreparationFailed => "LAW-query-preparation-failed",
            LawEnd::SendPreparationFailed => "LAW-send-preparation-failed",
            LawEnd::SessionError => "LAW-session-error",
        },
        Code::Vidas(code) => match code {
            VidasEnd::AcknowledgmentTimeout => "VIDAS-acknowledgment-timeout",
            VidasEnd::CommitTimeout => "VIDAS-commit-timeout",
            VidasEnd::ControlSuffixMismatch => "VIDAS-control-suffix-mismatch",
            VidasEnd::FramingError => "VIDAS-framing-error",
            VidasEnd::InputFailed => "VIDAS-input-failed",
            VidasEnd::InterbyteTimeout => "VIDAS-interbyte-timeout",
            VidasEnd::SessionTimeout => "VIDAS-session-timeout",
            VidasEnd::TransferCancelled => "VIDAS-transfer-cancelled",
            VidasEnd::TransferConfirmed => "VIDAS-transfer-confirmed",
            VidasEnd::TransferDisconnected => "VIDAS-transfer-disconnected",
            VidasEnd::TransferError => "VIDAS-transfer-error",
            VidasEnd::TransferRefusedOrUnexpected => "VIDAS-transfer-refused-or-unexpected",
            VidasEnd::WriteFailed => "VIDAS-write-failed",
        },
    }
}

/// Every fixed code, one protocol after another.
fn every_code() -> Vec<Code> {
    use AutolyserEnd as A;
    use VidasEnd as V;
    let host = [
        HostEnd::QueryReservationRefused,
        HostEnd::QueryDeadlineBeforeSend,
        HostEnd::HostRestart,
        HostEnd::ConnectionEndedBeforeConfirmation,
    ];
    let astm = [
        AstmEnd::Confirmed,
        AstmEnd::ReplacementUnverified,
        AstmEnd::Aborted,
        AstmEnd::Timeout,
        AstmEnd::Interrupted,
        AstmEnd::Incomplete,
    ];
    let autolyser = [
        A::InputFailed,
        A::ParkedAck,
        A::ParkedBusy,
        A::ParkedConfig,
        A::ParkedIncomplete,
        A::ParkedStale,
        A::ParkedStorage,
        A::ParkedTime,
        A::StorageFailed,
        A::StorageTimeout,
        A::TransferCancelled,
        A::TransferDisconnected,
        A::TransferError,
        A::TransferTimeout,
        A::WriteFailed,
    ];
    let hl7 = [
        Hl7End::Cr,
        Hl7End::DsrDisconnect,
        Hl7End::DsrSessionError,
        Hl7End::Ok,
        Hl7End::ApplicationNegative,
        Hl7End::OrderDisconnect,
        Hl7End::OrderNotReported,
        Hl7End::OrderResponseTimeout,
        Hl7End::OrderSessionError,
        Hl7End::PlacerIdentityTooLong,
    ];
    let law = [
        LawEnd::Cr,
        LawEnd::Ok,
        LawEnd::OrlItemMissing,
        LawEnd::OrlLate,
        LawEnd::OrlTimeout,
        LawEnd::Ua,
        LawEnd::Uc,
        LawEnd::Disconnect,
        LawEnd::IdleAdmissionFailed,
        LawEnd::IndependentAdmissionFailed,
        LawEnd::IndependentExchangeEnded,
        LawEnd::QueryIssuanceUncertain,
        LawEnd::IndependentQueueDeadline,
        LawEnd::IndependentShutdown,
        LawEnd::ResponseShutdown,
        LawEnd::NegativeQueryOrl,
        LawEnd::QueryPreparationFailed,
        LawEnd::SendPreparationFailed,
        LawEnd::SessionError,
    ];
    let vidas = [
        V::AcknowledgmentTimeout,
        V::CommitTimeout,
        V::ControlSuffixMismatch,
        V::FramingError,
        V::InputFailed,
        V::InterbyteTimeout,
        V::SessionTimeout,
        V::TransferCancelled,
        V::TransferConfirmed,
        V::TransferDisconnected,
        V::TransferError,
        V::TransferRefusedOrUnexpected,
        V::WriteFailed,
    ];
    host.into_iter()
        .map(Code::Host)
        .chain(astm.map(Code::Astm))
        .chain(autolyser.map(Code::Autolyser))
        .chain(hl7.map(Code::Hl7))
        .chain(law.map(Code::Law))
        .chain(vidas.map(Code::Vidas))
        .collect()
}

#[test]
fn every_fixed_code_keeps_its_stored_spelling() {
    let codes = every_code();
    assert_eq!(codes.len(), 67);
    let mut spellings = std::collections::BTreeSet::new();
    for code in codes {
        let expected = pinned(code);
        assert!(spellings.insert(expected), "{expected} is stored twice");
        let (stored, shown) = match code {
            Code::Host(code) => (code.as_str(), code.to_string()),
            Code::Astm(code) => (code.as_str(), code.to_string()),
            Code::Autolyser(code) => (code.as_str(), code.to_string()),
            Code::Hl7(code) => (code.as_str(), code.to_string()),
            Code::Law(code) => (code.as_str(), code.to_string()),
            Code::Vidas(code) => (code.as_str(), code.to_string()),
        };
        assert_eq!(stored, expected);
        assert_eq!(shown, expected);
    }
}

#[test]
fn detailed_codes_keep_their_stored_spelling() {
    let detail = || "detail".to_owned();
    for (value, expected) in [
        (
            Hl7Detail::OrderPreflight(detail()).to_string(),
            "HL7-order-preflight:detail",
        ),
        (
            Hl7Detail::InvalidOrderResponse(detail()).to_string(),
            "HL7-invalid-order-response:detail",
        ),
        (Hl7Detail::Ua(detail()).to_string(), "HL7-UA:detail"),
        (Hl7Detail::Uc(detail()).to_string(), "HL7-UC:detail"),
        (
            Hl7Detail::DisplayMapping(detail()).to_string(),
            "HL7-display-mapping: detail",
        ),
        (
            LawDetail::InvalidOrl(detail()).to_string(),
            "LAW-invalid-ORL:detail",
        ),
        (
            LawDetail::OrderPreflight(detail()).to_string(),
            "LAW-order-preflight:detail",
        ),
        (
            LawDetail::IndependentFailure(detail()).to_string(),
            "LAW-independent-failure:detail",
        ),
        (
            AutolyserDetail::Framing(detail()).to_string(),
            "Autolyser-parked-framing-detail",
        ),
        (
            AutolyserDetail::Record(detail()).to_string(),
            "Autolyser-parked-record-detail",
        ),
    ] {
        assert_eq!(value, expected);
    }
    for (code, expected) in [
        (AckCode::Aa, "LAW-ORL-application-Aa"),
        (AckCode::Ae, "LAW-ORL-application-Ae"),
        (AckCode::Ar, "LAW-ORL-application-Ar"),
        (AckCode::Ca, "LAW-ORL-application-Ca"),
        (AckCode::Ce, "LAW-ORL-application-Ce"),
        (AckCode::Cr, "LAW-ORL-application-Cr"),
    ] {
        assert_eq!(LawDetail::Application(code).to_string(), expected);
    }
}

#[test]
fn autolyser_parked_replies_keep_their_stored_spelling() {
    use crate::autolyser::Error;
    let framing = analyte_framing::autolyser::Error::Checksum;
    for (error, expected) in [
        (Error::Configuration, "Autolyser-parked-config".to_owned()),
        (Error::Busy, "Autolyser-parked-busy".to_owned()),
        (Error::TimeReversed, "Autolyser-parked-time".to_owned()),
        (Error::Exhausted, "Autolyser-parked-time".to_owned()),
        (Error::Incomplete, "Autolyser-parked-incomplete".to_owned()),
        (Error::Storage, "Autolyser-parked-storage".to_owned()),
        (Error::Stale, "Autolyser-parked-stale".to_owned()),
        (Error::Ack, "Autolyser-parked-ack".to_owned()),
        (
            Error::Framing(framing.clone()),
            format!("Autolyser-parked-framing-{framing}"),
        ),
    ] {
        assert_eq!(parked(&error), expected);
    }
    assert_eq!(
        parked(&Error::Framing(framing)),
        "Autolyser-parked-framing-checksum mismatch"
    );
}

#[test]
fn insertion_rejection_spellings_are_compatible() {
    use crate::autolyser::InsertionError;
    for (error, expected) in [
        (InsertionError::Checksum, "Checksum"),
        (InsertionError::UnknownCommand, "UnknownCommand"),
        (InsertionError::List, "List"),
        (InsertionError::Sample, "Sample"),
        (InsertionError::Clone, "Clone"),
        (InsertionError::Position, "Position"),
        (InsertionError::CountField, "CountField"),
        (InsertionError::CountMismatch, "CountMismatch"),
        (InsertionError::Executing, "Executing"),
        (InsertionError::CannotClone, "CannotClone"),
        (InsertionError::Duplicate, "Duplicate"),
        (InsertionError::UnknownAnalysis, "UnknownAnalysis"),
        (InsertionError::AnalysisOffTray, "AnalysisOffTray"),
        (InsertionError::TooManyAnalyses, "TooManyAnalyses"),
        (InsertionError::Constant, "Constant"),
        (InsertionError::Variable, "Variable"),
        (InsertionError::AnalysisVariable, "AnalysisVariable"),
        (InsertionError::NoRepeatPatient, "NoRepeatPatient"),
        (InsertionError::RepeatSampleMismatch, "RepeatSampleMismatch"),
        (InsertionError::RepeatListFull, "RepeatListFull"),
        (InsertionError::RepeatListMismatch, "RepeatListMismatch"),
        (InsertionError::PositionOccupied, "PositionOccupied"),
        (InsertionError::ExtraListPatient, "ExtraListPatient"),
        (InsertionError::RepeatDisabled, "RepeatDisabled"),
        (InsertionError::Other(255), "Other(255)"),
    ] {
        assert_eq!(insertion_rejection(error), expected);
    }
}

#[test]
fn insertion_and_incomplete_code_spellings_are_compatible() {
    use crate::autolyser::{InsertionError, Reception};
    for (value, expected) in [
        (
            Reception::Accepted(3),
            "Autolyser-insertion-confirmed-position-03",
        ),
        (
            Reception::Rejected(InsertionError::Position),
            "Autolyser-insertion-rejected-Position",
        ),
        (
            Reception::Rejected(InsertionError::Other(255)),
            "Autolyser-insertion-rejected-Other(255)",
        ),
        (
            Reception::Other(0x3f, 3),
            "Autolyser-insertion-unknown-3F-03",
        ),
    ] {
        assert_eq!(insertion(value), expected);
    }
    for (value, expected) in [
        (
            Interruption::Timeout,
            "Autolyser-transfer-timeout-incomplete",
        ),
        (
            Interruption::Disconnected,
            "Autolyser-transfer-disconnected-incomplete",
        ),
        (
            Interruption::Cancelled,
            "Autolyser-transfer-cancelled-incomplete",
        ),
    ] {
        assert_eq!(value.incomplete(), expected);
    }
}

#[test]
fn acceptance_evidence_spellings_are_compatible() {
    for (code, expected) in [
        (AckCode::Aa, "Aa"),
        (AckCode::Ae, "Ae"),
        (AckCode::Ar, "Ar"),
        (AckCode::Ca, "Ca"),
        (AckCode::Ce, "Ce"),
        (AckCode::Cr, "Cr"),
    ] {
        assert_eq!(acceptance(code), expected);
    }
}
