//! Classifies POCT session refusals with shared link and custody causes.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    /// POCT backpressure violated.
    #[error("POCT backpressure violated")]
    BackpressureViolated,
    /// POCT message limit exceeded.
    #[error("POCT message limit exceeded")]
    MessageLimitExceeded,
    /// An outbound command could not represent the mapped order.
    #[error(transparent)]
    OrderMapping(#[from] analyte_mapping::OrderMappingError),
    /// A POCT header or value failed its declared data type.
    #[error(transparent)]
    Data(#[from] analyte_poct::DataError),
    /// Order reservation or completion could not be persisted.
    #[error(transparent)]
    Orders(#[from] crate::orders::OrderError),
    /// The POCT conversation rejected a message or deadline.
    #[error(transparent)]
    Dialog(#[from] analyte_dialog::poct::Error),
    /// The incoming POCT document failed message validation.
    #[error(transparent)]
    Syntax(#[from] analyte_poct::MessageError),
    /// Retains uncertain custody or independent cleanup failures in their original order.
    #[error(transparent)]
    Refusal(#[from] crate::machine::custody::Refusal<Self>),
}

crate::machine::receive::error::session_boundary!(Error);
