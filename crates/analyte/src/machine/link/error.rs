//! Classifies shared transport ownership, shutdown and file-custody failures.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    /// File completion reply lost; source remains claimed.
    #[error("file completion reply lost; source remains claimed")]
    FileCompletionReplyLostSourceRemainsClaimed(
        #[source] std::sync::mpsc::SendError<analyte_transport::folder::Disposition>,
    ),
    /// File owner panicked.
    #[error("file owner panicked")]
    FileOwnerPanicked,
    /// File owner shutdown timed out.
    #[error("file owner shutdown timed out")]
    FileOwnerShutdownTimedOut(#[source] tokio::time::error::Elapsed),
    /// File owner stopped.
    #[error("file owner stopped")]
    FileOwnerStopped,
    /// File transport ended before completion; source remains claimed.
    #[error("file transport ended before completion; source remains claimed")]
    FileTransportEndedBeforeCompletionSourceRemainsClaimed,
    /// File transport opened twice.
    #[error("file transport opened twice")]
    FileTransportOpenedTwice,
    /// File transport receipt bound exceeded.
    #[error("file transport receipt bound exceeded")]
    FileTransportReceiptBoundExceeded,
    /// Invalid serial data bits.
    #[error("invalid serial data bits")]
    InvalidSerialDataBits,
    /// Invalid serial stop bits.
    #[error("invalid serial stop bits")]
    InvalidSerialStopBits,
    /// Serial owner panicked.
    #[error("serial owner panicked")]
    SerialOwnerPanicked,
    /// Serial owner shutdown timed out.
    #[error("serial owner shutdown timed out")]
    SerialOwnerShutdownTimedOut(#[source] tokio::time::error::Elapsed),
    /// Transport close stage exceeded 25 seconds.
    #[error("transport close stage exceeded 25 seconds")]
    TransportCloseStageExceeded(#[source] tokio::time::error::Elapsed),
    /// The file link was interrupted; its source remains claimed for recovery.
    #[error("file transport interrupted; source remains claimed: {0}")]
    FileInterrupted(analyte_transport::Reason),
    /// The operating system refused a transport-owner operation.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// The connection failed during a transport operation.
    #[error(transparent)]
    Transport(#[from] analyte_transport::Error),
    /// The configured network endpoint was not a valid socket address.
    #[error(transparent)]
    Address(#[from] std::net::AddrParseError),
    /// The folder watcher failed while retaining a claimed source file.
    #[error(transparent)]
    Watch(#[from] analyte_transport::folder::WatchError),
    /// File or input custody failed before link completion.
    #[error(transparent)]
    Custody(Box<crate::machine::receive::error::Error>),
}

#[cfg(test)]
impl Error {
    pub(super) fn downcast_ref<T: std::error::Error + 'static>(&self) -> Option<&T> {
        if let Self::Custody(error) = self
            && let crate::machine::receive::error::Error::Intake(error) = error.as_ref()
            && let crate::IntakeError::Store(cause) = error.as_ref()
        {
            let cause: &(dyn std::error::Error + 'static) = cause;
            return cause.downcast_ref();
        }
        None
    }
}

impl From<crate::machine::receive::error::Error> for Error {
    fn from(error: crate::machine::receive::error::Error) -> Self {
        Self::Custody(Box::new(error))
    }
}

crate::runtime::service::conversions::causes!(Error;
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
