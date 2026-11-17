//! Adapts shared protocol causes and concrete store errors at their owning boundaries.
/// Concrete store commands accepted by mixed-command owners. The closed list
/// keeps model and protocol errors out of this adapter even when the store can
/// also construct an error from them.
pub(crate) trait StoreCause: Into<analyte_store::Error> {}
impl StoreCause for analyte_store::WorkerError {}
impl StoreCause for analyte_store::WorkerStartError {}
impl StoreCause for analyte_store::IntakeError {}
impl StoreCause for analyte_store::DeliveryError {}
impl StoreCause for analyte_store::OrderError {}
impl StoreCause for analyte_store::MaintenanceError {}
impl StoreCause for analyte_store::OperatorError {}
impl StoreCause for analyte_store::ReconciliationError {}
impl StoreCause for analyte_store::StorageError {}

/// Connects an owner's error to its shared component. The concrete cause set
/// lives once in `Shared` (protocols) or `StoreCause` (mixed store owners).
/// No nested macros, cause erasure or protocol-specific classification occur here.
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
    ($target:ty; store) => {
        impl<C: $crate::runtime::service::conversions::StoreCause> From<C> for $target {
            fn from(cause: C) -> Self {
                let cause: analyte_store::Error = cause.into();
                Self::from(cause)
            }
        }
    };
}
pub(crate) use causes;
