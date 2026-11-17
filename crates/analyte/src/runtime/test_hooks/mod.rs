//! Exposes synthetic controls only with test-hooks, and their typed failures to host facades.
#[cfg(feature = "test-hooks")]
mod enabled;
#[cfg(feature = "test-hooks")]
pub use enabled::*;

/// Specific failures of bounded synthetic test gates.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// test commit hook stopped connection.
    #[error("test commit hook stopped connection")]
    Stopped,
    /// test commit hook control dropped.
    #[error("test commit hook control dropped")]
    ControlDropped,
    /// test commit hook deadline.
    #[error("test commit hook deadline")]
    Deadline,
    /// test commit hook not reached.
    #[error("test commit hook not reached")]
    NotReached,
    /// test probe poisoned.
    #[error("test probe poisoned")]
    ProbePoisoned,
    /// test store closed.
    #[error("test store closed")]
    StoreClosed,
    /// A gate wait exceeded its bounded timeout.
    #[error(transparent)]
    Wait(#[from] tokio::time::error::Elapsed),
}
