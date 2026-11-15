//! Declared single-persistent-link scope; TF-2b B.5/B.6 and 3.27-3.29.
//! This is host integration evidence, not independent-link conformance.
use super::*;

async fn client(
    settings: &str,
) -> Result<
    (
        tempfile::TempDir,
        Fixture,
        Reference,
        Running,
        TcpStream,
        TcpListener,
    ),
    Error,
> {
    let temp = tempfile::tempdir()?;
    let path = vendor_config(
        temp.path(),
        "law",
        &settings.replace("kind = \"tcp_server\"", "kind = \"tcp_client\""),
    )?;
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let loaded = Program::load(&path)?;
    let old = &loaded
        .machines()
        .first()
        .ok_or("machine")?
        .snapshot()
        .effective()
        .site()
        .endpoint;
    let site = temp.path().join("hl7-site.toml");
    std::fs::write(
        &site,
        std::fs::read_to_string(&site)?.replace(old, &listener.local_addr()?.to_string()),
    )?;
    let reference = Reference::start(
        &temp.path().join("receiver.db"),
        TcpListener::bind("127.0.0.1:0").await?,
        false,
    )
    .await?;
    reference
        .receiver
        .seed(vec![order("order-1", "000Ab12", Some("law"), 2)?])
        .await?;
    let mut loaded = Program::load(&path)?;
    loaded.set_loopback_destination_for_test("demo", reference.url.clone())?;
    let running = Running::start_program(loaded, "synthetic-api")?;
    let (peer, _) = tokio::time::timeout(Duration::from_secs(5), listener.accept())
        .await
        .map_err(|e| format!("reconnect after disconnect: {e}"))??;
    ready(&path).await?;
    Ok((temp, path, reference, running, peer, listener))
}

#[tokio::test]
async fn law_tf2b_b5_b6_declared_single_persistent_link_carries_all_transactions_in_both_directions()
-> Result<(), Error> {
    for is_client in [false, true] {
        let (_temp, path, mut reference, running, mut peer, listener) = if is_client {
            let (temp, path, reference, running, peer, listener) = client(&definition()).await?;
            (temp, path, reference, running, peer, Some(listener))
        } else {
            let (temp, path, reference, running, peer) = setup(
                vec![order("order-1", "000Ab12", Some("law"), 2)?],
                &definition(),
                false,
            )
            .await?;
            (temp, path, reference, running, peer, None)
        };
        send(&mut peer, query(CONTAINER, "role-query").as_bytes()).await?;
        let rsp = packet(&mut peer).await?;
        assert_eq!(field(&rsp, b"MSA", 1)?, b"AA");
        assert_eq!(field(&rsp, b"QAK", 2)?, b"OK");
        let work = packet(&mut peer).await?;
        assert_eq!(field(&work, b"MSH", 9)?, b"OML^O33^OML_O33");
        orl(&mut peer, &work, &["OK", "OK"], false).await?;
        vendor_support::state(&path, "Accepted").await?;
        wait_state(&path, "done", 1).await?;
        let awos = std::str::from_utf8(field(&work, b"OBR", 2)?)?;
        let result = law_results::result("role-result", &format!("{awos}^LAB"), "", "12.30", "F")
            .replace("00012a^LAB", "000Ab12^LAB");
        send(&mut peer, result.as_bytes()).await?;
        let ack = packet(&mut peer).await?;
        assert_eq!(field(&ack, b"MSH", 9)?, b"ACK^R22^ACK");
        assert_eq!(field(&ack, b"MSA", 1)?, b"AA");
        let delivered: serde_json::Value = serde_json::from_slice(&reference.event().await?.bytes)?;
        assert_eq!(
            delivered
                .pointer("/results/0/sample/id")
                .and_then(|v| v.as_str()),
            Some("000Ab12")
        );
        assert_eq!(
            delivered
                .pointer("/results/0/identity/resolved/observation/order/value")
                .and_then(|v| v.as_str()),
            Some(awos)
        );
        send(&mut peer, query(ALL, "role-empty").as_bytes()).await?;
        assert_eq!(field(&packet(&mut peer).await?, b"QAK", 2)?, b"OK");
        let negative = packet(&mut peer).await?;
        assert!(!negative.segments().any(|s| s.id() == b"OBR"));
        orl(&mut peer, &negative, &[], false).await?;
        wait_audit(&path).await?;
        assert_eq!(count(&path, "machine_order_items")?, 2);
        wait_state(&path, "done", 2).await?;
        quiet(&mut peer).await?;
        if let Some(listener) = listener {
            assert!(
                tokio::time::timeout(Duration::from_millis(100), listener.accept())
                    .await
                    .is_err(),
                "transactions must share the persistent link"
            );
        }
        running.stop().await?;
        reference.close().await?;
    }
    Ok(())
}

#[tokio::test]
async fn law_tf2b_b6_single_persistent_client_reconnects_without_replaying_work()
-> Result<(), Error> {
    let settings = definition()
        .replace("send_ahead = false", "send_ahead = true")
        .replace("receive_timeout_ms = 30000", "receive_timeout_ms = 100");
    let (_temp, path, reference, running, mut peer, listener) = client(&settings).await?;
    packet(&mut peer).await?;
    vendor_support::state(&path, "Sending").await?;
    drop(peer);
    let (mut peer, _) = tokio::time::timeout(Duration::from_secs(5), listener.accept())
        .await
        .map_err(|e| format!("reconnect after disconnect: {e}"))??;
    vendor_support::state(&path, "Uncertain").await?;
    quiet(&mut peer).await?;
    // A first-byte deadline closes only this session. Original bytes survive and
    // reconnect cannot grant permission to resend its already-reserved work.
    peer.write_all(&[11, b'M', b'S', b'H']).await?;
    let mut byte = [0];
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(3), peer.read(&mut byte)).await??,
        0
    );
    drop(peer);
    let (mut peer, _) = tokio::time::timeout(Duration::from_secs(3), listener.accept())
        .await
        .map_err(|e| format!("reconnect after partial deadline: {e}"))??;
    quiet(&mut peer).await?;
    assert_eq!(count(&path, "machine_orders")?, 1);
    assert_eq!(count(&path, "machine_order_items")?, 2);
    assert!(
        count(&path, "receipts")? >= 1,
        "partial input must remain retained"
    );
    running.stop().await?;
    reference.close().await?;
    Ok(())
}
