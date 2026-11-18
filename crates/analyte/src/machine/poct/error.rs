//! Classifies POCT session refusals with shared link and custody causes.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    /// POCT backpressure violated.
    #[error("POCT backpressure violated")]
    BackpressureViolated,
    /// POCT message limit exceeded.
    #[error("POCT message limit exceeded")]
    MessageLimitExceeded,
    /// A POCT header or value failed its declared data type.
    #[error(transparent)]
    Data(#[from] analyte_poct::DataError),
    /// The POCT conversation rejected a message or deadline.
    #[error(transparent)]
    Dialog(#[from] analyte_dialog::poct::Error),
    /// The incoming POCT document failed message validation.
    #[error(transparent)]
    Syntax(#[from] analyte_poct::MessageError),
    /// Retains link, store and evidence causes from [`crate::machine::receive::error::Error`],
    /// uncertain custody and independent cleanup failures in their original order.
    #[error(transparent)]
    Refusal(#[from] crate::machine::receive::custody::Refusal<Self>),
}

crate::machine::receive::custody::refusal_conversions!(Error);
