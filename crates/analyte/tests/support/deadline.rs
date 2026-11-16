//! Names integration waits while retaining their original error causes.
use analyte::Error;
use std::future::Future;

#[derive(Debug, thiserror::Error)]
#[error("test step '{name}' failed: {source}")]
struct StepError {
    name: &'static str,
    #[source]
    source: Error,
}

/// Bounds a test step; cancellation does not establish whether a side effect committed.
pub(crate) async fn step<T, E: Into<Error>>(
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

#[tokio::test(start_paused = true)]
async fn rule7_named_deadline_identifies_stalled_step() {
    let error = step(
        "synthetic stalled commit",
        std::future::pending::<Result<(), Error>>(),
    )
    .await
    .expect_err("pending work must reach its named deadline");
    assert!(error.to_string().contains("synthetic stalled commit"));
    assert!(
        std::error::Error::source(error.as_ref())
            .is_some_and(<dyn std::error::Error>::is::<tokio::time::error::Elapsed>)
    );
}

#[tokio::test]
async fn named_step_retains_original_io_error() {
    let error = step("synthetic ACK EOF", async {
        Err::<(), _>(std::io::Error::from(std::io::ErrorKind::UnexpectedEof))
    })
    .await
    .expect_err("EOF must remain an error");
    assert!(error.to_string().contains("synthetic ACK EOF"));
    assert!(
        std::error::Error::source(error.as_ref())
            .and_then(|cause| cause.downcast_ref::<std::io::Error>())
            .is_some_and(|cause| cause.kind() == std::io::ErrorKind::UnexpectedEof)
    );
}
