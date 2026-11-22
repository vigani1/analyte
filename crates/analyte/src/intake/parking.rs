//! Keeps parked interpretation reasons separate from operational failures.
use analyte_model::{Reason, TextError};

/// A semantic refusal retained as evidence; it is not an operational error.
#[derive(Debug)]
pub(crate) enum ParkReason {
    /// Original parser, protocol or vendor diagnostic, bounded at the store boundary.
    Diagnostic(String),
    /// A validated departure ledger could not establish complete evidence.
    IncompleteDepartures(Reason),
    /// Returned work contradicts the retained immutable command.
    WorkConflict(Reason),
}
impl ParkReason {
    pub(crate) fn into_reason(self) -> Result<Reason, TextError> {
        match self {
            Self::Diagnostic(text) => Reason::new(&text),
            Self::IncompleteDepartures(reason) | Self::WorkConflict(reason) => Ok(reason),
        }
    }
}

/// Mapping output or a deliberately parked interpretation; neither implies custody failure.
pub(crate) enum Mapping {
    Mapped(analyte_mapping::Output),
    Parked(ParkReason),
}
impl Mapping {
    pub(crate) fn output(&self) -> Option<&analyte_mapping::Output> {
        match self {
            Self::Mapped(output) => Some(output),
            Self::Parked(_) => None,
        }
    }
}
impl<E: std::fmt::Display> From<Result<analyte_mapping::Output, E>> for Mapping {
    fn from(result: Result<analyte_mapping::Output, E>) -> Self {
        match result {
            Ok(output) => Self::Mapped(output),
            // Diagnostics may contain source tokens. They belong only in protected evidence.
            Err(error) => Self::Parked(ParkReason::Diagnostic(error.to_string())),
        }
    }
}

#[test]
fn parked_reason_text_is_preserved_without_an_error_roundtrip() -> Result<(), TextError> {
    assert_eq!(
        ParkReason::Diagnostic("source <0.1; MissingTest".into())
            .into_reason()?
            .as_str(),
        "source <0.1; MissingTest"
    );
    assert_eq!(
        ParkReason::IncompleteDepartures(Reason::new("departure-evidence failure: exact detail")?)
            .into_reason()?
            .as_str(),
        "departure-evidence failure: exact detail"
    );
    assert!(ParkReason::Diagnostic(String::new()).into_reason().is_err());
    Ok(())
}
