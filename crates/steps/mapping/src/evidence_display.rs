//! Writes stable mapping evidence spellings, preserving the historical protected-store format.
use crate::{Position, Problem};
use analyte_model::quoted;
use std::fmt;

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Position {{ record: {}, field: {} }}",
            self.record, self.field
        )
    }
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnqualifiedHl7Report => f.write_str("UnqualifiedHl7Report"),
            Self::DepartureConflict => f.write_str("DepartureConflict"),
            Self::DepartureDerivedStatusRequiresContract => {
                f.write_str("DepartureDerivedStatusRequiresContract")
            }
            Self::ComponentParts => f.write_str("ComponentParts"),
            Self::MissingSample => f.write_str("MissingSample"),
            Self::UnknownTest => f.write_str("UnknownTest"),
            Self::UnsupportedValue => f.write_str("UnsupportedValue"),
            Self::UnsupportedFormatting => f.write_str("UnsupportedFormatting"),
            Self::UnsupportedEscape => f.write_str("UnsupportedEscape"),
            Self::UnknownProcessing => f.write_str("UnknownProcessing"),
            Self::ConflictingClassification => f.write_str("ConflictingClassification"),
            Self::NoObservations => f.write_str("NoObservations"),
            Self::UnsupportedLawStatusOnly => f.write_str("UnsupportedLawStatusOnly"),
            Self::InvalidTime => f.write_str("InvalidTime"),
            Self::ValueKindMismatch { expected } => {
                let value = match expected {
                    analyte_config::ValueKind::Numeric => "Numeric",
                    analyte_config::ValueKind::Coded => "Coded",
                    analyte_config::ValueKind::Either => "Either",
                };
                write!(f, "ValueKindMismatch {{ expected: {value} }}")
            }
            Self::MissingIdentity { setting } => {
                f.write_str("MissingIdentity { setting: ")?;
                quoted(f, setting)?;
                f.write_str(" }")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mapping_evidence_spellings_are_compatible() {
        assert_eq!(
            Position {
                record: 12,
                field: 3
            }
            .to_string(),
            "Position { record: 12, field: 3 }"
        );
        for (problem, expected) in [
            (Problem::UnqualifiedHl7Report, "UnqualifiedHl7Report"),
            (Problem::DepartureConflict, "DepartureConflict"),
            (
                Problem::DepartureDerivedStatusRequiresContract,
                "DepartureDerivedStatusRequiresContract",
            ),
            (Problem::ComponentParts, "ComponentParts"),
            (Problem::MissingSample, "MissingSample"),
            (Problem::UnknownTest, "UnknownTest"),
            (Problem::UnsupportedValue, "UnsupportedValue"),
            (Problem::UnsupportedFormatting, "UnsupportedFormatting"),
            (Problem::UnsupportedEscape, "UnsupportedEscape"),
            (Problem::UnknownProcessing, "UnknownProcessing"),
            (
                Problem::ConflictingClassification,
                "ConflictingClassification",
            ),
            (Problem::NoObservations, "NoObservations"),
            (
                Problem::UnsupportedLawStatusOnly,
                "UnsupportedLawStatusOnly",
            ),
            (Problem::InvalidTime, "InvalidTime"),
        ] {
            assert_eq!(problem.to_string(), expected);
        }
        for (kind, expected) in [
            (
                analyte_config::ValueKind::Numeric,
                "ValueKindMismatch { expected: Numeric }",
            ),
            (
                analyte_config::ValueKind::Coded,
                "ValueKindMismatch { expected: Coded }",
            ),
            (
                analyte_config::ValueKind::Either,
                "ValueKindMismatch { expected: Either }",
            ),
        ] {
            assert_eq!(
                Problem::ValueKindMismatch { expected: kind }.to_string(),
                expected
            );
        }
        assert_eq!(
            Problem::MissingIdentity {
                setting: "LAW order"
            }
            .to_string(),
            "MissingIdentity { setting: \"LAW order\" }"
        );
    }
}
