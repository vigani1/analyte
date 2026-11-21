//! Turns mapper output into a stored reading with corrections and findings, and classifies the commit outcome.
use crate::intake::{Error, MappedReading, Mapping};
use analyte_mapping::{Position, Problem, ReasonPiece, Unidentified};
use analyte_store::Reading;

/// The finding stored when a final status taken from the machine's documented
/// behaviour cannot be carried by the lab app contract in use.
pub(crate) const DERIVED_STATUS_REASON: &str =
    "departure-derived status requires a contract version that carries departure evidence";

/// A departure conflict is stored as `departure-conflict at {position}: …`.
const DEPARTURE_OPEN: &str = "departure-conflict at ";
const DEPARTURE_CLOSE: &str = ": present non-final status contradicts documented final-only scope";

fn departure_conflict(position: Position) -> String {
    format!("{DEPARTURE_OPEN}{position}{DEPARTURE_CLOSE}")
}

/// Reads back where a stored departure conflict points, the exact reverse of
/// how it was written; any other text gives `None`.
pub(crate) fn departure_conflict_at(stored: &str) -> Option<Position> {
    match Position::split_stored(stored).as_slice() {
        [
            ReasonPiece::Text(DEPARTURE_OPEN),
            ReasonPiece::Position(position),
            ReasonPiece::Text(DEPARTURE_CLOSE),
        ] => Some(*position),
        _ => None,
    }
}

pub(crate) fn map_reading(mapping: Mapping) -> Result<MappedReading, Error> {
    let mut accepted = false;
    let (reading, corrections, findings) = match mapping {
        Mapping::Mapped(output) => {
            accepted = !output.requires_parking();
            let mut findings = output
                .findings
                .iter()
                .map(|f| match f.problem {
                    Problem::DerivedStatusContract => DERIVED_STATUS_REASON.to_owned(),
                    Problem::DepartureConflict => departure_conflict(f.position),
                    Problem::UnqualifiedHl7Report
                    | Problem::ValueKindMismatch { .. }
                    | Problem::ComponentParts
                    | Problem::MissingSample
                    | Problem::UnknownTest
                    | Problem::MissingIdentity { .. }
                    | Problem::UnsupportedValue
                    | Problem::UnsupportedFormatting
                    | Problem::UnsupportedEscape
                    | Problem::UnknownProcessing
                    | Problem::ConflictingClassification
                    | Problem::NoObservations
                    | Problem::UnsupportedLawStatusOnly
                    | Problem::InvalidTime => f.to_string(),
                })
                .collect::<Vec<_>>();
            // One cause is one item: a result's missing identity is kept as a
            // finding only when no finding already names its cause, and once
            // per message (every result of a VIDAS Literal gives the same one).
            // The result keeps its own reason in the stored reading either way.
            for result in &output.results {
                if let analyte_model::IdentityStatus::Unresolved(reason) = &result.content.identity
                {
                    let restated = Unidentified::from_stored(reason.as_str()).is_some_and(|why| {
                        output.findings.iter().any(|finding| why.restates(finding))
                    });
                    if !restated && !findings.iter().any(|kept| kept == reason.as_str()) {
                        findings.push(reason.as_str().to_owned());
                    }
                }
            }
            let corrections = output
                .results
                .iter()
                .map(|r| r.corrections.as_slice().iter().any(|c| *c))
                .collect::<Vec<_>>();
            (
                Reading::Results(output.results.into_iter().map(|r| r.content).collect()),
                corrections,
                findings,
            )
        }
        // Parser diagnostics can contain source tokens; keep them only in protected evidence.
        Mapping::Parked(reason) => (
            Reading::Failed(reason.into_reason()?),
            Vec::new(),
            Vec::new(),
        ),
    };
    Ok(MappedReading {
        accepted,
        reading,
        corrections,
        findings,
    })
}
