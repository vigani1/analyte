//! Defines stored Autolyser order outcome codes: transfer ends, parked replies and insertions.
/// Stable stored codes; spellings are compatibility data, never derived from Debug.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AutolyserEnd {
    InputFailed,
    ParkedAck,
    ParkedBusy,
    ParkedConfig,
    ParkedIncomplete,
    ParkedStale,
    ParkedStorage,
    ParkedTime,
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
            Self::InputFailed => "Autolyser-input-failed",
            Self::ParkedAck => "Autolyser-parked-ack",
            Self::ParkedBusy => "Autolyser-parked-busy",
            Self::ParkedConfig => "Autolyser-parked-config",
            Self::ParkedIncomplete => "Autolyser-parked-incomplete",
            Self::ParkedStale => "Autolyser-parked-stale",
            Self::ParkedStorage => "Autolyser-parked-storage",
            Self::ParkedTime => "Autolyser-parked-time",
            Self::StorageFailed => "Autolyser-storage-failed",
            Self::StorageTimeout => "Autolyser-storage-timeout",
            Self::TransferCancelled => "Autolyser-transfer-cancelled",
            Self::TransferDisconnected => "Autolyser-transfer-disconnected",
            Self::TransferError => "Autolyser-transfer-error",
            Self::TransferTimeout => "Autolyser-transfer-timeout",
            Self::WriteFailed => "Autolyser-write-failed",
        }
    }
}
impl std::fmt::Display for AutolyserEnd {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
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
/// How an Autolyser reply was cut short; only these give an incomplete code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Interruption {
    Timeout,
    Disconnected,
    Cancelled,
}
impl Interruption {
    /// The transfer end this interruption also names when the session ends.
    pub(crate) const fn end(self) -> AutolyserEnd {
        match self {
            Self::Timeout => AutolyserEnd::TransferTimeout,
            Self::Disconnected => AutolyserEnd::TransferDisconnected,
            Self::Cancelled => AutolyserEnd::TransferCancelled,
        }
    }
    /// Writes the historical code of a reply parked as incomplete after this interruption.
    pub(crate) fn incomplete(self) -> String {
        format!("{}-incomplete", self.end().as_str())
    }
}

/// Stored codes that carry diagnostic text; the caller bounds the text before storing it.
pub(crate) enum AutolyserDetail {
    Framing(String),
    Record(String),
}
impl std::fmt::Display for AutolyserDetail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Framing(detail) => write!(f, "Autolyser-parked-framing-{detail}"),
            Self::Record(detail) => write!(f, "Autolyser-parked-record-{detail}"),
        }
    }
}

/// Writes the historical code of a reply the dialog parked, from its reason.
pub(crate) fn parked(error: &analyte_dialog::autolyser::Error) -> String {
    use analyte_dialog::autolyser::Error;
    let code = match error {
        Error::Configuration => AutolyserEnd::ParkedConfig,
        Error::Busy => AutolyserEnd::ParkedBusy,
        Error::TimeReversed | Error::Exhausted => AutolyserEnd::ParkedTime,
        Error::Incomplete => AutolyserEnd::ParkedIncomplete,
        Error::Storage => AutolyserEnd::ParkedStorage,
        Error::Stale => AutolyserEnd::ParkedStale,
        Error::Ack => AutolyserEnd::ParkedAck,
        Error::Framing(cause) => return AutolyserDetail::Framing(cause.to_string()).to_string(),
        Error::Record(cause) => return AutolyserDetail::Record(cause.to_string()).to_string(),
    };
    code.as_str().to_owned()
}
