//! Holds the interrupted-input parking reasons and pins every stored parking spelling.
//! Each protocol host keeps its own reasons beside its code. The spellings are
//! protected-store compatibility data, so the tests here pin every one.
#[cfg(test)]
use crate::machine::{astm, autolyser, hl7, law, poct, vidas};

/// Interrupted input whose fed prefix is unknown: ownership uncertain.
pub(crate) const INPUT_UNCERTAIN: &str =
    "Interrupted input: ownership uncertain; possible duplicates";
/// Interrupted input that the dialog had already been fed.
pub(crate) const INPUT_PREFIX: &str =
    "Interrupted input: fed prefix; possible duplicate of dialog receipts";
/// Interrupted input that never entered the dialog.
pub(crate) const INPUT_TAIL: &str = "Interrupted input: untouched transport tail";
/// Synthetic fixture evidence.
#[cfg(test)]
pub(crate) const SYNTHETIC: &str = "synthetic parking";

#[test]
fn byte_reason_spellings_are_pinned() {
    assert_eq!(
        astm::CONFLICT_REASON,
        "same ASTM frame number carried conflicting bytes"
    );
    assert_eq!(astm::UNSOLICITED_REASON, "unsolicited ASTM frame");
    assert_eq!(
        hl7::UNSOLICITED_REASON,
        "HL7 unsolicited message or queue overflow"
    );
    assert_eq!(
        law::INTERRUPTED_REASON,
        "LAW receipt processing interrupted before commit confirmation"
    );
    assert_eq!(
        INPUT_UNCERTAIN,
        "Interrupted input: ownership uncertain; possible duplicates"
    );
    assert_eq!(
        INPUT_PREFIX,
        "Interrupted input: fed prefix; possible duplicate of dialog receipts"
    );
    assert_eq!(INPUT_TAIL, "Interrupted input: untouched transport tail");
    assert_eq!(vidas::SUFFIX_REASON, "VIDAS control suffix mismatch");
    assert_eq!(
        vidas::INCOMPLETE_REASON,
        "VIDAS transfer incomplete or refused; retained transport journal requires reconciliation"
    );
    assert_eq!(poct::LIMIT_REASON, "POCT XML message byte limit exceeded");
    assert_eq!(poct::CANCELLED_REASON, "POCT partial XML on cancellation");
    assert_eq!(
        poct::DEADLINE_REASON,
        "POCT partial XML exceeded dialog deadline"
    );
    assert_eq!(
        poct::INCOMPLETE_REASON,
        "POCT incomplete XML on timeout or disconnect"
    );
    assert_eq!(
        poct::DEFERRED_REASON,
        "POCT message deferred by bounded custody queue"
    );
    assert_eq!(SYNTHETIC, "synthetic parking");
    assert_eq!(
        poct::closure_reason(false, "1,2"),
        "POCT dialog closure: normal=false; unresolved custody tickets: 1,2; inspect retained transport journal"
    );
}

#[test]
fn diagnostic_reason_spellings_are_pinned() {
    assert_eq!(
        analyte_framing::astm::Error::RecordBoundary.to_string(),
        "multiple or prematurely terminated LIS02 records in frame"
    );
    assert_eq!(
        analyte_framing::hl7::Error::MissingCr.to_string(),
        "missing CR trailer"
    );
    assert_eq!(
        analyte_dialog::hl7::Error::Exhausted.to_string(),
        "identity or time exhausted"
    );
    assert_eq!(
        vidas::framing_reason(&analyte_framing::vidas::Error::Checksum),
        "VIDAS framing: checksum mismatch"
    );
    assert_eq!(
        poct::syntax_reason(&analyte_poct::MessageError::Structure),
        "POCT syntax: invalid message structure"
    );
    assert_eq!(
        analyte_dialog::poct::RefusalCode::HelloPolicy.as_str(),
        "POCT Hello rejected by host policy"
    );
    assert_eq!(
        autolyser::refusal_reason(&analyte_dialog::autolyser::Error::Storage),
        "Autolyser: durable storage unavailable"
    );
}
