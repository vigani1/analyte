//! Classifies AUTOLYSER session refusals with shared link and custody causes.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    /// Autolyser poll confirmation did not reconcile custody.
    #[error("Autolyser poll confirmation did not reconcile custody")]
    PollCustodyUnreconciled,
    /// Autolyser previous poll requires operator reconciliation before R can advance.
    #[error("Autolyser previous poll requires operator reconciliation before R can advance")]
    PollUnreconciled,
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
    /// The Autolyser response envelope failed validation.
    #[error(transparent)]
    Frame(#[from] analyte_framing::autolyser::Error),
    /// Retains link, store and evidence causes from [`crate::machine::receive::error::Error`],
    /// uncertain custody and independent cleanup failures in their original order.
    #[error(transparent)]
    Refusal(#[from] crate::machine::receive::custody::Refusal<Self>),
}

crate::machine::receive::custody::refusal_conversions!(Error);
