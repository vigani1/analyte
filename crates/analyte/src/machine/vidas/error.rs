//! Classifies VIDAS session refusals with shared link and custody causes.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    /// VIDAS active dialog has no deadline.
    #[error("VIDAS active dialog has no deadline")]
    ActiveDialogHasNoDeadline,
    /// VIDAS control suffix mismatch.
    #[error("VIDAS control suffix mismatch")]
    ControlSuffixMismatch,
    /// VIDAS mapped request missing.
    #[error("VIDAS mapped request missing")]
    MappedRequestMissing,
    /// VIDAS order poll deadline overflow.
    #[error("VIDAS order poll deadline overflow")]
    OrderPollDeadlineOverflow,
    /// VIDAS received another stack's order.
    #[error("VIDAS received another stack's order")]
    ForeignStackOrder,
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
    /// Retains uncertain custody or independent cleanup failures in their original order.
    #[error(transparent)]
    Refusal(#[from] crate::machine::custody::Refusal<Self>),
}

crate::machine::receive::error::session_boundary!(Error);
