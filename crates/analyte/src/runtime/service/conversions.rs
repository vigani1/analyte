//! Connects protocol errors to shared causes and worker failures to their owning boundary.
/// The shared protocol component owns all accepted leaf causes. These bridges
/// preserve the concrete cause and the two independent cleanup failures.
/// Worker adapters name the storage boundary explicitly; no compatibility error
/// or nested conversion macro is involved.
macro_rules! causes {
    ($target:ty; protocol) => {
        impl<C> From<C> for $target
        where
            $crate::machine::receive::shared::Shared: From<C>,
        {
            fn from(cause: C) -> Self {
                Self::Refusal($crate::machine::receive::shared::Shared::from(cause).into())
            }
        }
        impl From<$crate::machine::receive::custody::StoreWaitError<Self>> for $target {
            fn from(cause: $crate::machine::receive::custody::StoreWaitError<Self>) -> Self {
                Self::Refusal($crate::machine::receive::custody::Refusal::Custody(
                    Box::new(cause),
                ))
            }
        }
        impl $crate::machine::receive::custody::CleanupFailure for $target {
            fn with_effect(original: Self, effect: Self) -> Self {
                Self::Refusal($crate::machine::receive::custody::Refusal::Cleanup(
                    Box::new($crate::machine::receive::custody::Cleanup { original, effect }),
                ))
            }
        }
    };
    ($target:ty; worker $boundary:ty) => {
        impl From<analyte_store::WorkerError> for $target {
            fn from(cause: analyte_store::WorkerError) -> Self {
                Self::Store(<$boundary>::from(cause))
            }
        }
    };
}
pub(crate) use causes;
