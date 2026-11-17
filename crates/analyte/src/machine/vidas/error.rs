//! Classifies VIDAS session refusals with shared link and custody causes.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    /// VIDAS active dialog has no deadline.
    #[error("VIDAS active dialog has no deadline")]
    VidasActiveDialogHasNoDeadline,
    /// VIDAS control suffix mismatch.
    #[error("VIDAS control suffix mismatch")]
    VidasControlSuffixMismatch,
    /// VIDAS mapped request missing.
    #[error("VIDAS mapped request missing")]
    VidasMappedRequestMissing,
    /// VIDAS order poll deadline overflow.
    #[error("VIDAS order poll deadline overflow")]
    VidasOrderPollDeadlineOverflow,
    /// VIDAS received another stack's order.
    #[error("VIDAS received another stack's order")]
    VidasReceivedAnotherStackSOrder,
    /// VIDAS transfer parked; reconcile before reconnect.
    #[error("VIDAS transfer parked; reconcile before reconnect")]
    VidasTransferParkedReconcileBeforeReconnect,
    /// An outbound command could not represent the mapped order.
    #[error(transparent)]
    OrderMapping(#[from] analyte_mapping::OrderMappingError),
    /// Order reservation or completion could not be persisted.
    #[error(transparent)]
    Orders(#[from] crate::orders::OrderError),
    /// The VIDAS transfer could not advance its phase.
    #[error(transparent)]
    VidasDialog(#[from] analyte_dialog::vidas::Error),
    /// VIDAS record fields failed grammar validation.
    #[error(transparent)]
    Vidas(#[from] analyte_vidas::syntax::Error),
    /// The VIDAS packet failed framing or checksum validation.
    #[error(transparent)]
    VidasFrame(#[from] analyte_framing::vidas::Error),
    /// The owned transport or journaled input failed.
    #[error(transparent)]
    Link(#[from] crate::machine::link::error::Error),
    /// Durable storage or retained evidence could not complete.
    #[error(transparent)]
    Evidence(#[from] crate::machine::receive::error::Error),
    /// Retains both the triggering failure and an independent cleanup failure.
    #[error("{original}; dialog effect failed: {effect}")]
    Cleanup {
        #[source]
        original: Box<Self>,
        effect: Box<Self>,
    },
    /// The bounded session custody operation failed; its original cause is retained.
    #[error(transparent)]
    Custody(Box<crate::machine::custody::StoreWaitError<Self>>),
}
impl From<crate::machine::custody::StoreWaitError<Self>> for Error {
    fn from(error: crate::machine::custody::StoreWaitError<Self>) -> Self {
        Self::Custody(Box::new(error))
    }
}
impl crate::machine::custody::CleanupFailure for Error {
    fn with_effect(original: Self, effect: Self) -> Self {
        Self::Cleanup {
            original: Box::new(original),
            effect: Box::new(effect),
        }
    }
}
impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::Link(error.into())
    }
}
impl From<analyte_transport::Error> for Error {
    fn from(error: analyte_transport::Error) -> Self {
        Self::Link(error.into())
    }
}
impl From<std::net::AddrParseError> for Error {
    fn from(error: std::net::AddrParseError) -> Self {
        Self::Link(error.into())
    }
}
impl From<analyte_transport::folder::WatchError> for Error {
    fn from(error: analyte_transport::folder::WatchError) -> Self {
        Self::Link(error.into())
    }
}
impl From<analyte_store::Error> for Error {
    fn from(error: analyte_store::Error) -> Self {
        Self::Evidence(error.into())
    }
}
impl From<crate::IntakeError> for Error {
    fn from(error: crate::IntakeError) -> Self {
        Self::Evidence(error.into())
    }
}
impl From<crate::machine::interpretation::ReadError> for Error {
    fn from(error: crate::machine::interpretation::ReadError) -> Self {
        Self::Evidence(error.into())
    }
}
impl From<analyte_model::TextError> for Error {
    fn from(error: analyte_model::TextError) -> Self {
        Self::Evidence(error.into())
    }
}
impl From<analyte_model::ListError> for Error {
    fn from(error: analyte_model::ListError) -> Self {
        Self::Evidence(error.into())
    }
}
impl From<analyte_config::Error> for Error {
    fn from(error: analyte_config::Error) -> Self {
        Self::Evidence(error.into())
    }
}
impl From<std::num::TryFromIntError> for Error {
    fn from(error: std::num::TryFromIntError) -> Self {
        Self::Evidence(error.into())
    }
}
impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        Self::Evidence(error.into())
    }
}
impl From<jiff::Error> for Error {
    fn from(error: jiff::Error) -> Self {
        Self::Evidence(error.into())
    }
}
impl From<analyte_mapping::read::Error> for Error {
    fn from(error: analyte_mapping::read::Error) -> Self {
        Self::Evidence(error.into())
    }
}
impl From<crate::machine::custody::WriteError> for Error {
    fn from(error: crate::machine::custody::WriteError) -> Self {
        Self::Evidence(error.into())
    }
}
impl From<crate::machine::custody::StoreWaitError<crate::IntakeError>> for Error {
    fn from(error: crate::machine::custody::StoreWaitError<crate::IntakeError>) -> Self {
        Self::Evidence(error.into())
    }
}
impl From<crate::machine::custody::StoreWaitError<analyte_store::Error>> for Error {
    fn from(error: crate::machine::custody::StoreWaitError<analyte_store::Error>) -> Self {
        Self::Evidence(error.into())
    }
}
