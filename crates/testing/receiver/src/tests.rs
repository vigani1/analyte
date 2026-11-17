//! Tests that the reference receiver joins its storage worker on every exit path.
use super::*;
use analyte_fixtures::SECRET;
#[tokio::test]
async fn rule1_error_after_open_drains_queued_write_with_live_router()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("receiver.db");
    let mut receiver = Receiver::open(&path, "key".into(), SigningSecret::parse(SECRET)?)?;
    let router = receiver.router();
    let (reply, response) = oneshot::channel();
    receiver.state.try_send(Command::Delivery {
        id: "synthetic".into(),
        kind: "results",
        body: b"synthetic".to_vec(),
        reply,
    })?;
    let order: analyte_contract::Order = serde_json::from_slice(include_bytes!(
        "../../../../docs/contract/vectors/order.json"
    ))?;
    let original = receiver.seed(vec![order; 101]).await;
    assert!(matches!(original, Err(ReceiverError::SeedLimit)));
    receiver.shutdown().await?;
    assert!(response.await??);
    let connection = Connection::open(path)?;
    let body: Vec<u8> = connection.query_row(
        "SELECT body FROM deliveries WHERE id='synthetic'",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(body, b"synthetic");
    drop(router);
    Ok(())
}
#[tokio::test]
async fn rule7_shutdown_surfaces_worker_panic() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    let mut receiver = Receiver::open(
        &temp.path().join("receiver.db"),
        "key".into(),
        SigningSecret::parse(SECRET)?,
    )?;
    receiver.state.try_send(Command::Panic)?;
    assert!(matches!(
        receiver.shutdown().await,
        Err(ReceiverError::Worker)
    ));
    Ok(())
}
