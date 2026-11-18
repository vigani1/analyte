//! Defines stored Autolyser order outcome codes: transfer ends, parked replies and insertions.
use super::{Error, InsertionError, Reception};

codes! {
    /// Stored Autolyser order transfer outcome codes.
    Code {
        InputFailed => "Autolyser-input-failed",
        ParkedAck => "Autolyser-parked-ack",
        ParkedBusy => "Autolyser-parked-busy",
        ParkedConfig => "Autolyser-parked-config",
        ParkedIncomplete => "Autolyser-parked-incomplete",
        ParkedStale => "Autolyser-parked-stale",
        ParkedStorage => "Autolyser-parked-storage",
        ParkedTime => "Autolyser-parked-time",
        StorageFailed => "Autolyser-storage-failed",
        StorageTimeout => "Autolyser-storage-timeout",
        TransferCancelled => "Autolyser-transfer-cancelled",
        TransferDisconnected => "Autolyser-transfer-disconnected",
        TransferError => "Autolyser-transfer-error",
        TransferTimeout => "Autolyser-transfer-timeout",
        WriteFailed => "Autolyser-write-failed",
    }
}

/// Writes historical insertion rejection evidence; unknown byte values remain decimal.
pub(crate) fn insertion_rejection(error: InsertionError) -> String {
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
#[must_use]
pub fn insertion(reception: Reception) -> String {
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
pub enum Interruption {
    /// A deadline expired.
    Timeout,
    /// The connection closed.
    Disconnected,
    /// The host cancelled the session.
    Cancelled,
}
impl Interruption {
    /// The transfer end this interruption also names when the session ends.
    #[must_use]
    pub const fn end(self) -> Code {
        match self {
            Self::Timeout => Code::TransferTimeout,
            Self::Disconnected => Code::TransferDisconnected,
            Self::Cancelled => Code::TransferCancelled,
        }
    }
    /// Writes the historical code of a reply parked as incomplete after this interruption.
    #[must_use]
    pub fn incomplete(self) -> String {
        format!("{}-incomplete", self.end().as_str())
    }
}

/// Stored codes that carry diagnostic text; the caller bounds the text before storing it.
pub(crate) enum Detail {
    /// Stored as `Autolyser-parked-framing-<text>`.
    Framing(String),
    /// Stored as `Autolyser-parked-record-<text>`.
    Record(String),
}
impl std::fmt::Display for Detail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Framing(detail) => write!(f, "Autolyser-parked-framing-{detail}"),
            Self::Record(detail) => write!(f, "Autolyser-parked-record-{detail}"),
        }
    }
}

/// Writes the historical code of a reply the dialog parked, from its reason.
#[must_use]
pub fn parked(error: &Error) -> String {
    let code = match error {
        Error::Configuration => Code::ParkedConfig,
        Error::Busy => Code::ParkedBusy,
        Error::TimeReversed | Error::Exhausted => Code::ParkedTime,
        Error::Incomplete => Code::ParkedIncomplete,
        Error::Storage => Code::ParkedStorage,
        Error::Stale => Code::ParkedStale,
        Error::Ack => Code::ParkedAck,
        Error::Framing(cause) => return Detail::Framing(cause.to_string()).to_string(),
        Error::Record(cause) => return Detail::Record(cause.to_string()).to_string(),
    };
    code.as_str().to_owned()
}
