//! Writes stable mapping evidence spellings, preserving the historical protected-store format, and reads them back.
use crate::{Finding, Position, Problem};
use analyte_model::quoted;
use std::fmt;

/// The stored position spelling is `Position { record: R, field: F }`.
const POSITION_OPEN: &str = "Position { record: ";
const POSITION_FIELD: &str = ", field: ";
const POSITION_CLOSE: &str = " }";

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{POSITION_OPEN}{}{POSITION_FIELD}{}{POSITION_CLOSE}",
            self.record, self.field
        )
    }
}

/// A piece of stored text: plain text, or a position written by [`Position`]'s `Display`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReasonPiece<'a> {
    /// Text exactly as stored.
    Text(&'a str),
    /// A position spelling, read back.
    Position(Position),
}

impl Position {
    /// Cuts stored text into plain text and the positions written inside it, in
    /// order; joining the pieces with their `Display` gives `text` back exactly.
    /// Text that only resembles a position stays text. The scan is linear and
    /// not recursive; the caller bounds `text` (a stored reason is bounded).
    #[must_use]
    pub fn split_stored(text: &str) -> Vec<ReasonPiece<'_>> {
        let mut pieces = Vec::new();
        let mut done = 0usize;
        for (start, _) in text.match_indices(POSITION_OPEN) {
            let Some((position, rest)) = text
                .get(start..)
                .filter(|_| start >= done)
                .and_then(Self::read_prefix)
            else {
                continue;
            };
            if let Some(before) = text.get(done..start).filter(|t| !t.is_empty()) {
                pieces.push(ReasonPiece::Text(before));
            }
            pieces.push(ReasonPiece::Position(position));
            done = text.len().saturating_sub(rest.len());
        }
        if let Some(tail) = text.get(done..).filter(|t| !t.is_empty()) {
            pieces.push(ReasonPiece::Text(tail));
        }
        pieces
    }

    /// Reads a position spelling at the start of `text`; returns it and the rest.
    fn read_prefix(text: &str) -> Option<(Self, &str)> {
        let rest = text.strip_prefix(POSITION_OPEN)?;
        let (record, rest) = number(rest)?;
        let rest = rest.strip_prefix(POSITION_FIELD)?;
        let (field, rest) = number(rest)?;
        let rest = rest.strip_prefix(POSITION_CLOSE)?;
        Some((Self { record, field }, rest))
    }
}

/// Reads a number the way `Display` writes one: ASCII digits, no sign and no
/// leading zero, so only an exact spelling reads back.
fn number(text: &str) -> Option<(usize, &str)> {
    let length = text.bytes().take_while(u8::is_ascii_digit).count();
    let (digits, rest) = text.split_at_checked(length)?;
    if digits.is_empty() || (digits.len() > 1 && digits.starts_with('0')) {
        return None;
    }
    Some((digits.parse().ok()?, rest))
}

/// Every problem whose spelling reads back. `MissingIdentity` is left out: its
/// setting name cannot be rebuilt from stored text.
const READABLE: [Problem; 17] = [
    Problem::UnqualifiedHl7Report,
    Problem::DepartureConflict,
    Problem::DerivedStatusContract,
    Problem::ComponentParts,
    Problem::MissingSample,
    Problem::UnknownTest,
    Problem::UnsupportedValue,
    Problem::UnsupportedFormatting,
    Problem::UnsupportedEscape,
    Problem::UnknownProcessing,
    Problem::ConflictingClassification,
    Problem::NoObservations,
    Problem::UnsupportedLawStatusOnly,
    Problem::InvalidTime,
    Problem::ValueKindMismatch {
        expected: analyte_config::ValueKind::Numeric,
    },
    Problem::ValueKindMismatch {
        expected: analyte_config::ValueKind::Coded,
    },
    Problem::ValueKindMismatch {
        expected: analyte_config::ValueKind::Either,
    },
];

impl Problem {
    /// Reads back a spelling written by `Display`, the exact reverse of it; any
    /// other text, and a spelling that cannot be rebuilt, gives `None`.
    #[must_use]
    pub fn from_stored(text: &str) -> Option<Self> {
        READABLE
            .iter()
            .find(|problem| problem.to_string() == text)
            .cloned()
    }
}

impl Finding {
    /// Reads back a finding written by `Display` (`{position}: {problem}`), the
    /// exact reverse of it; any other text gives `None`.
    #[must_use]
    pub fn from_stored(text: &str) -> Option<Self> {
        let (position, rest) = Position::read_prefix(text)?;
        let problem = Problem::from_stored(rest.strip_prefix(": ")?)?;
        Some(Self { position, problem })
    }
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnqualifiedHl7Report => f.write_str("UnqualifiedHl7Report"),
            Self::DepartureConflict => f.write_str("DepartureConflict"),
            Self::DerivedStatusContract => f.write_str("DerivedStatusContract"),
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

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.position, self.problem)
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
            (Problem::DerivedStatusContract, "DerivedStatusContract"),
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
            assert_eq!(Problem::from_stored(expected), Some(problem));
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
            let problem = Problem::ValueKindMismatch { expected: kind };
            assert_eq!(problem.to_string(), expected);
            assert_eq!(Problem::from_stored(expected), Some(problem));
        }
        assert_eq!(
            Problem::MissingIdentity {
                setting: "LAW order"
            }
            .to_string(),
            "MissingIdentity { setting: \"LAW order\" }"
        );
        // The setting name cannot be rebuilt, so this spelling gives the general advice.
        assert_eq!(
            Problem::from_stored("MissingIdentity { setting: \"LAW order\" }"),
            None
        );
        for problem in READABLE {
            let finding = Finding {
                position: Position {
                    record: 12,
                    field: 3,
                },
                problem,
            };
            let stored = finding.to_string();
            assert_eq!(
                stored,
                format!("Position {{ record: 12, field: 3 }}: {}", finding.problem)
            );
            assert_eq!(Finding::from_stored(&stored), Some(finding));
        }
    }

    // Owner review 2026-11-19 (structure review PR 2): every stored position is
    // found, text around it is kept exactly, and look-alikes stay text.
    #[test]
    fn stored_text_splits_into_text_and_positions() {
        use ReasonPiece::{Position as At, Text};
        let at = |record, field| At(Position { record, field });
        for (stored, pieces) in [
            ("", vec![]),
            ("no position here", vec![Text("no position here")]),
            (
                "Position { record: 3, field: 3 }: UnknownTest",
                vec![at(3, 3), Text(": UnknownTest")],
            ),
            (
                "departure-conflict at Position { record: 3, field: 25 }: present",
                vec![Text("departure-conflict at "), at(3, 25), Text(": present")],
            ),
            (
                "Position { record: 1, field: 0 }Position { record: 2, field: 7 }",
                vec![at(1, 0), at(2, 7)],
            ),
            (
                "Position { record: 01, field: 3 } and Position { record: 4, field: 0 }",
                vec![Text("Position { record: 01, field: 3 } and "), at(4, 0)],
            ),
            (
                "Position { record: Position { record: 5, field: 6 } }",
                vec![Text("Position { record: "), at(5, 6), Text(" }")],
            ),
        ] {
            let split = Position::split_stored(stored);
            assert_eq!(split, pieces, "{stored}");
            let joined: String = split
                .iter()
                .map(|piece| match piece {
                    Text(text) => (*text).to_owned(),
                    At(position) => position.to_string(),
                })
                .collect();
            assert_eq!(joined, stored);
        }
    }

    // Only the exact spelling reads back; anything else is not a finding.
    #[test]
    fn stored_findings_read_back_only_from_their_exact_spelling() {
        assert_eq!(
            Finding::from_stored("Position { record: 3, field: 0 }: MissingSample"),
            Some(Finding {
                position: Position {
                    record: 3,
                    field: 0
                },
                problem: Problem::MissingSample,
            })
        );
        for text in [
            "",
            "UnknownTest",
            "Position { record: 3, field: 3 }",
            "Position { record: 3, field: 3 }: UnknownTest ",
            "Position { record: 3, field: 3 }:UnknownTest",
            "Position { record: 03, field: 3 }: UnknownTest",
            "Position { record: +3, field: 3 }: UnknownTest",
            "Position { record: , field: 3 }: UnknownTest",
            "Position { record: 99999999999999999999999, field: 3 }: UnknownTest",
            "x Position { record: 3, field: 3 }: UnknownTest",
            "Position { record: 3, field: 3 }: unknowntest",
            "departure-conflict at Position { record: 3, field: 3 }: present non-final status",
        ] {
            assert_eq!(Finding::from_stored(text), None, "{text}");
        }
    }
}
