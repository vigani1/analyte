//! Foreground host orchestration and command-line operations.
#![forbid(unsafe_code)]

pub mod admin;
mod alerts;
mod capture;
pub mod config;
mod delivery;
mod evidence;
mod hl7_response;
pub mod machine;
pub mod orders;
pub mod output;
pub mod recovery;
pub mod secrets;
pub mod service;
mod shutdown;
mod space;
pub mod status;
#[cfg(feature = "test-hooks")]
pub mod test_hooks;
#[cfg(windows)]
pub mod windows_service;

/// The orchestration boundary retains concrete errors as their original causes.
pub type Error = Box<dyn std::error::Error + Send + Sync>;

pub(crate) fn invalid(message: &str) -> Error {
    std::io::Error::new(std::io::ErrorKind::InvalidInput, message).into()
}

pub(crate) fn deadline() -> std::time::Instant {
    std::time::Instant::now()
        .checked_add(std::time::Duration::from_secs(5))
        .unwrap_or(std::time::Instant::now())
}

/// Opens the store using the operating system volume probe.
/// # Errors
/// Reports directory, migration, integrity or worker errors.
pub async fn open_system_store(program: &config::Program) -> Result<analyte_store::Handle, Error> {
    service::open_store(program, Box::new(space::Volume)).await
}
