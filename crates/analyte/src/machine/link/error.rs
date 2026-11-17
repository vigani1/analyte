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
    TransportCloseStageExceeded25Seconds(#[source] tokio::time::error::Elapsed),
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

crate::machine::receive::error::forward!(Error, |cause| Self::Custody(Box::new(cause)));

// Preserve the source classification when a protocol wraps a shared link error.
macro_rules! forward {
    ($target:ty, $wrap:expr) => {
        impl From<std::io::Error> for $target {
            fn from(error: std::io::Error) -> Self {
                ($wrap)(crate::machine::link::error::Error::from(error))
            }
        }
        impl From<analyte_transport::Error> for $target {
            fn from(error: analyte_transport::Error) -> Self {
                ($wrap)(crate::machine::link::error::Error::from(error))
            }
        }
        impl From<std::net::AddrParseError> for $target {
            fn from(error: std::net::AddrParseError) -> Self {
                ($wrap)(crate::machine::link::error::Error::from(error))
            }
        }
        impl From<analyte_transport::folder::WatchError> for $target {
            fn from(error: analyte_transport::folder::WatchError) -> Self {
                ($wrap)(crate::machine::link::error::Error::from(error))
            }
        }
    };
}
pub(crate) use forward;
