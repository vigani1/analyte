//! Retains the concrete storage boundaries combined by authenticated admin commands.
/// Store and filesystem failures of mixed admin commands.
#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    /// The evidence boundary failed.
    #[error(transparent)]
    Evidence(#[from] analyte_store::OperatorError),
    /// The order boundary failed.
    #[error(transparent)]
    Order(#[from] analyte_store::OrderError),
    /// The maintenance boundary failed.
    #[error(transparent)]
    Maintenance(#[from] analyte_store::MaintenanceError),
    /// The audit boundary failed.
    #[error(transparent)]
    Audit(#[from] analyte_store::StorageError),
    /// The delivery boundary failed.
    #[error(transparent)]
    Delivery(#[from] analyte_store::DeliveryError),
    /// The worker boundary failed.
    #[error(transparent)]
    Worker(#[from] analyte_store::WorkerError),
    /// The host volume probe failed.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
