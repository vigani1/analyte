//! Tests immutable alert delivery with the durable local reference receiver.
use super::*;
use analyte_store::{AlertFailure, AlertKey, PendingAlert};

async fn alert(store: &Handle, now: i64) -> Result<PendingAlert, Box<dyn std::error::Error>> {
    Ok(store
        .request::<_, TestStoreError>(until()?, move |s| {
            let endpoint = Identifier::new("relay").map_err(TestStoreError::Text)?;
            s.register_alert_endpoint(&endpoint)?;
            s.evaluate_alert(&AlertKey::ParkedItems, "Duty officer", true, Some(1), now)?;
            s.prepare_alert_event(&endpoint, now, 60, 10)?;
            s.claim_alert_attempt(&endpoint, now, 3, 5, 20)?
                .ok_or(TestStoreError::MissingAlertAttempt)
        })
        .await?)
}

// ADR 0014
#[tokio::test]
async fn alert_lifecycle_and_crash_retry_reuse_bytes() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = store(&temp.path().join("store.db")).await?;
    let mut receiver = analyte_receiver::Receiver::open(
        &temp.path().join("receiver.db"),
        "key".into(),
        analyte_contract::SigningSecret::parse(SECRET)?,
    )?;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let sender = WebhookSender::for_loopback_test(
        &format!("http://{}/alerts", listener.local_addr()?),
        Duration::from_secs(2),
    )?;
    let (stop, stopped) = watch::channel(false);
    let server = tokio::spawn(receiver.serve_until(listener, stopped));
    let first = alert(&store, 100).await?;
    sender
        .send(&first, "key")
        .await
        .map_err(|_| "raise rejected")?;
    // Simulate crash after acceptance by omitting the completion write and reopening.
    store.shutdown(until()?).await?;
    let mut store = super::store(&temp.path().join("store.db")).await?;
    let retry = store
        .request::<_, TestStoreError>(until()?, |s| {
            Ok::<_, TestStoreError>(s.claim_alert_attempt(
                &Identifier::new("relay").map_err(TestStoreError::Text)?,
                105,
                3,
                5,
                20,
            )?)
        })
        .await?
        .ok_or("retry missing")?;
    assert_eq!(retry.id(), first.id());
    assert_eq!(retry.payload(), first.payload());
    sender
        .send(&retry, "key")
        .await
        .map_err(|_| "duplicate rejected")?;
    store
        .request(until()?, move |s| s.complete_alert_attempt(retry.id(), 105))
        .await?;
    for (now, active, event_kind) in [(160, true, "reminder"), (161, false, "resolved")] {
        let event = store
            .request::<_, TestStoreError>(until()?, move |s| {
                let endpoint = Identifier::new("relay").map_err(TestStoreError::Text)?;
                s.evaluate_alert(
                    &AlertKey::ParkedItems,
                    "Duty officer",
                    active,
                    Some(u64::from(active)),
                    now,
                )?;
                s.prepare_alert_event(&endpoint, now, 60, 10)?;
                s.claim_alert_attempt(&endpoint, now, 3, 5, 20)?
                    .ok_or(TestStoreError::MissingAlertAttempt)
            })
            .await?;
        let body: serde_json::Value = serde_json::from_slice(event.payload())?;
        assert_eq!(body["event"], event_kind);
        sender
            .send(&event, "key")
            .await
            .map_err(|_| "lifecycle rejected")?;
        store
            .request(until()?, move |s| s.complete_alert_attempt(event.id(), now))
            .await?;
    }
    stop.send_replace(true);
    server.await??;
    receiver.shutdown().await.map_err(|_| "receiver worker")?;
    store.shutdown(until()?).await?;
    Ok(())
}

// ADR 0014
#[tokio::test]
async fn down_endpoint_backoff_is_durable_and_bounded() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = store(&temp.path().join("store.db")).await?;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    drop(listener);
    let sender = WebhookSender::for_loopback_test(
        &format!("http://{address}/alerts"),
        Duration::from_secs(1),
    )?;
    let first = alert(&store, 100).await?;
    let failed = sender.send(&first, "key").await;
    assert!(matches!(failed, Err(AlertFailure::Network)));
    let id = first.id().clone();
    store
        .request(until()?, move |s| {
            s.fail_alert_attempt(&id, AlertFailure::Network)
        })
        .await?;
    assert!(
        store
            .request::<_, TestStoreError>(until()?, |s| Ok::<_, TestStoreError>(
                s.claim_alert_attempt(
                    &Identifier::new("relay").map_err(TestStoreError::Text)?,
                    104,
                    3,
                    5,
                    20
                )?
            ))
            .await?
            .is_none()
    );
    let mut receiver = analyte_receiver::Receiver::open(
        &temp.path().join("receiver.db"),
        "key".into(),
        analyte_contract::SigningSecret::parse(SECRET)?,
    )?;
    // A fresh port avoids racing other tests for the released one; the retry
    // identity and bytes come from the store, not from the endpoint address.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let sender = WebhookSender::for_loopback_test(
        &format!("http://{}/alerts", listener.local_addr()?),
        Duration::from_secs(1),
    )?;
    let (stop, stopped) = watch::channel(false);
    let server = tokio::spawn(receiver.serve_until(listener, stopped));
    let retry = store
        .request::<_, TestStoreError>(until()?, |s| {
            Ok::<_, TestStoreError>(s.claim_alert_attempt(
                &Identifier::new("relay").map_err(TestStoreError::Text)?,
                105,
                3,
                5,
                20,
            )?)
        })
        .await?
        .ok_or("retry missing")?;
    assert_eq!(retry.id(), first.id());
    assert_eq!(retry.payload(), first.payload());
    sender
        .send(&retry, "key")
        .await
        .map_err(|_| "recovery rejected")?;
    store
        .request(until()?, move |s| s.complete_alert_attempt(retry.id(), 105))
        .await?;
    assert_eq!(
        store
            .request(until()?, |s| s.alert_endpoint_status())
            .await?[0]
            .pending,
        0
    );
    stop.send_replace(true);
    server.await??;
    receiver.shutdown().await.map_err(|_| "receiver worker")?;
    store.shutdown(until()?).await?;
    Ok(())
}

// ADR 0014
#[tokio::test]
async fn slow_endpoint_never_blocks_store_or_intake() -> TestResult {
    use tokio::io::AsyncWriteExt as _;
    let temp = tempfile::tempdir()?;
    let mut store = store(&temp.path().join("store.db")).await?;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let sender = WebhookSender::for_loopback_test(
        &format!("http://{}/alerts", listener.local_addr()?),
        Duration::from_secs(30),
    )?;
    let event = alert(&store, 100).await?;
    let sending = sender.send(&event, "SYNTHETIC-CREDENTIAL");
    tokio::pin!(sending);
    let peer = async {
        let (mut socket, _) = listener.accept().await?;
        let mut input = tokio::io::BufReader::new(&mut socket);
        let mut length = 0;
        loop {
            let mut line = String::new();
            input.read_line(&mut line).await?;
            if line == "\r\n" {
                break;
            }
            if let Some((name, value)) = line.split_once(':')
                && name.eq_ignore_ascii_case("content-length")
            {
                length = value.trim().parse::<usize>()?;
            }
        }
        assert!(length <= 16384);
        let mut body = vec![0; length];
        input.read_exact(&mut body).await?;
        assert!(!String::from_utf8(body)?.contains("SYNTHETIC-CREDENTIAL"));
        drop(input);
        // Sender is waiting for this peer while real clinical receive/interpret/enqueue
        // still completes using its independently owned store operations.
        let metadata = receipt()?;
        let content = make_result(metadata.id, "slow-alert")?;
        let delivery = encode_delivery(
            &content,
            &metadata,
            Identifier::new("app")?,
            jiff::Timestamp::now(),
        )?;
        enqueue(&store, metadata, content, delivery).await?;
        assert_eq!(
            store
                .request::<_, TestStoreError>(until()?, |s| Ok::<_, TestStoreError>(s.health(
                    &Identifier::new("app").map_err(TestStoreError::Text)?,
                    100,
                    30
                )?))
                .await?
                .pending_count,
            1
        );
        socket
            .write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .await?;
        Ok::<_, Box<dyn std::error::Error>>(())
    };
    let (sent, peer) =
        tokio::time::timeout(HANG_GUARD, async { tokio::join!(&mut sending, peer) }).await?;
    sent.map_err(|failure| format!("fixed category: {failure:?}"))?;
    peer?;
    store.shutdown(until()?).await?;
    Ok(())
}
