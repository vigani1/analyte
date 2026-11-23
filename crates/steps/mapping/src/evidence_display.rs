//! Writes stable mapping evidence spellings, preserving the historical protected-store format, and reads them back.
use crate::{Finding, IdentitySetting, Position, Problem};
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

/// Every problem without a setting; with [`IdentitySetting::ALL`] they are
/// every problem there is.
const READABLE: [Problem; 17] = [
    Problem::UnqualifiedHl7Report,
    Problem::DepartureConflict,
    Problem::DerivedStatusContract,
    Problem::ComponentParts,
    Problem::MissingSample,
    Problem::MissingTest,
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

/// Every problem there is, each `MissingIdentity` with each setting.
fn every_problem() -> impl Iterator<Item = Problem> {
    READABLE.into_iter().chain(
        IdentitySetting::ALL
            .into_iter()
            .map(|setting| Problem::MissingIdentity { setting }),
    )
}

impl Problem {
    /// Reads back a spelling written by `Display`, the exact reverse of it; any
    /// other text gives `None`.
    #[must_use]
    pub fn from_stored(text: &str) -> Option<Self> {
        every_problem().find(|problem| problem.to_string() == text)
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
            Self::MissingTest => f.write_str("MissingTest"),
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
                quoted(f, setting.as_str())?;
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

/// Why a mapped result has no identity, as its stored reason spells it. The
/// host parks each one with the message, so the spellings never change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unidentified {
    /// A required identity value is missing at `position`.
    Missing {
        /// The value that is missing.
        setting: IdentitySetting,
        /// Where it should have been.
        position: Position,
    },
    /// Autolyser Rev18 sends no run identity that never changes, and its
    /// patient code is not a trusted sample identity.
    AutolyserRun,
    /// Two POCT results derive the same identity and no sub-ID tells them apart.
    PoctRepeated,
    /// The POCT device, time or sequence evidence the configuration names is missing.
    PoctEvidence,
    /// A POCT service sequence may wrap, so it needs a trusted run and revision.
    PoctSequence,
    /// A VIDAS Literal has no trusted result or run identity and does not say
    /// whether it is a patient or a QC result.
    VidasLiteral,
}

/// The spelling of [`Unidentified::Missing`] is `missing identity S at record R field F`.
const MISSING_OPEN: &str = "missing identity ";
const MISSING_RECORD: &str = " at record ";
const MISSING_FIELD: &str = " field ";

impl Unidentified {
    /// Whether `finding` already names the cause of this reason, so the reason
    /// adds nothing for an operator. The mapper gives a missing identity value
    /// the reason [`Unidentified::Missing`] next to the finding at the same place.
    #[must_use]
    pub fn restates(&self, finding: &Finding) -> bool {
        match self {
            Self::Missing { setting, position } => {
                finding.position == *position
                    && finding.problem == Problem::MissingIdentity { setting: *setting }
            }
            Self::AutolyserRun
            | Self::PoctRepeated
            | Self::PoctEvidence
            | Self::PoctSequence
            | Self::VidasLiteral => false,
        }
    }

    /// Every reason without a position, for reading back.
    const FIXED: [Self; 5] = [
        Self::AutolyserRun,
        Self::PoctRepeated,
        Self::PoctEvidence,
        Self::PoctSequence,
        Self::VidasLiteral,
    ];

    /// Reads back a spelling written by `Display`, the exact reverse of it; any
    /// other text gives `None`.
    #[must_use]
    pub fn from_stored(text: &str) -> Option<Self> {
        if let Some(fixed) = Self::FIXED.into_iter().find(|r| r.to_string() == text) {
            return Some(fixed);
        }
        let rest = text.strip_prefix(MISSING_OPEN)?;
        IdentitySetting::ALL.into_iter().find_map(|setting| {
            let rest = rest
                .strip_prefix(setting.as_str())?
                .strip_prefix(MISSING_RECORD)?;
            let (record, rest) = number(rest)?;
            let (field, rest) = number(rest.strip_prefix(MISSING_FIELD)?)?;
            rest.is_empty().then_some(Self::Missing {
                setting,
                position: Position { record, field },
            })
        })
    }
}

impl fmt::Display for Unidentified {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing { setting, position } => write!(
                f,
                "{MISSING_OPEN}{setting}{MISSING_RECORD}{}{MISSING_FIELD}{}",
                position.record, position.field
            ),
            Self::AutolyserRun => f.write_str(concat!(
                "Autolyser Rev18 has no immutable run identity; ",
                "patient code is not trusted specimen identity",
            )),
            Self::PoctRepeated => f.write_str(
                "POCT repeated derived observation identity without a distinguishing sub-ID",
            ),
            Self::PoctEvidence => {
                f.write_str("POCT configured device/time/sequence identity evidence missing")
            }
            Self::PoctSequence => f.write_str(
                "POCT service sequence may wrap; trusted run and revision evidence is required",
            ),
            Self::VidasLiteral => f.write_str(concat!(
                "VIDAS Literal lacks trusted result/run identity ",
                "and patient versus QC classification",
            )),
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
            (Problem::DerivedStatusContract, "DerivedStatusContract"),
            (Problem::ComponentParts, "ComponentParts"),
            (Problem::MissingSample, "MissingSample"),
            (Problem::MissingTest, "MissingTest"),
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
        for (setting, expected) in [
            (IdentitySetting::OrderId, "order_id"),
            (IdentitySetting::OrderNamespace, "order_namespace"),
            (IdentitySetting::SubId, "sub_id"),
            (IdentitySetting::Sample, "sample"),
            (IdentitySetting::Run, "run"),
            (IdentitySetting::LawOrder, "LAW order"),
            (
                IdentitySetting::LawOrderAuthority,
                "LAW AWOS/analyzer order authority or OG",
            ),
            (
                IdentitySetting::LawSpecimenAwos,
                "LAW specimen observation has no AWOS",
            ),
        ] {
            let problem = Problem::MissingIdentity { setting };
            let stored = format!("MissingIdentity {{ setting: \"{expected}\" }}");
            assert_eq!(problem.to_string(), stored);
            assert_eq!(Problem::from_stored(&stored), Some(problem));
        }
        assert_eq!(
            Problem::from_stored("MissingIdentity { setting: \"other\" }"),
            None
        );
        for problem in every_problem() {
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
                "Position { record: 3, field: 3 }: MissingTest",
                vec![at(3, 3), Text(": MissingTest")],
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
            "MissingTest",
            "Position { record: 3, field: 3 }",
            "Position { record: 3, field: 3 }: MissingTest ",
            "Position { record: 3, field: 3 }:MissingTest",
            "Position { record: 03, field: 3 }: MissingTest",
            "Position { record: +3, field: 3 }: MissingTest",
            "Position { record: , field: 3 }: MissingTest",
            "Position { record: 99999999999999999999999, field: 3 }: MissingTest",
            "x Position { record: 3, field: 3 }: MissingTest",
            "Position { record: 3, field: 3 }: missingtest",
            "departure-conflict at Position { record: 3, field: 3 }: present non-final status",
        ] {
            assert_eq!(Finding::from_stored(text), None, "{text}");
        }
    }

    // The unresolved-identity reasons keep their stored spelling and read back
    // exactly; anything else is not one of them.
    #[test]
    fn unresolved_identity_reasons_keep_their_spelling_and_read_back() {
        let at = |record, field| Position { record, field };
        for (reason, expected) in [
            (
                Unidentified::Missing {
                    setting: IdentitySetting::OrderId,
                    position: at(3, 2),
                },
                "missing identity order_id at record 3 field 2",
            ),
            (
                Unidentified::Missing {
                    setting: IdentitySetting::LawOrderAuthority,
                    position: at(4, 0),
                },
                "missing identity LAW AWOS/analyzer order authority or OG at record 4 field 0",
            ),
            (
                Unidentified::AutolyserRun,
                concat!(
                    "Autolyser Rev18 has no immutable run identity; ",
                    "patient code is not trusted specimen identity",
                ),
            ),
            (
                Unidentified::PoctRepeated,
                "POCT repeated derived observation identity without a distinguishing sub-ID",
            ),
            (
                Unidentified::PoctEvidence,
                "POCT configured device/time/sequence identity evidence missing",
            ),
            (
                Unidentified::PoctSequence,
                "POCT service sequence may wrap; trusted run and revision evidence is required",
            ),
            (
                Unidentified::VidasLiteral,
                concat!(
                    "VIDAS Literal lacks trusted result/run identity ",
                    "and patient versus QC classification",
                ),
            ),
        ] {
            assert_eq!(reason.to_string(), expected);
            assert_eq!(Unidentified::from_stored(expected), Some(reason));
        }
        for text in [
            "",
            "missing identity order_id at record 3 field",
            "missing identity order_id at record 03 field 2",
            "missing identity other at record 3 field 2",
            "missing identity order_id at record 3 field 2 ",
            "unknown test mapping.",
            // ADR 0029 removed the test table, so this reason no longer exists.
            "unknown test mapping",
        ] {
            assert_eq!(Unidentified::from_stored(text), None, "{text}");
        }
    }

    // Owner report 2026-11-21: a result with no identity because of a finding
    // must not open a second Needs review item. Only the finding with the same
    // cause (and, for a missing value, the same place) restates the reason.
    #[test]
    fn a_reason_restates_only_the_finding_with_its_cause() {
        let at = |record, field| Position { record, field };
        let finding = |position, problem| Finding { position, problem };
        let order = IdentitySetting::OrderId;
        let missing = Unidentified::Missing {
            setting: order,
            position: at(3, 2),
        };
        let unknown = finding(at(4, 3), Problem::MissingTest);
        let absent = finding(at(3, 2), Problem::MissingIdentity { setting: order });
        assert!(missing.restates(&absent));
        assert!(!missing.restates(&unknown));
        assert!(!missing.restates(&finding(
            at(3, 3),
            Problem::MissingIdentity { setting: order }
        )));
        assert!(!missing.restates(&finding(
            at(3, 2),
            Problem::MissingIdentity {
                setting: IdentitySetting::SubId
            }
        )));
        for own in [
            Unidentified::AutolyserRun,
            Unidentified::PoctRepeated,
            Unidentified::PoctEvidence,
            Unidentified::PoctSequence,
            Unidentified::VidasLiteral,
        ] {
            assert!(!own.restates(&unknown), "{own}");
            assert!(!own.restates(&absent), "{own}");
        }
    }
}
