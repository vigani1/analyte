//! Classifies shared durable-store, interpretation and evidence failures.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    /// Transport ownership, shutdown or file-link custody failed.
    #[error(transparent)]
    Link(#[from] crate::machine::link::error::Error),

    /// A bounded synthetic gate stopped or could not resume the connection.
    #[error(transparent)]
    TestHook(#[from] crate::runtime::test_hooks::Error),
    /// The host could not represent the storage request deadline.
    #[error(transparent)]
    Deadline(#[from] crate::DeadlineError),
    /// A durable store operation failed before its promised completion.
    #[error(transparent)]
    Store(#[from] analyte_store::IntakeError),
    /// Order or poll lifecycle storage failed.
    #[error(transparent)]
    Order(#[from] analyte_store::OrderError),
    /// Protected evidence lookup failed.
    #[error(transparent)]
    Evidence(#[from] analyte_store::OperatorError),
    /// Stored interpretations could not be reconciled.
    #[error(transparent)]
    Reconciliation(#[from] analyte_store::ReconciliationError),
    /// Receipt retention or interpretation failed at the intake boundary.
    #[error(transparent)]
    Intake(Box<crate::IntakeError>),
    /// Retained receipt context could not be loaded for interpretation.
    #[error(transparent)]
    Reading(Box<crate::machine::interpretation::ReadError>),
    /// Evidence text could not satisfy its model bounds.
    #[error(transparent)]
    Text(#[from] analyte_model::TextError),
    /// An evidence list exceeded its model limit.
    #[error(transparent)]
    List(#[from] analyte_model::ListError),
    /// The installed protocol configuration could not be used.
    #[error(transparent)]
    Config(#[from] analyte_config::Error),
    /// A protocol length or offset could not fit its target integer.
    #[error(transparent)]
    Integer(#[from] std::num::TryFromIntError),
    /// Retained structured evidence could not be serialized or read.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    /// A protocol timestamp could not be represented.
    #[error(transparent)]
    Timestamp(#[from] jiff::Error),
    /// A retained receipt failed framing, syntax or mapping.
    #[error(transparent)]
    MappingRead(Box<crate::reading::Error>),
    /// A bounded vendor write failed or has uncertain completion.
    #[error(transparent)]
    Write(#[from] crate::machine::receive::custody::WriteError),
    /// The complete intake custody operation failed or exceeded its budget.
    #[error(transparent)]
    IntakeWait(#[from] crate::machine::receive::custody::StoreWaitError<crate::IntakeError>),
    /// A durable store request failed or exceeded its custody budget.
    #[error(transparent)]
    StoreWait(#[from] crate::machine::receive::custody::StoreWaitError<analyte_store::IntakeError>),
}

/// The host step a shared cause comes from. Vendor hosts name this step in
/// the order outcome they store when a session ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    /// A bounded write to the machine failed or its completion is uncertain.
    Write,
    /// The driver could not receive or journal connection input. Hosts never
    /// use the link themselves; their writes have their own [`Step::Write`].
    Input,
    /// Durable custody of received bytes failed or exceeded its budget.
    Store {
        /// The budget expired, so the store may still complete.
        timed_out: bool,
    },
    /// Any other cause; the protocol's own rule decides its outcome.
    Other,
}

impl Error {
    /// Names the host step this cause comes from.
    pub(crate) const fn step(&self) -> Step {
        use crate::machine::receive::custody::StoreWaitError;
        match self {
            Self::Write(_) => Step::Write,
            Self::Link(_) => Step::Input,
            Self::IntakeWait(StoreWaitError::Timeout(_))
            | Self::StoreWait(StoreWaitError::Timeout(_)) => Step::Store { timed_out: true },
            Self::IntakeWait(StoreWaitError::Operation(_))
            | Self::StoreWait(StoreWaitError::Operation(_)) => Step::Store { timed_out: false },
            Self::TestHook(_)
            | Self::Deadline(_)
            | Self::Store(_)
            | Self::Order(_)
            | Self::Evidence(_)
            | Self::Reconciliation(_)
            | Self::Intake(_)
            | Self::Reading(_)
            | Self::Text(_)
            | Self::List(_)
            | Self::Config(_)
            | Self::Integer(_)
            | Self::Json(_)
            | Self::Timestamp(_)
            | Self::MappingRead(_) => Step::Other,
        }
    }
}

impl From<crate::IntakeError> for Error {
    fn from(error: crate::IntakeError) -> Self {
        Self::Intake(Box::new(error))
    }
}

impl From<crate::reading::Error> for Error {
    fn from(error: crate::reading::Error) -> Self {
        Self::MappingRead(Box::new(error))
    }
}

impl From<crate::machine::interpretation::ReadError> for Error {
    fn from(error: crate::machine::interpretation::ReadError) -> Self {
        Self::Reading(Box::new(error))
    }
}

impl From<analyte_store::WorkerError> for Error {
    fn from(cause: analyte_store::WorkerError) -> Self {
        Self::Store(analyte_store::IntakeError::from(cause))
    }
}

impl From<std::io::Error> for Error {
    fn from(cause: std::io::Error) -> Self {
        Self::Link(cause.into())
    }
}
impl From<analyte_transport::Error> for Error {
    fn from(cause: analyte_transport::Error) -> Self {
        Self::Link(cause.into())
    }
}
impl From<std::net::AddrParseError> for Error {
    fn from(cause: std::net::AddrParseError) -> Self {
        Self::Link(cause.into())
    }
}
impl From<analyte_transport::folder::WatchError> for Error {
    fn from(cause: analyte_transport::folder::WatchError) -> Self {
        Self::Link(cause.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // ADR 0003
    #[test]
    fn conversions_keep_typed_link_and_store_causes() {
        let link = Error::from(analyte_transport::Error::Deadline(
            analyte_transport::DeadlineKind::WriteAll,
        ));
        assert!(matches!(
            link,
            Error::Link(crate::machine::link::error::Error::Transport(
                analyte_transport::Error::Deadline(analyte_transport::DeadlineKind::WriteAll)
            ))
        ));
        let store = Error::from(analyte_store::IntakeError::from(
            analyte_store::StorageError::Capacity,
        ));
        assert!(matches!(
            store,
            Error::Store(analyte_store::IntakeError::Storage(
                analyte_store::StorageError::Capacity
            ))
        ));
    }
}
