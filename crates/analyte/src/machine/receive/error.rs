//! Classifies shared durable-store, interpretation and evidence failures.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    /// The host could not represent the storage request deadline.
    #[error(transparent)]
    Deadline(#[from] crate::DeadlineError),
    /// A durable store operation failed before its promised completion.
    #[error(transparent)]
    Store(#[from] analyte_store::Error),
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
    Write(#[from] crate::machine::custody::WriteError),
    /// The complete intake custody operation failed or exceeded its budget.
    #[error(transparent)]
    IntakeWait(#[from] crate::machine::custody::StoreWaitError<crate::IntakeError>),
    /// A durable store request failed or exceeded its custody budget.
    #[error(transparent)]
    StoreWait(#[from] crate::machine::custody::StoreWaitError<analyte_store::Error>),
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

// Rust does not chain From conversions through an enum's typed custody field.
macro_rules! forward {
    ($target:ty, $wrap:expr) => {
        analyte_store::forward_command_errors!($target);

        impl From<crate::DeadlineError> for $target {
            fn from(error: crate::DeadlineError) -> Self {
                ($wrap)(crate::machine::receive::error::Error::from(error))
            }
        }
        impl From<analyte_store::Error> for $target {
            fn from(error: analyte_store::Error) -> Self {
                ($wrap)(crate::machine::receive::error::Error::from(error))
            }
        }
        impl From<crate::IntakeError> for $target {
            fn from(error: crate::IntakeError) -> Self {
                ($wrap)(crate::machine::receive::error::Error::from(error))
            }
        }
        impl From<crate::machine::interpretation::ReadError> for $target {
            fn from(error: crate::machine::interpretation::ReadError) -> Self {
                ($wrap)(crate::machine::receive::error::Error::from(error))
            }
        }
        impl From<analyte_model::TextError> for $target {
            fn from(error: analyte_model::TextError) -> Self {
                ($wrap)(crate::machine::receive::error::Error::from(error))
            }
        }
        impl From<analyte_model::ListError> for $target {
            fn from(error: analyte_model::ListError) -> Self {
                ($wrap)(crate::machine::receive::error::Error::from(error))
            }
        }
        impl From<analyte_config::Error> for $target {
            fn from(error: analyte_config::Error) -> Self {
                ($wrap)(crate::machine::receive::error::Error::from(error))
            }
        }
        impl From<std::num::TryFromIntError> for $target {
            fn from(error: std::num::TryFromIntError) -> Self {
                ($wrap)(crate::machine::receive::error::Error::from(error))
            }
        }
        impl From<serde_json::Error> for $target {
            fn from(error: serde_json::Error) -> Self {
                ($wrap)(crate::machine::receive::error::Error::from(error))
            }
        }
        impl From<jiff::Error> for $target {
            fn from(error: jiff::Error) -> Self {
                ($wrap)(crate::machine::receive::error::Error::from(error))
            }
        }
        impl From<crate::reading::Error> for $target {
            fn from(error: crate::reading::Error) -> Self {
                ($wrap)(crate::machine::receive::error::Error::from(error))
            }
        }
        impl From<crate::machine::custody::WriteError> for $target {
            fn from(error: crate::machine::custody::WriteError) -> Self {
                ($wrap)(crate::machine::receive::error::Error::from(error))
            }
        }
        impl From<crate::machine::custody::StoreWaitError<crate::IntakeError>> for $target {
            fn from(error: crate::machine::custody::StoreWaitError<crate::IntakeError>) -> Self {
                ($wrap)(crate::machine::receive::error::Error::from(error))
            }
        }
        impl From<crate::machine::custody::StoreWaitError<analyte_store::Error>> for $target {
            fn from(error: crate::machine::custody::StoreWaitError<analyte_store::Error>) -> Self {
                ($wrap)(crate::machine::receive::error::Error::from(error))
            }
        }
    };
}
pub(crate) use forward;

// Each protocol owns cleanup state, while retention of both causes is one rule.
macro_rules! session_cleanup {
    ($target:ty) => {
        impl From<crate::machine::custody::StoreWaitError<Self>> for $target {
            fn from(error: crate::machine::custody::StoreWaitError<Self>) -> Self {
                Self::Refusal(crate::machine::custody::Refusal::Custody(Box::new(error)))
            }
        }
        impl crate::machine::custody::CleanupFailure for $target {
            fn with_effect(original: Self, effect: Self) -> Self {
                Self::Refusal(crate::machine::custody::Refusal::Cleanup(Box::new(
                    crate::machine::custody::Cleanup { original, effect },
                )))
            }
        }
    };
}
pub(crate) use session_cleanup;

// All six protocols use the same typed propagation and cleanup policy.
macro_rules! session_boundary {
    ($target:ty) => {
        impl From<crate::machine::link::error::Error> for $target {
            fn from(error: crate::machine::link::error::Error) -> Self {
                Self::Refusal(crate::machine::custody::Refusal::Link(Box::new(error)))
            }
        }
        impl From<crate::machine::receive::error::Error> for $target {
            fn from(error: crate::machine::receive::error::Error) -> Self {
                Self::Refusal(crate::machine::custody::Refusal::Evidence(Box::new(error)))
            }
        }

        crate::machine::link::error::forward!($target, |cause| Self::Refusal(
            crate::machine::custody::Refusal::Link(Box::new(cause))
        ));
        crate::machine::receive::error::forward!($target, |cause| Self::Refusal(
            crate::machine::custody::Refusal::Evidence(Box::new(cause))
        ));
        crate::machine::receive::error::session_cleanup!($target);
    };
}
pub(crate) use session_boundary;

analyte_store::forward_command_errors!(Error);
