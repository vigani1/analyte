//! Distinguishes worker admission, uncertain completion, startup and shutdown failures.
/// Failures of bounded worker admission and ownership, independent of an operation's error.
#[derive(Debug, thiserror::Error)]
pub enum WorkerError {
    /// No command was submitted; cancellation of queue admission is safe to retry.
    #[error("storage admission deadline expired; operation was not submitted")]
    AdmissionTimeout {
        /// Timer cause, absent when the deadline was already expired on entry.
        #[source]
        cause: Option<tokio::time::error::Elapsed>,
    },
    /// A submitted write may have committed; reconcile its original identity.
    #[error("storage completion deadline expired; reconcile using the original identity")]
    ResponseTimeout(#[source] tokio::time::error::Elapsed),
    /// The worker has stopped accepting new commands.
    #[error("storage worker stopped")]
    Closed,
    /// The submitted command's reply was lost; do not infer rollback.
    #[error("storage response lost; reconcile using the original identity")]
    ResponseClosed(#[source] tokio::sync::oneshot::error::RecvError),
    /// Shutdown did not finish; the handle still owns the worker and may resume joining it.
    #[error("storage shutdown deadline expired")]
    ShutdownTimeout(#[source] tokio::time::error::Elapsed),
    /// The owning thread panicked; it cannot continue a protocol session.
    #[error("storage worker panicked")]
    Panicked,
}
/// Failures starting a store owner; opening errors retain their maintenance classification.
#[derive(Debug, thiserror::Error)]
pub enum WorkerStartError {
    /// Queue bounds were refused before any owner thread was created.
    #[error("store worker queue capacity {capacity} is outside 1..=1024")]
    QueueCapacity {
        /// Rejected capacity.
        capacity: usize,
    },
    /// The operating system could not create the owner thread.
    #[error("storage worker thread creation failed")]
    Spawn(#[from] std::io::Error),
    /// Startup refused or cancelled opening, integrity checking or migration.
    #[error(transparent)]
    Open(#[from] crate::MaintenanceError),
    /// The owner stopped before publishing the startup result.
    #[error("storage worker stopped during startup")]
    Closed(#[source] tokio::sync::oneshot::error::RecvError),
}
