//! Distinguishes intake storage, evidence construction and input failures while retaining causes.
/// Operational failures while retaining or interpreting an already received message.
#[derive(Debug, thiserror::Error)]
pub enum IntakeError {
    /// Protocol-specific intake failed before durable acceptance.
    #[error(transparent)]
    Protocol(#[from] crate::machine::law::custody::Error),
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

crate::runtime::service::conversions::causes!(IntakeError; store);
