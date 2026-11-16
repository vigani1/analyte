//! Writes stable acknowledgment evidence independently of Rust variant names.
use super::{AckCode, Condition};
use std::fmt;

impl fmt::Display for AckCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Aa => "Aa",
            Self::Ae => "Ae",
            Self::Ar => "Ar",
            Self::Ca => "Ca",
            Self::Ce => "Ce",
            Self::Cr => "Cr",
        })
    }
}

#[test]
fn ackcode_spellings_are_compatible() {
    for (value, expected) in [
        (AckCode::Aa, "Aa"),
        (AckCode::Ae, "Ae"),
        (AckCode::Ar, "Ar"),
        (AckCode::Ca, "Ca"),
        (AckCode::Ce, "Ce"),
        (AckCode::Cr, "Cr"),
    ] {
        assert_eq!(value.to_string(), expected);
    }
}

impl fmt::Display for Condition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Always => "Always",
            Self::Never => "Never",
            Self::Error => "Error",
            Self::Success => "Success",
        })
    }
}

#[test]
fn condition_spellings_are_compatible() {
    for (value, expected) in [
        (Condition::Always, "Always"),
        (Condition::Never, "Never"),
        (Condition::Error, "Error"),
        (Condition::Success, "Success"),
    ] {
        assert_eq!(value.to_string(), expected);
    }
}
