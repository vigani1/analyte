//! Runs program workers, host I/O, custody and operator actions without interpreting clinical results.
#![forbid(unsafe_code)]

pub mod admin;
pub mod installation;
mod intake;
/// Operational failures retaining and interpreting machine receipts.
pub use intake::Error as IntakeError;
/// Failures constructing complete departure evidence.
pub use machine::interpretation::departure_evidence::EvidenceError as DepartureEvidenceError;
/// Failures comparing returned LAW work with retained immutable commands.
pub use orders::law::validation::ValidationError as WorkValidationError;
pub mod machine;
pub mod operator;
pub mod orders;
mod reading;
/// Failures reading retained receipts through framing, syntax and mapping.
pub use reading::{Context as ReceiptReadContext, Error as ReceiptReadError};
pub mod runtime;
/// Bounds ordinary orchestration storage requests independently of machine protocol timers.
const STORE_OPERATION_BUDGET: std::time::Duration = std::time::Duration::from_secs(5);

/// The orchestration boundary retains concrete errors as their original causes.
pub type Error = Box<dyn std::error::Error + Send + Sync>;

#[cfg(test)]
pub(crate) fn invalid(message: &str) -> Error {
    std::io::Error::new(std::io::ErrorKind::InvalidInput, message).into()
}

/// Bounds a store operation by the shared five-second budget.
/// # Errors
/// Reports deadline overflow rather than submitting an expired request.
pub fn deadline() -> Result<std::time::Instant, analyte_store::Error> {
    storage_deadline(std::time::Instant::now(), STORE_OPERATION_BUDGET)
}

fn storage_deadline(
    now: std::time::Instant,
    budget: std::time::Duration,
) -> Result<std::time::Instant, analyte_store::Error> {
    now.checked_add(budget)
        .ok_or(analyte_store::Error::DeadlineOverflow)
}

/// Opens the store using the operating system volume probe.
/// # Errors
/// Reports directory, migration, integrity or worker errors.
pub async fn open_system_store(
    program: &installation::config::Program,
) -> Result<analyte_store::Handle, runtime::Error> {
    runtime::service::open_store(program, Box::new(runtime::space::Volume)).await
}

#[cfg(test)]
use analyte_fixtures as test_guard;

/// Fixture migrations use the existing test wait bound, not a production operation budget.
#[cfg(test)]
pub(crate) fn test_startup_deadline() -> Result<std::time::Instant, Error> {
    std::time::Instant::now()
        .checked_add(test_guard::HANG_GUARD)
        .ok_or_else(|| invalid("test startup deadline overflow"))
}

#[cfg(test)]
mod deadline_tests {
    #[test]
    fn overflowing_store_deadline_is_specific() {
        assert!(matches!(
            super::storage_deadline(std::time::Instant::now(), std::time::Duration::MAX),
            Err(analyte_store::Error::DeadlineOverflow)
        ));
    }
}
