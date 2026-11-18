//! Tests named test deadlines and retained failure causes.
use super::*;
// Safety rule 7 (ADR 0003)
#[tokio::test(start_paused = true)]
async fn named_deadline_identifies_stalled_step() {
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
