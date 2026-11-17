//! Classifies AUTOLYSER session refusals with shared link and custody causes.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    /// Autolyser poll confirmation did not reconcile custody.
    #[error("Autolyser poll confirmation did not reconcile custody")]
    PollConfirmationDidNotReconcileCustody,
    /// Autolyser previous poll requires operator reconciliation before R can advance.
    #[error("Autolyser previous poll requires operator reconciliation before R can advance")]
    PreviousPollRequiresOperatorReconciliationBeforeRCanAdvance,
    /// Autolyser report receipt missing.
    #[error("Autolyser report receipt missing")]
    ReportReceiptMissing,
    /// An outbound command could not represent the mapped order.
    #[error(transparent)]
    OrderMapping(#[from] analyte_mapping::OrderMappingError),
    /// Order reservation or completion could not be persisted.
    #[error(transparent)]
    Orders(#[from] crate::orders::OrderError),
    /// The Autolyser poll or insertion dialog refused an event.
    #[error(transparent)]
    Dialog(#[from] analyte_dialog::autolyser::Error),
    /// Autolyser report records failed grammar validation.
    #[error(transparent)]
    Syntax(#[from] analyte_autolyser::syntax::Error),
    /// A bounded synthetic gate stopped or could not resume the connection.
    #[error(transparent)]
    TestHook(crate::runtime::test_hooks::Error),
    /// The Autolyser response envelope failed validation.
    #[error(transparent)]
    Frame(#[from] analyte_framing::autolyser::Error),
    /// Retains uncertain custody or independent cleanup failures in their original order.
    #[error(transparent)]
    Refusal(#[from] crate::machine::custody::Refusal<Self>),
}

impl From<crate::machine::link::error::Error> for Error {
    fn from(error: crate::machine::link::error::Error) -> Self {
        Self::Refusal(crate::machine::custody::Refusal::Link(Box::new(error)))
    }
}
impl From<crate::machine::receive::error::Error> for Error {
    fn from(error: crate::machine::receive::error::Error) -> Self {
        Self::Refusal(crate::machine::custody::Refusal::Evidence(Box::new(error)))
    }
}
impl From<crate::machine::custody::StoreWaitError<Self>> for Error {
    fn from(error: crate::machine::custody::StoreWaitError<Self>) -> Self {
        Self::Refusal(crate::machine::custody::Refusal::Custody(Box::new(error)))
    }
}
impl crate::machine::custody::CleanupFailure for Error {
    fn with_effect(original: Self, effect: Self) -> Self {
        Self::Refusal(crate::machine::custody::Refusal::Cleanup(Box::new(
            crate::machine::custody::Cleanup { original, effect },
        )))
    }
}

crate::runtime::service::conversions::causes!(Error;
    crate::machine::link::error::Error => [
        std::io::Error,
        analyte_transport::Error,
        std::net::AddrParseError,
        analyte_transport::folder::WatchError,
    ],
    crate::machine::receive::error::Error => [
        analyte_store::Error,
        crate::IntakeError,
        crate::machine::interpretation::ReadError,
        analyte_model::TextError,
        analyte_model::ListError,
        analyte_config::Error,
        std::num::TryFromIntError,
        serde_json::Error,
        jiff::Error,
        crate::reading::Error,
        crate::machine::custody::WriteError,
        crate::machine::custody::StoreWaitError<crate::IntakeError>,
        crate::machine::custody::StoreWaitError<analyte_store::Error>,
    ],
    analyte_store::Error => [
        analyte_store::WorkerError,
        analyte_store::WorkerStartError,
        analyte_store::IntakeError,
        analyte_store::DeliveryError,
        analyte_store::OrderError,
        analyte_store::MaintenanceError,
        analyte_store::OperatorError,
        analyte_store::ReconciliationError,
        analyte_store::StorageError,
    ],
);
