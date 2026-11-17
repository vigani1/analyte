//! Classifies failures while constructing and inspecting synthetic admin fixtures.
#[derive(Debug, thiserror::Error)]
pub(super) enum FixtureError {
    #[error(transparent)]
    Maintenance(#[from] analyte_store::MaintenanceError),
    #[error(transparent)]
    Intake(#[from] analyte_store::IntakeError),
    #[error(transparent)]
    Order(#[from] analyte_store::OrderError),
    #[error(transparent)]
    Reconciliation(#[from] analyte_store::ReconciliationError),
    #[error(transparent)]
    Operator(#[from] analyte_store::OperatorError),
    #[error(transparent)]
    Delivery(#[from] analyte_store::DeliveryError),
    #[error(transparent)]
    Audit(#[from] analyte_store::StorageError),
    #[error(transparent)]
    Worker(#[from] analyte_store::WorkerError),
    #[error(transparent)]
    Text(#[from] analyte_model::TextError),
    #[error(transparent)]
    Identity(#[from] analyte_model::InvalidUuid),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    EvidenceTimestamp(#[from] jiff::Error),
    #[error("identity or state constraint conflict")]
    Conflict,
    #[error("stored evidence is corrupt")]
    Corruption,
    #[error("receipt does not exist")]
    UnknownReceipt,
    #[error("delivery does not exist")]
    UnknownDelivery,
    #[error("audit evidence does not exist")]
    UnknownAudit,
    #[error("order document lacks {field}")]
    MissingOrderField { field: &'static str },
}
