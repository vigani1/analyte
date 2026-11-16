//! Writes stable revision and LAW disposition evidence independently of Debug output.
use super::{
    RevisionOutcome,
    law::{LawConflict, LawDisposition},
};
use std::fmt;
impl fmt::Display for RevisionOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::New(value) => write!(f, "New(Revision({}))", value.0),
            Self::Retransmission(value) => write!(f, "Retransmission(Revision({}))", value.0),
            Self::Correction(value) => write!(f, "Correction(Revision({}))", value.0),
            Self::Conflict => f.write_str("Conflict"),
            Self::Unresolved => f.write_str("Unresolved"),
        }
    }
}
impl fmt::Display for LawConflict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::UnknownWork => "UnknownWork",
            Self::Context => "Context",
            Self::Progression => "Progression",
            Self::Identity => "Identity",
            Self::Processing => "Processing",
        })
    }
}
impl fmt::Display for LawDisposition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Accepted => f.write_str("Accepted"),
            Self::Rejected(reason) => write!(
                f,
                "Rejected(LawRejection {{ occurrence: {}, reason: {} }})",
                reason.occurrence, reason.reason
            ),
        }
    }
}
#[test]
fn revision_and_law_spellings_are_compatible() {
    use analyte_model::Revision;
    for (value, expected) in [
        (RevisionOutcome::New(Revision(0)), "New(Revision(0))"),
        (
            RevisionOutcome::Retransmission(Revision(1)),
            "Retransmission(Revision(1))",
        ),
        (
            RevisionOutcome::Correction(Revision(2)),
            "Correction(Revision(2))",
        ),
        (RevisionOutcome::Conflict, "Conflict"),
        (RevisionOutcome::Unresolved, "Unresolved"),
    ] {
        assert_eq!(value.to_string(), expected);
    }
    assert_eq!(LawDisposition::Accepted.to_string(), "Accepted");
    for (value, expected) in [
        (LawConflict::UnknownWork, "UnknownWork"),
        (LawConflict::Context, "Context"),
        (LawConflict::Progression, "Progression"),
        (LawConflict::Identity, "Identity"),
        (LawConflict::Processing, "Processing"),
    ] {
        assert_eq!(value.to_string(), expected);
        assert_eq!(
            LawDisposition::Rejected(super::law::LawRejection {
                occurrence: 2,
                reason: value
            })
            .to_string(),
            format!("Rejected(LawRejection {{ occurrence: 2, reason: {expected} }})")
        );
    }
}
