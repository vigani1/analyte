//! Groups failures by the store operation that can return them.
mod intake;
mod storage;
pub use intake::IntakeError;
pub use storage::StorageError;

mod delivery;
pub use delivery::DeliveryError;
