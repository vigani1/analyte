//! Writes stable revision and LAW disposition evidence independently of Debug output.
use super::{
    RevisionOutcome,
    law::{Conflict, Disposition},
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
impl fmt::Display for Conflict {
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
impl fmt::Display for Disposition {
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
    assert_eq!(Disposition::Accepted.to_string(), "Accepted");
    for (value, expected) in [
        (Conflict::UnknownWork, "UnknownWork"),
        (Conflict::Context, "Context"),
        (Conflict::Progression, "Progression"),
        (Conflict::Identity, "Identity"),
        (Conflict::Processing, "Processing"),
    ] {
        assert_eq!(value.to_string(), expected);
        assert_eq!(
            Disposition::Rejected(super::law::Rejection {
                occurrence: 2,
                reason: value
            })
            .to_string(),
            format!("Rejected(LawRejection {{ occurrence: 2, reason: {expected} }})")
        );
    }
}

// ADR 0016
#[test]
fn scoped_disposition_keeps_stored_json() -> Result<(), serde_json::Error> {
    let disposition = Disposition::Rejected(super::law::Rejection {
        occurrence: 2,
        reason: Conflict::Context,
    });
    let original = r#"{"Rejected":{"occurrence":2,"reason":"Context"}}"#;
    assert_eq!(serde_json::to_string(&disposition)?, original);
    let restored: Disposition = serde_json::from_str(original)?;
    assert_eq!(restored.to_string(), disposition.to_string());
    Ok(())
}
