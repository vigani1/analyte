//! Writes stable historical LAW acceptance evidence independently of variant names.
use analyte_hl7::AckCode;

/// Preserves the protected-store spelling; these are not HL7 wire codes.
pub(super) const fn acceptance(code: AckCode) -> &'static str {
    match code {
        AckCode::Aa => "Aa",
        AckCode::Ae => "Ae",
        AckCode::Ar => "Ar",
        AckCode::Ca => "Ca",
        AckCode::Ce => "Ce",
        AckCode::Cr => "Cr",
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
