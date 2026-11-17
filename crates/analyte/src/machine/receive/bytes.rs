//! Defines typed reasons for retaining uninterpreted machine bytes.
/// Closed evidence vocabulary; spellings are protected-store compatibility data.
pub(crate) enum ByteReason<'a> {
    /// same ASTM frame number carried conflicting bytes.
    AstmConflict,
    /// unsolicited ASTM frame.
    AstmUnsolicited,
    /// HL7 unsolicited message or queue overflow.
    Hl7Unsolicited,
    /// LAW receipt processing interrupted before commit confirmation.
    LawInterrupted,
    /// Interrupted input: ownership uncertain; possible duplicates.
    InputUncertain,
    /// Interrupted input: fed prefix; possible duplicate of dialog receipts.
    InputPrefix,
    /// Interrupted input: untouched transport tail.
    InputTail,
    /// VIDAS control suffix mismatch.
    VidasSuffix,
    /// VIDAS transfer incomplete or refused; retained transport journal requires reconciliation.
    VidasIncomplete,
    /// POCT XML message byte limit exceeded.
    PoctLimit,
    /// POCT partial XML on cancellation.
    PoctCancelled,
    /// POCT partial XML exceeded dialog deadline.
    PoctDeadline,
    /// POCT incomplete XML on timeout or disconnect.
    PoctIncomplete,
    /// POCT message deferred by bounded custody queue.
    PoctDeferred,
    /// ASTM frame rejection diagnostic.
    AstmFrame(&'a analyte_framing::astm::Error),
    /// MLLP envelope rejection diagnostic.
    Mllp(&'a analyte_framing::hl7::Error),
    /// HL7 conversation rejection diagnostic.
    Hl7Dialog(&'a analyte_dialog::hl7::Error),
    /// VIDAS packet rejection diagnostic.
    VidasFrame(&'a analyte_framing::vidas::Error),
    /// POCT syntax rejection diagnostic.
    PoctSyntax(&'a analyte_poct::MessageError),
    /// POCT conversation refusal.
    PoctDialog(analyte_dialog::poct::RefusalCode),
    /// POCT closure evidence, including bounded unresolved ticket details.
    PoctClosure { normal: bool, unresolved: &'a str },
    /// Autolyser conversation refusal.
    Autolyser(&'a analyte_dialog::autolyser::Error),
    /// Synthetic fixture evidence.
    #[cfg(test)]
    Synthetic,
}
impl std::fmt::Display for ByteReason<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AstmConflict => f.write_str("same ASTM frame number carried conflicting bytes"),
            Self::AstmUnsolicited => f.write_str("unsolicited ASTM frame"),
            Self::Hl7Unsolicited => f.write_str("HL7 unsolicited message or queue overflow"),
            Self::LawInterrupted => f.write_str("LAW receipt processing interrupted before commit confirmation"),
            Self::InputUncertain => f.write_str("Interrupted input: ownership uncertain; possible duplicates"),
            Self::InputPrefix => f.write_str("Interrupted input: fed prefix; possible duplicate of dialog receipts"),
            Self::InputTail => f.write_str("Interrupted input: untouched transport tail"),
            Self::VidasSuffix => f.write_str("VIDAS control suffix mismatch"),
            Self::VidasIncomplete => f.write_str("VIDAS transfer incomplete or refused; retained transport journal requires reconciliation"),
            Self::PoctLimit => f.write_str("POCT XML message byte limit exceeded"),
            Self::PoctCancelled => f.write_str("POCT partial XML on cancellation"),
            Self::PoctDeadline => f.write_str("POCT partial XML exceeded dialog deadline"),
            Self::PoctIncomplete => f.write_str("POCT incomplete XML on timeout or disconnect"),
            Self::PoctDeferred => f.write_str("POCT message deferred by bounded custody queue"),
            Self::AstmFrame(error) => write!(f, "{error}"),
            Self::Mllp(error) => write!(f, "{error}"),
            Self::Hl7Dialog(error) => write!(f, "{error}"),
            Self::VidasFrame(error) => write!(f, "VIDAS framing: {error}"),
            Self::PoctSyntax(error) => write!(f, "POCT syntax: {error}"),
            Self::PoctDialog(reason) => f.write_str(reason.as_str()),
            Self::PoctClosure { normal, unresolved } => write!(f, "POCT dialog closure: normal={normal}; unresolved custody tickets: {unresolved}; inspect retained transport journal"),
            Self::Autolyser(error) => write!(f, "Autolyser: {error}"),
            #[cfg(test)]
            Self::Synthetic => f.write_str("synthetic parking"),
        }
    }
}
#[test]
fn byte_reason_spellings_are_pinned() {
    assert_eq!(
        ByteReason::AstmConflict.to_string(),
        "same ASTM frame number carried conflicting bytes"
    );
    assert_eq!(
        ByteReason::AstmUnsolicited.to_string(),
        "unsolicited ASTM frame"
    );
    assert_eq!(
        ByteReason::Hl7Unsolicited.to_string(),
        "HL7 unsolicited message or queue overflow"
    );
    assert_eq!(
        ByteReason::LawInterrupted.to_string(),
        "LAW receipt processing interrupted before commit confirmation"
    );
    assert_eq!(
        ByteReason::InputUncertain.to_string(),
        "Interrupted input: ownership uncertain; possible duplicates"
    );
    assert_eq!(
        ByteReason::InputPrefix.to_string(),
        "Interrupted input: fed prefix; possible duplicate of dialog receipts"
    );
    assert_eq!(
        ByteReason::InputTail.to_string(),
        "Interrupted input: untouched transport tail"
    );
    assert_eq!(
        ByteReason::VidasSuffix.to_string(),
        "VIDAS control suffix mismatch"
    );
    assert_eq!(
        ByteReason::VidasIncomplete.to_string(),
        "VIDAS transfer incomplete or refused; retained transport journal requires reconciliation"
    );
    assert_eq!(
        ByteReason::PoctLimit.to_string(),
        "POCT XML message byte limit exceeded"
    );
    assert_eq!(
        ByteReason::PoctCancelled.to_string(),
        "POCT partial XML on cancellation"
    );
    assert_eq!(
        ByteReason::PoctDeadline.to_string(),
        "POCT partial XML exceeded dialog deadline"
    );
    assert_eq!(
        ByteReason::PoctIncomplete.to_string(),
        "POCT incomplete XML on timeout or disconnect"
    );
    assert_eq!(
        ByteReason::PoctDeferred.to_string(),
        "POCT message deferred by bounded custody queue"
    );
    assert_eq!(ByteReason::Synthetic.to_string(), "synthetic parking");
    assert_eq!(
        ByteReason::PoctClosure {
            normal: false,
            unresolved: "1,2"
        }
        .to_string(),
        "POCT dialog closure: normal=false; unresolved custody tickets: 1,2; inspect retained transport journal"
    );
}

#[test]
fn diagnostic_reason_spellings_are_pinned() {
    assert_eq!(
        ByteReason::AstmFrame(&analyte_framing::astm::Error::RecordBoundary).to_string(),
        "multiple or prematurely terminated LIS02 records in frame"
    );
    assert_eq!(
        ByteReason::Mllp(&analyte_framing::hl7::Error::MissingCr).to_string(),
        "missing CR trailer"
    );
    assert_eq!(
        ByteReason::Hl7Dialog(&analyte_dialog::hl7::Error::Exhausted).to_string(),
        "identity or time exhausted"
    );
    assert_eq!(
        ByteReason::VidasFrame(&analyte_framing::vidas::Error::Checksum).to_string(),
        "VIDAS framing: checksum mismatch"
    );
    assert_eq!(
        ByteReason::PoctSyntax(&analyte_poct::MessageError::Structure).to_string(),
        "POCT syntax: invalid message structure"
    );
    assert_eq!(
        ByteReason::PoctDialog(analyte_dialog::poct::RefusalCode::KeepAlive).to_string(),
        "POCT unsupported Keep Alive"
    );
    assert_eq!(
        ByteReason::Autolyser(&analyte_dialog::autolyser::Error::Storage).to_string(),
        "Autolyser: durable storage unavailable"
    );
}
