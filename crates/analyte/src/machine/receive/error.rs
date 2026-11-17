//! Classifies shared durable-store, interpretation and evidence failures.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    /// A bounded synthetic gate stopped or could not resume the connection.
    #[error(transparent)]
    TestHook(#[from] crate::runtime::test_hooks::Error),
    /// The host could not represent the storage request deadline.
    #[error(transparent)]
    Deadline(#[from] crate::DeadlineError),
    /// A durable store operation failed before its promised completion.
    #[error(transparent)]
    Store(#[from] analyte_store::IntakeError),
    /// Order or poll lifecycle storage failed.
    #[error(transparent)]
    Order(#[from] analyte_store::OrderError),
    /// Protected evidence lookup failed.
    #[error(transparent)]
    Evidence(#[from] analyte_store::OperatorError),
    /// Stored interpretations could not be reconciled.
    #[error(transparent)]
    Reconciliation(#[from] analyte_store::ReconciliationError),
    /// Receipt retention or interpretation failed at the intake boundary.
    #[error(transparent)]
    Intake(Box<crate::IntakeError>),
    /// Retained receipt context could not be loaded for interpretation.
    #[error(transparent)]
    Reading(Box<crate::machine::interpretation::ReadError>),
    /// Evidence text could not satisfy its model bounds.
    #[error(transparent)]
    Text(#[from] analyte_model::TextError),
    /// An evidence list exceeded its model limit.
    #[error(transparent)]
    List(#[from] analyte_model::ListError),
    /// The installed protocol configuration could not be used.
    #[error(transparent)]
    Config(#[from] analyte_config::Error),
    /// A protocol length or offset could not fit its target integer.
    #[error(transparent)]
    Integer(#[from] std::num::TryFromIntError),
    /// Retained structured evidence could not be serialized or read.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    /// A protocol timestamp could not be represented.
    #[error(transparent)]
    Timestamp(#[from] jiff::Error),
    /// A retained receipt failed framing, syntax or mapping.
    #[error(transparent)]
    MappingRead(Box<crate::reading::Error>),
    /// A bounded vendor write failed or has uncertain completion.
    #[error(transparent)]
    Write(#[from] crate::machine::receive::custody::WriteError),
    /// The complete intake custody operation failed or exceeded its budget.
    #[error(transparent)]
    IntakeWait(#[from] crate::machine::receive::custody::StoreWaitError<crate::IntakeError>),
    /// A durable store request failed or exceeded its custody budget.
    #[error(transparent)]
    StoreWait(#[from] crate::machine::receive::custody::StoreWaitError<analyte_store::IntakeError>),
}

impl From<crate::IntakeError> for Error {
    fn from(error: crate::IntakeError) -> Self {
        Self::Intake(Box::new(error))
    }
}

impl From<crate::reading::Error> for Error {
    fn from(error: crate::reading::Error) -> Self {
        Self::MappingRead(Box::new(error))
    }
}

impl From<crate::machine::interpretation::ReadError> for Error {
    fn from(error: crate::machine::interpretation::ReadError) -> Self {
        Self::Reading(Box::new(error))
    }
}

crate::runtime::service::conversions::causes!(Error; worker analyte_store::IntakeError);
