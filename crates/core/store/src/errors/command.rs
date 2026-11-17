//! Forwards concrete store-boundary causes into a host's mixed-command error field.
/// Implements cause-preserving conversions for host errors that already accept
/// the compatibility command `Error`. Direct worker callers retain their native
/// boundary error and do not need these conversions.
#[macro_export]
macro_rules! forward_command_errors {
    ($target:ty) => {
        $crate::forward_command_errors!(@impl $target;
            $crate::WorkerError, $crate::WorkerStartError, $crate::IntakeError,
            $crate::DeliveryError, $crate::OrderError, $crate::MaintenanceError,
            $crate::OperatorError, $crate::ReconciliationError, $crate::StorageError
        );
    };
    (@impl $target:ty; $($cause:ty),+ $(,)?) => {
        $(
            impl From<$cause> for $target {
                fn from(cause: $cause) -> Self {
                    Self::from($crate::Error::from(cause))
                }
            }
        )+
    };
}
