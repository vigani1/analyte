//! Tests startup failure propagation before peer operations.
use super::*;
#[tokio::test]
async fn startup_failure_reaches_caller_before_any_peer_or_cache_wait() -> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let path = config(temp.path(), false)?;
    // A file where the protected directory belongs forces a real startup I/O error.
    std::fs::write(temp.path().join("data"), b"synthetic blocked directory")?;
    let error = Running::start(&path, "http://127.0.0.1:1", "synthetic-api")
        .await
        .err()
        .ok_or("failed service startup was reported as ready")?;
    assert!(
        matches!(
            error.downcast_ref::<analyte::runtime::Error>(),
            Some(analyte::runtime::Error::Io(_))
        ),
        "{error}"
    );
    Ok(())
}
