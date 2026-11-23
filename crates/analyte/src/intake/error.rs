//! Distinguishes intake storage, evidence construction and input failures while retaining causes.
/// Operational failures while retaining or interpreting an already received message.
#[derive(Debug, thiserror::Error)]
pub enum IntakeError {
    /// The host could not represent the storage request deadline.
    #[error(transparent)]
    Deadline(#[from] crate::DeadlineError),
    /// Protocol-specific intake failed before durable acceptance.
    #[error(transparent)]
    Protocol(#[from] crate::WorkPolicyError),
    /// Durable custody or reconciliation failed; do not acknowledge success.
    #[error(transparent)]
    Store(#[from] analyte_store::IntakeError),
    /// The retained receipt needed by the host does not exist.
    #[error("receipt does not exist")]
    UnknownReceipt,
    /// Prepared mapping could not be committed.
    #[error(transparent)]
    Commit(#[from] super::CommitError),
    /// Protected receipt lookup failed.
    #[error(transparent)]
    Receipt(#[from] analyte_store::OperatorError),
    /// Interpretation reconciliation failed.
    #[error(transparent)]
    Reconciliation(#[from] analyte_store::ReconciliationError),
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
    /// A record-only machine reads and delivers nothing; its receipts are
    /// kept as recorded and are never interpreted (ADR 0034).
    #[error("this machine records only: nothing is read or delivered for it")]
    RecordOnly,
    /// The receipt was kept by a record-only connection; it is evidence of the
    /// conversation, not a message to read again (ADR 0034).
    #[error(
        "this message was kept by a record-only connection; replay its capture instead of reprocessing it"
    )]
    RecordedOnly,
}

impl From<analyte_store::WorkerError> for IntakeError {
    fn from(cause: analyte_store::WorkerError) -> Self {
        Self::Store(analyte_store::IntakeError::from(cause))
    }
}
