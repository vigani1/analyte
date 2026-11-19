//! Tests the site lab-app request deadline against a slow reference receiver.
use super::*;

pub(crate) fn set_request_timeout(root: &Path, machine: &str, seconds: u64) -> Result<(), Error> {
    let site = root.join(format!("{machine}-site.toml"));
    let original = std::fs::read_to_string(&site)?;
    std::fs::write(
        &site,
        format!("{original}\nrequest_timeout_seconds = {seconds}\n"),
    )?;
    Ok(())
}

fn attempts(path: &Path) -> Result<Vec<String>, Error> {
    let connection = database(path)?;
    let mut query = connection.prepare("SELECT outcome FROM attempts ORDER BY sequence")?;
    let outcomes = query
        .query_map([], |row| row.get(0))?
        .collect::<Result<Vec<String>, _>>()?;
    Ok(outcomes)
}

// The former fixed 3-second deadline timed out every attempt to an app that needs
// longer to store and answer, so no delivery ever completed. The site deadline
// lets that answer arrive on the first attempt (ADR 0008 idempotent receiver).
// ADR 0008
#[tokio::test]
async fn slow_app_within_request_timeout_succeeds() -> Result<(), Error> {
    const APP_DELAY: Duration = Duration::from_secs(4);
    let temp = tempfile::tempdir()?;
    let fixture = config(temp.path(), false)?;
    set_request_timeout(temp.path(), "hl7", 10 * analyte_fixtures::CI_SCALE)?;
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let mut reference = Reference::start(&temp.path().join("reference.db"), listener, true).await?;
    let running = Running::start(&fixture, &reference.url, "synthetic-api").await?;
    let mut peer = connect(&fixture, "hl7").await?;
    let ack = tokio::time::timeout(HANG_GUARD, hl7(&mut peer, ORU)).await??;
    assert!(ack.contains("MSA|AA|message1"));
    // The receiver has stored the delivery; its answer is held back for APP_DELAY.
    let stored = reference.event().await?;
    assert!(stored.outcome.contains("accepted"));
    tokio::time::sleep(APP_DELAY).await;
    reference.capture.release.notify_one();
    wait_state(&fixture, "done", 1).await?;
    let outcomes = attempts(&fixture)?;
    assert_eq!(outcomes.len(), 1, "one attempt must succeed: {outcomes:?}");
    assert!(
        reference.events.try_recv().is_err(),
        "no retry may reach the app"
    );
    assert_eq!(receiver_count(&temp.path().join("reference.db"))?, 1);
    drop(peer);
    running.stop().await?;
    reference.close().await?;
    Ok(())
}

// A timeout never proves the app did nothing: the attempt is recorded as a retry
// and the next attempt resends the same delivery ID and bytes (ADR 0008).
// ADR 0008
#[tokio::test]
async fn app_slower_than_timeout_retries_same_delivery() -> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let fixture = config(temp.path(), false)?;
    set_request_timeout(temp.path(), "hl7", 1)?;
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let mut reference = Reference::start(&temp.path().join("reference.db"), listener, true).await?;
    let running = Running::start(&fixture, &reference.url, "synthetic-api").await?;
    let mut peer = connect(&fixture, "hl7").await?;
    let ack = tokio::time::timeout(HANG_GUARD, hl7(&mut peer, ORU)).await??;
    assert!(ack.contains("MSA|AA|message1"));
    // The app stores the first attempt but holds its answer past the deadline.
    let first = reference.event().await?;
    assert!(first.outcome.contains("accepted"));
    let retry = reference.event().await?;
    assert!(retry.outcome.contains("duplicate"));
    assert_eq!(retry.id, first.id);
    assert_eq!(retry.bytes, first.bytes);
    wait_state(&fixture, "done", 1).await?;
    let outcomes = attempts(&fixture)?;
    assert_eq!(outcomes.len(), 2, "{outcomes:?}");
    assert!(outcomes[0].contains("RetryAt"), "{outcomes:?}");
    assert_eq!(receiver_count(&temp.path().join("reference.db"))?, 1);
    reference.capture.release.notify_one();
    drop(peer);
    running.stop().await?;
    reference.close().await?;
    Ok(())
}

// One destination has one request deadline; machines sharing it must agree.
// ADR 0004
#[test]
fn shared_destination_refuses_different_timeouts() -> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let fixture = config(temp.path(), true)?;
    set_request_timeout(temp.path(), "hl7", 20)?;
    assert!(matches!(
        Program::load(&fixture),
        Err(analyte::installation::config::Error::ConflictingLabApp { .. })
    ));
    set_request_timeout(temp.path(), "astm", 20)?;
    Program::load(&fixture)?;
    Ok(())
}
