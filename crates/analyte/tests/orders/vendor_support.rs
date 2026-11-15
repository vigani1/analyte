//! Shared assertions for synthetic vendor order exchanges.
use super::*;
use tokio::{io::AsyncReadExt, net::TcpStream};
pub(super) async fn state(path: &Path, expected: &str) -> Result<(), Error> {
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            let found: i64 = database(path)?.query_row(
                "SELECT count(*) FROM machine_orders WHERE state=?1",
                [format!("\"{expected}\"")],
                |r| r.get(0),
            )?;
            if found == 1 {
                return Ok::<_, Error>(());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?
}
pub(super) async fn byte(peer: &mut TcpStream) -> Result<u8, Error> {
    Ok(tokio::time::timeout(Duration::from_secs(8), peer.read_u8()).await??)
}
pub(super) async fn no_event(path: &Path) -> Result<(), Error> {
    assert_eq!(
        database(path)?.query_row("SELECT count(*) FROM outbox", [], |r| r.get::<_, i64>(0))?,
        0
    );
    Ok(())
}
pub(super) async fn event(
    path: &Path,
    receiver: &Path,
    expected_state: &str,
    code: &str,
) -> Result<(), Error> {
    state(path, expected_state).await?;
    wait_state(path, "done", 1).await?;
    assert_eq!(
        database(path)?.query_row("SELECT count(*) FROM outbox", [], |r| r.get::<_, i64>(0))?,
        1
    );
    assert_eq!(receiver_count(receiver)?, 1);
    let bytes: Vec<u8> = rusqlite::Connection::open(receiver)?.query_row(
        "SELECT body FROM deliveries WHERE kind='order-events'",
        [],
        |r| r.get(0),
    )?;
    let body: serde_json::Value = serde_json::from_slice(&bytes)?;
    assert_eq!(
        body.get("state").and_then(serde_json::Value::as_str),
        Some(expected_state.to_ascii_lowercase().as_str())
    );
    assert_eq!(
        body.get("order_id").and_then(serde_json::Value::as_str),
        Some("order-1")
    );
    assert_eq!(
        body.get("source_status")
            .and_then(serde_json::Value::as_str),
        Some(code)
    );
    Ok(())
}
