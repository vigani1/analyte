//! Connects protocol refusals to typed custody causes and retains independent cleanup failures.
/// Every protocol retains the same typed custody, link and evidence causes,
/// including both failures when cleanup also fails. Keeping these three adapters
/// together prevents the six hosts from drifting in cause retention.
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
