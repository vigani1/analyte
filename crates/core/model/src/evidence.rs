//! Writes stable lifecycle and identity evidence independently of derived Debug output.
use crate::{AwosExecution, DepartureMeaning, NamespacedId, OrderState, ResultStatus};
#[cfg(test)]
use crate::{Identifier, TextError};
use std::fmt::{self, Write};

impl fmt::Display for OrderState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Requested => "Requested",
            Self::Sending => "Sending",
            Self::Sent => "Sent",
            Self::Accepted => "Accepted",
            Self::Done => "Done",
            Self::Rejected => "Rejected",
            Self::CancelRequested => "CancelRequested",
            Self::CancelConfirmed => "CancelConfirmed",
            Self::Uncertain => "Uncertain",
        })
    }
}

#[test]
fn orderstate_spellings_are_compatible() {
    for (value, expected) in [
        (OrderState::Requested, "Requested"),
        (OrderState::Sending, "Sending"),
        (OrderState::Sent, "Sent"),
        (OrderState::Accepted, "Accepted"),
        (OrderState::Done, "Done"),
        (OrderState::Rejected, "Rejected"),
        (OrderState::CancelRequested, "CancelRequested"),
        (OrderState::CancelConfirmed, "CancelConfirmed"),
        (OrderState::Uncertain, "Uncertain"),
    ] {
        assert_eq!(value.to_string(), expected);
    }
}

impl fmt::Display for AwosExecution {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Scheduled => "Scheduled",
            Self::InProgress => "InProgress",
            Self::AnalyzerCancelled => "AnalyzerCancelled",
        })
    }
}

#[test]
fn awosexecution_spellings_are_compatible() {
    for (value, expected) in [
        (AwosExecution::Scheduled, "Scheduled"),
        (AwosExecution::InProgress, "InProgress"),
        (AwosExecution::AnalyzerCancelled, "AnalyzerCancelled"),
    ] {
        assert_eq!(value.to_string(), expected);
    }
}

impl fmt::Display for DepartureMeaning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::FinalReportFromDocumentation => "FinalReportFromDocumentation",
            Self::FinalObservationFromDocumentation => "FinalObservationFromDocumentation",
            Self::PackedRecords => "PackedRecords",
            Self::WireCompatibility => "WireCompatibility",
        })
    }
}

#[test]
fn departuremeaning_spellings_are_compatible() {
    for (value, expected) in [
        (
            DepartureMeaning::FinalReportFromDocumentation,
            "FinalReportFromDocumentation",
        ),
        (
            DepartureMeaning::FinalObservationFromDocumentation,
            "FinalObservationFromDocumentation",
        ),
        (DepartureMeaning::PackedRecords, "PackedRecords"),
        (DepartureMeaning::WireCompatibility, "WireCompatibility"),
    ] {
        assert_eq!(value.to_string(), expected);
    }
}

impl fmt::Display for ResultStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Pending => "Pending",
            Self::Preliminary => "Preliminary",
            Self::Final => "Final",
            Self::Corrected => "Corrected",
            Self::Withdrawn => "Withdrawn",
            Self::NotPerformed => "NotPerformed",
            Self::Unknown => "Unknown",
        })
    }
}

#[test]
fn resultstatus_spellings_are_compatible() {
    for (value, expected) in [
        (ResultStatus::Pending, "Pending"),
        (ResultStatus::Preliminary, "Preliminary"),
        (ResultStatus::Final, "Final"),
        (ResultStatus::Corrected, "Corrected"),
        (ResultStatus::Withdrawn, "Withdrawn"),
        (ResultStatus::NotPerformed, "NotPerformed"),
        (ResultStatus::Unknown, "Unknown"),
    ] {
        assert_eq!(value.to_string(), expected);
    }
}

impl fmt::Display for NamespacedId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("NamespacedId { namespace: Identifier(")?;
        quoted(f, self.namespace.as_str())?;
        f.write_str("), value: Identifier(")?;
        quoted(f, self.value.as_str())?;
        f.write_str(") }")
    }
}
fn quoted(f: &mut fmt::Formatter<'_>, value: &str) -> fmt::Result {
    f.write_char('"')?;
    for character in value.chars() {
        if character == '\'' {
            f.write_char(character)?;
        } else {
            for escaped in character.escape_debug() {
                f.write_char(escaped)?;
            }
        }
    }
    f.write_char('"')
}
#[test]
fn namespaced_id_spelling_is_compatible() -> Result<(), TextError> {
    let value = NamespacedId {
        namespace: Identifier::new("local")?,
        value: Identifier::new("a'\"?")?,
    };
    assert_eq!(
        value.to_string(),
        "NamespacedId { namespace: Identifier(\"local\"), value: Identifier(\"a'\\\"?\") }"
    );
    Ok(())
}
