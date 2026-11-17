//! Classifies AUTOLYSER session refusals with shared link and custody causes.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    /// Autolyser poll confirmation did not reconcile custody.
    #[error("Autolyser poll confirmation did not reconcile custody")]
    AutolyserPollConfirmationDidNotReconcileCustody,
    /// Autolyser previous poll requires operator reconciliation before R can advance.
    #[error("Autolyser previous poll requires operator reconciliation before R can advance")]
    AutolyserPreviousPollRequiresOperatorReconciliationBeforeRCanAdvance,
    /// Autolyser received another stack's order.
    #[error("Autolyser received another stack's order")]
    AutolyserReceivedAnotherStackSOrder,
    /// Autolyser report receipt missing.
    #[error("Autolyser report receipt missing")]
    AutolyserReportReceiptMissing,
    /// Unexpected Autolyser insertion outcome.
    #[error("unexpected Autolyser insertion outcome")]
    UnexpectedAutolyserInsertionOutcome,
    /// An outbound command could not represent the mapped order.
    #[error(transparent)]
    OrderMapping(#[from] analyte_mapping::OrderMappingError),
    /// Order reservation or completion could not be persisted.
    #[error(transparent)]
    Orders(#[from] crate::orders::OrderError),
    /// The Autolyser poll or insertion dialog refused an event.
    #[error(transparent)]
    AutolyserDialog(#[from] analyte_dialog::autolyser::Error),
    /// Autolyser report records failed grammar validation.
    #[error(transparent)]
    Autolyser(#[from] analyte_autolyser::syntax::Error),
    /// A bounded synthetic gate stopped or could not resume the connection.
    #[cfg(feature = "test-hooks")]
    #[error(transparent)]
    TestHook(crate::runtime::test_hooks::Error),
    /// The Autolyser response envelope failed validation.
    #[error(transparent)]
    AutolyserFrame(#[from] analyte_framing::autolyser::Error),
    /// Retains uncertain custody or independent cleanup failures in their original order.
    #[error(transparent)]
    Refusal(#[from] crate::machine::custody::Refusal<Self>),
}

crate::machine::receive::error::session_boundary!(Error);
