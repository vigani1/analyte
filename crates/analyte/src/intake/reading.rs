//! Classifies committed revisions without changing protocol outcomes.
use super::{Error, InterpretationOutcome};
use analyte_store::RevisionOutcome;

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
