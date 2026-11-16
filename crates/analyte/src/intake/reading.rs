//! Turns mapper output into a stored reading with corrections and findings, and classifies the commit outcome.
use super::{Error, InterpretationOutcome, MappedReading};
use analyte_store::{Reading, RevisionOutcome};

pub(super) fn map_reading(
    mapping: Result<analyte_mapping::Output, Error>,
) -> Result<MappedReading, Error> {
    let mut accepted = false;
    let (reading, corrections, findings) = match mapping {
        Ok(output) => {
            accepted = !output.requires_parking();
            let mut findings = output
                .findings
                .iter()
                .map(|f| match f.problem {
                    analyte_mapping::Problem::DepartureDerivedStatusRequiresContract =>
                        "departure-derived status requires a contract version that carries departure evidence".to_owned(),
                    analyte_mapping::Problem::DepartureConflict => format!("departure-conflict at {}: present non-final status contradicts documented final-only scope", f.position),
                    analyte_mapping::Problem::UnqualifiedHl7Report
                    | analyte_mapping::Problem::ValueKindMismatch { .. }
                    | analyte_mapping::Problem::ComponentParts | analyte_mapping::Problem::MissingSample
                    | analyte_mapping::Problem::UnknownTest | analyte_mapping::Problem::MissingIdentity { .. }
                    | analyte_mapping::Problem::UnsupportedValue | analyte_mapping::Problem::UnsupportedFormatting
                    | analyte_mapping::Problem::UnsupportedEscape | analyte_mapping::Problem::UnknownProcessing
                    | analyte_mapping::Problem::ConflictingClassification | analyte_mapping::Problem::NoObservations
                    | analyte_mapping::Problem::UnsupportedLawStatusOnly | analyte_mapping::Problem::InvalidTime => format!("{}: {}", f.position, f.problem),
                })
                .collect::<Vec<_>>();
            for result in &output.results {
                if let analyte_model::IdentityStatus::Unresolved(reason) = &result.content.identity
                {
                    findings.push(reason.as_str().to_owned());
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
        Err(error) => (
            Reading::Failed(analyte_model::Reason::new(&error.to_string())?),
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

pub(super) fn mapping_outcome(
    result: analyte_store::InterpretationCommit,
    successful: InterpretationOutcome,
) -> Result<(InterpretationOutcome, analyte_store::InterpretationCommit), Error> {
    let accepted = matches!(successful, InterpretationOutcome::Accepted)
        && !result
            .outcomes
            .iter()
            .any(|o| matches!(o, RevisionOutcome::Conflict | RevisionOutcome::Unresolved));
    Ok((
        if accepted {
            InterpretationOutcome::Accepted
        } else {
            match successful {
                InterpretationOutcome::Conflict => InterpretationOutcome::Conflict,
                InterpretationOutcome::WorkConflict(location) => {
                    InterpretationOutcome::WorkConflict(location)
                }
                InterpretationOutcome::Accepted | InterpretationOutcome::Failed => {
                    InterpretationOutcome::Failed
                }
            }
        },
        result,
    ))
}
