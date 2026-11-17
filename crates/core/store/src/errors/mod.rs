//! Groups failures by the store operation that can return them.
mod intake;
mod storage;
pub use intake::IntakeError;
pub use storage::StorageError;

mod delivery;
pub use delivery::DeliveryError;

mod orders;
pub use orders::OrderError;

mod maintenance;
pub use maintenance::MaintenanceError;

mod operator;
pub use operator::OperatorError;
