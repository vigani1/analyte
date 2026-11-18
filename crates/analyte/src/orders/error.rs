//! Distinguishes shared order lookup, deadline, storage and settlement failures while retaining their causes.
/// Failures every protocol's order code can raise; each protocol's error wraps them.
#[derive(Debug, thiserror::Error)]
pub enum OrderError {
    /// The shared send-ahead poll cadence could not represent its next deadline.
    #[error("order poll deadline overflow")]
    PollDeadlineOverflow,
    /// Durable order lookup or lifecycle persistence failed.
    #[error(transparent)]
    Store(#[from] analyte_store::OrderError),
    /// The host could not represent the storage request deadline.
    #[error(transparent)]
    Deadline(#[from] crate::DeadlineError),
    /// Retaining or interpreting the machine response failed.
    #[error(transparent)]
    Intake(#[from] crate::IntakeError),
    /// An order identity or retained outcome violated model text bounds.
    #[error(transparent)]
    Text(#[from] analyte_model::TextError),
    /// The cached app order violated the app contract.
    #[error(transparent)]
    Document(#[from] analyte_contract::DocumentError),
    /// Order response text was not valid UTF-8.
    #[error(transparent)]
    Encoding(#[from] std::str::Utf8Error),
    /// Retains both the triggering failure and a later independent settlement failure.
    #[error("{original}; dialog effect failed: {effect}")]
    Cleanup {
        /// Triggering operation failure.
        #[source]
        original: Box<Self>,
        /// Independent failure while settling the operation.
        effect: Box<Self>,
    },
}
impl crate::cleanup::CleanupFailure for OrderError {
    fn with_effect(original: Self, effect: Self) -> Self {
        Self::Cleanup {
            original: Box::new(original),
            effect: Box::new(effect),
        }
    }
}

impl From<analyte_store::WorkerError> for OrderError {
    fn from(error: analyte_store::WorkerError) -> Self {
        Self::Store(error.into())
    }
}

/// Routes every shared cause into a protocol order error's `Shared` variant, so it
/// never becomes a custody refusal, and pairs cleanup failures in its `Cleanup` variant.
macro_rules! shared_conversions {
    ($target:ty) => {
        impl<C> From<C> for $target
        where
            $crate::orders::OrderError: From<C>,
        {
            fn from(cause: C) -> Self {
                Self::Shared($crate::orders::OrderError::from(cause))
            }
        }
        impl $crate::cleanup::CleanupFailure for $target {
            fn with_effect(original: Self, effect: Self) -> Self {
                Self::Cleanup(Box::new($crate::machine::receive::custody::Cleanup {
                    original,
                    effect,
                }))
            }
        }
    };
}
pub(crate) use shared_conversions;
