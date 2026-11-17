//! Connects protocol errors to shared causes and worker failures to their owning boundary.
/// The shared protocol component owns all accepted leaf causes. These bridges
/// preserve the concrete cause and the two independent cleanup failures.
/// Worker adapters name the storage boundary explicitly; no compatibility error
/// or nested conversion macro is involved.
macro_rules! causes {
    ($target:ty; protocol) => {
        impl<C> From<C> for $target
        where
            $crate::machine::custody::Refusal<Self>: From<C>,
        {
            fn from(cause: C) -> Self {
                Self::Refusal(cause.into())
            }
        }
        impl From<$crate::machine::custody::StoreWaitError<Self>> for $target {
            fn from(cause: $crate::machine::custody::StoreWaitError<Self>) -> Self {
                Self::Refusal($crate::machine::custody::Refusal::Custody(Box::new(cause)))
            }
        }
        impl $crate::machine::custody::CleanupFailure for $target {
            fn with_effect(original: Self, effect: Self) -> Self {
                Self::Refusal($crate::machine::custody::Refusal::Cleanup(Box::new(
                    $crate::machine::custody::Cleanup { original, effect },
                )))
            }
        }
    };
}
pub(crate) use causes;
