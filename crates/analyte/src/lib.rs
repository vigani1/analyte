//! Program orchestration owns host I/O, custody and operator actions; it must not interpret clinical results.
#![forbid(unsafe_code)]

pub mod admin;
mod evidence;
pub mod installation;
pub mod machine;
pub mod operator;
pub mod orders;
pub mod runtime;

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
pub async fn open_system_store(program: &installation::config::Program) -> Result<analyte_store::Handle, Error> {
    runtime::service::open_store(program, Box::new(runtime::space::Volume)).await
}
