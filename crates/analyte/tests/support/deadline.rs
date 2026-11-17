//! Names integration waits while retaining their original error causes.
type Error = Box<dyn std::error::Error + Send + Sync>;
use std::future::Future;

#[derive(Debug, thiserror::Error)]
#[error("test step '{name}' failed: {source}")]
struct StepError {
    name: &'static str,
    #[source]
    source: Error,
}

/// Bounds a test step; cancellation does not establish whether a side effect committed.
/// # Errors
/// Retains the original operation failure or deadline error beneath the step name.
pub async fn step<T, E: Into<Error>>(
    name: &'static str,
    future: impl Future<Output = Result<T, E>>,
) -> Result<T, Error> {
    match tokio::time::timeout(super::HANG_GUARD, future).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(error)) => Err(StepError {
            name,
            source: error.into(),
        }
        .into()),
        Err(error) => Err(StepError {
            name,
            source: error.into(),
        }
        .into()),
    }
}
