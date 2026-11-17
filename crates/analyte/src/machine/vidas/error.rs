//! Classifies VIDAS session refusals with shared link and custody causes.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    /// VIDAS active dialog has no deadline.
    #[error("VIDAS active dialog has no deadline")]
    ActiveDialogHasNoDeadline,
    /// VIDAS control suffix mismatch.
    #[error("VIDAS control suffix mismatch")]
    ControlSuffixMismatch,
    /// VIDAS transfer parked; reconcile before reconnect.
    #[error("VIDAS transfer parked; reconcile before reconnect")]
    TransferParkedReconcileBeforeReconnect,
    /// An outbound command could not represent the mapped order.
    #[error(transparent)]
    OrderMapping(#[from] analyte_mapping::OrderMappingError),
    /// Order reservation or completion could not be persisted.
    #[error(transparent)]
    Orders(#[from] crate::orders::OrderError),
    /// The VIDAS transfer could not advance its phase.
    #[error(transparent)]
    Dialog(#[from] analyte_dialog::vidas::Error),
    /// VIDAS record fields failed grammar validation.
    #[error(transparent)]
    Syntax(#[from] analyte_vidas::syntax::Error),
    /// The VIDAS packet failed framing or checksum validation.
    #[error(transparent)]
    Frame(#[from] analyte_framing::vidas::Error),
    /// Retains link, store and evidence causes from [`crate::machine::receive::error::Error`],
    /// uncertain custody and independent cleanup failures in their original order.
    #[error(transparent)]
    Refusal(#[from] crate::machine::custody::Refusal<Self>),
}

impl<C> From<C> for Error
where
    crate::machine::receive::error::Error: From<C>,
{
    fn from(cause: C) -> Self {
        Self::Refusal(crate::machine::custody::Refusal::Evidence(Box::new(
            crate::machine::receive::error::Error::from(cause),
        )))
    }
}
impl From<crate::machine::custody::StoreWaitError<Self>> for Error {
    fn from(cause: crate::machine::custody::StoreWaitError<Self>) -> Self {
        Self::Refusal(crate::machine::custody::Refusal::Custody(Box::new(cause)))
    }
}
impl crate::machine::custody::CleanupFailure for Error {
    fn with_effect(original: Self, effect: Self) -> Self {
        Self::Refusal(crate::machine::custody::Refusal::Cleanup(Box::new(
            crate::machine::custody::Cleanup { original, effect },
        )))
    }
}
