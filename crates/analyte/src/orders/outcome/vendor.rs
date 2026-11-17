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
            Self::AcknowledgmentTimeout => super::Code::VidasAcknowledgmentTimeout.as_str(),
            Self::CommitTimeout => super::Code::VidasCommitTimeout.as_str(),
            Self::ControlSuffixMismatch => super::Code::VidasControlSuffixMismatch.as_str(),
            Self::FramingError => super::Code::VidasFramingError.as_str(),
            Self::InputFailed => super::Code::VidasInputFailed.as_str(),
            Self::InterbyteTimeout => super::Code::VidasInterbyteTimeout.as_str(),
            Self::SessionTimeout => super::Code::VidasSessionTimeout.as_str(),
            Self::TransferCancelled => super::Code::VidasTransferCancelled.as_str(),
            Self::TransferConfirmed => super::Code::VidasTransferConfirmed.as_str(),
            Self::TransferDisconnected => super::Code::VidasTransferDisconnected.as_str(),
            Self::TransferError => super::Code::VidasTransferError.as_str(),
            Self::TransferRefusedOrUnexpected => {
                super::Code::VidasTransferRefusedOrUnexpected.as_str()
            }
            Self::WriteFailed => super::Code::VidasWriteFailed.as_str(),
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

/// Stable stored codes; spellings are compatibility data, never derived from Debug.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AutolyserEnd {
    InputFailed,
    StorageFailed,
    StorageTimeout,
    TransferCancelled,
    TransferDisconnected,
    TransferError,
    TransferTimeout,
    WriteFailed,
}
impl AutolyserEnd {
    /// Returns the exact historical spelling retained in the protected store.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::InputFailed => super::Code::AutolyserInputFailed.as_str(),
            Self::StorageFailed => super::Code::AutolyserStorageFailed.as_str(),
            Self::StorageTimeout => super::Code::AutolyserStorageTimeout.as_str(),
            Self::TransferCancelled => super::Code::AutolyserTransferCancelled.as_str(),
            Self::TransferDisconnected => super::Code::AutolyserTransferDisconnected.as_str(),
            Self::TransferError => super::Code::AutolyserTransferError.as_str(),
            Self::TransferTimeout => super::Code::AutolyserTransferTimeout.as_str(),
            Self::WriteFailed => super::Code::AutolyserWriteFailed.as_str(),
        }
    }
}
impl std::fmt::Display for AutolyserEnd {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
#[test]
fn autolyser_stored_spellings_are_compatible() {
    for (code, expected) in [
        (AutolyserEnd::InputFailed, "Autolyser-input-failed"),
        (AutolyserEnd::StorageFailed, "Autolyser-storage-failed"),
        (AutolyserEnd::StorageTimeout, "Autolyser-storage-timeout"),
        (
            AutolyserEnd::TransferCancelled,
            "Autolyser-transfer-cancelled",
        ),
        (
            AutolyserEnd::TransferDisconnected,
            "Autolyser-transfer-disconnected",
        ),
        (AutolyserEnd::TransferError, "Autolyser-transfer-error"),
        (AutolyserEnd::TransferTimeout, "Autolyser-transfer-timeout"),
        (AutolyserEnd::WriteFailed, "Autolyser-write-failed"),
    ] {
        assert_eq!(code.as_str(), expected);
        assert_eq!(code.to_string(), expected);
    }
}

/// Writes historical insertion rejection evidence; unknown byte values remain decimal.
pub(crate) fn insertion_rejection(error: analyte_dialog::autolyser::InsertionError) -> String {
    use analyte_dialog::autolyser::InsertionError;
    match error {
        InsertionError::Checksum => "Checksum".to_owned(),
        InsertionError::UnknownCommand => "UnknownCommand".to_owned(),
        InsertionError::List => "List".to_owned(),
        InsertionError::Sample => "Sample".to_owned(),
        InsertionError::Clone => "Clone".to_owned(),
        InsertionError::Position => "Position".to_owned(),
        InsertionError::CountField => "CountField".to_owned(),
        InsertionError::CountMismatch => "CountMismatch".to_owned(),
        InsertionError::Executing => "Executing".to_owned(),
        InsertionError::CannotClone => "CannotClone".to_owned(),
        InsertionError::Duplicate => "Duplicate".to_owned(),
        InsertionError::UnknownAnalysis => "UnknownAnalysis".to_owned(),
        InsertionError::AnalysisOffTray => "AnalysisOffTray".to_owned(),
        InsertionError::TooManyAnalyses => "TooManyAnalyses".to_owned(),
        InsertionError::Constant => "Constant".to_owned(),
        InsertionError::Variable => "Variable".to_owned(),
        InsertionError::AnalysisVariable => "AnalysisVariable".to_owned(),
        InsertionError::NoRepeatPatient => "NoRepeatPatient".to_owned(),
        InsertionError::RepeatSampleMismatch => "RepeatSampleMismatch".to_owned(),
        InsertionError::RepeatListFull => "RepeatListFull".to_owned(),
        InsertionError::RepeatListMismatch => "RepeatListMismatch".to_owned(),
        InsertionError::PositionOccupied => "PositionOccupied".to_owned(),
        InsertionError::ExtraListPatient => "ExtraListPatient".to_owned(),
        InsertionError::RepeatDisabled => "RepeatDisabled".to_owned(),
        InsertionError::Other(byte) => format!("Other({byte})"),
    }
}

#[test]
fn insertion_rejection_spellings_are_compatible() {
    use analyte_dialog::autolyser::InsertionError;
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

/// Encodes each closed insertion response family with its historical stable prefix.
pub(crate) fn insertion(reception: analyte_dialog::autolyser::Reception) -> String {
    use analyte_dialog::autolyser::Reception;
    match reception {
        Reception::Accepted(position) => {
            format!("Autolyser-insertion-confirmed-position-{position:02X}")
        }
        Reception::Rejected(error) => format!(
            "Autolyser-insertion-rejected-{}",
            insertion_rejection(error)
        ),
        Reception::Other(prefix, detail) => {
            format!("Autolyser-insertion-unknown-{prefix:02X}-{detail:02X}")
        }
    }
}
impl AutolyserEnd {
    pub(crate) fn incomplete(self) -> String {
        format!("{}-incomplete", self.as_str())
    }
}
#[test]
fn insertion_and_incomplete_code_spellings_are_compatible() {
    use analyte_dialog::autolyser::{InsertionError, Reception};
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
            AutolyserEnd::TransferTimeout,
            "Autolyser-transfer-timeout-incomplete",
        ),
        (
            AutolyserEnd::TransferDisconnected,
            "Autolyser-transfer-disconnected-incomplete",
        ),
        (
            AutolyserEnd::TransferCancelled,
            "Autolyser-transfer-cancelled-incomplete",
        ),
    ] {
        assert_eq!(value.incomplete(), expected);
    }
}
