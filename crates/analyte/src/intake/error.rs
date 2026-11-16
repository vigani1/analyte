//! Distinguishes intake storage, evidence construction and input failures while retaining causes.
/// Operational failures while retaining or interpreting an already received message.
#[derive(Debug, thiserror::Error)]
pub enum IntakeError {
    /// Required retained LAW context is unavailable or exceeds its bound.
    #[error("LAW header")]
    LawHeader,
    /// Required retained LAW context is unavailable or exceeds its bound.
    #[error("LAW component delimiter")]
    LawComponentDelimiter,
    /// Required retained LAW context is unavailable or exceeds its bound.
    #[error("LAW OBR occurrence")]
    LawObrOccurrence,
    /// Required retained LAW context is unavailable or exceeds its bound.
    #[error("retained LAW item")]
    RetainedLawItem,
    /// Retained work comparison failed operationally.
    #[error(transparent)]
    Validation(#[from] crate::orders::law::validation::ValidationError),
    /// Durable custody or reconciliation failed; do not acknowledge success.
    #[error(transparent)]
    Store(#[from] analyte_store::Error),
    /// Bounded model text refused source evidence.
    #[error(transparent)]
    Text(#[from] analyte_model::TextError),
    /// Evidence formatting failed before submission.
    #[error(transparent)]
    Format(#[from] std::fmt::Error),
    /// Original source encoding is invalid.
    #[error(transparent)]
    Encoding(#[from] std::str::Utf8Error),
    /// Protected structured evidence could not be read or encoded.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    /// Departure evidence could not be constructed.
    #[error(transparent)]
    Evidence(#[from] crate::machine::interpretation::departure_evidence::EvidenceError),
    /// Reprocessing cannot cross machine ownership.
    #[error("receipt belongs to a different machine")]
    WrongMachine,
}
