//! Tests LAW connection ownership and reconnect behavior.
//! This is host integration evidence, not independent-link conformance.
use super::*;
use crate::support::HANG_GUARD;

#[path = "client_fixture.rs"]
mod client_fixture;
#[cfg(feature = "test-hooks")]
use client_fixture::client_with_setup;
use client_fixture::{client, wait_parked_frame};

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
                // Deliberate quiet window: no reply may arrive.
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
async fn law_tf2b_b6_adr0003_client_partial_deadline_retains_exact_bytes_across_reconnect()
-> Result<(), Error> {
    let settings = definition()
        .replace("send_ahead = false", "send_ahead = true")
        .replace("receive_timeout_ms = 30000", "receive_timeout_ms = 100");
    let (_temp, path, reference, running, mut peer, listener) = client(&settings).await?;
    packet(&mut peer).await?;
    vendor_support::state(&path, "Sending").await?;
    drop(peer);
    let (mut peer, _) = tokio::time::timeout(HANG_GUARD, listener.accept())
        .await
        .map_err(|e| format!("reconnect after disconnect: {e}"))??;
    vendor_support::state(&path, "Uncertain").await?;
    quiet(&mut peer).await?;
    // PaLM TF-2b B.6 / ADR 0003: retain unrelated framing evidence first.
    // A complete query would require storage and a response inside this test's
    // deliberately short 100 ms receive budget, racing slow CI storage.
    peer.write_all(b"?").await?;
    assert_eq!(
        tokio::time::timeout(HANG_GUARD, peer.read(&mut [0]))
            .await
            .map_err(|e| format!("unrelated EOF: {e}"))??,
        0
    );
    wait_parked_frame(&path, b"?", "byte outside block")
        .await
        .map_err(|e| format!("unrelated parking: {e}"))?;
    let unrelated: Vec<u8> = database(&path)?.query_row(
        "SELECT raw FROM receipts WHERE raw=?1",
        [b"?".as_slice()],
        |r| r.get(0),
    )?;
    assert_eq!(unrelated, b"?");
    drop(peer);
    // Observe the replacement session after the unrelated framing rejection;
    // only this newly accepted connection receives the partial block.
    let (mut peer, _) = tokio::time::timeout(HANG_GUARD, listener.accept())
        .await
        .map_err(|e| format!("replacement accept: {e}"))??;
    let partial = [11, b'M', b'S', b'H'];
    peer.write_all(&partial).await?;
    let mut byte = [0];
    assert_eq!(
        tokio::time::timeout(HANG_GUARD, peer.read(&mut byte))
            .await
            .map_err(|e| format!("partial EOF: {e}"))??,
        0
    );
    drop(peer);
    let (mut peer, _) = tokio::time::timeout(HANG_GUARD, listener.accept())
        .await
        .map_err(|e| format!("reconnect after partial deadline: {e}"))??;
    quiet(&mut peer).await?;
    assert_eq!(count(&path, "machine_orders")?, 1);
    assert_eq!(count(&path, "machine_order_items")?, 2);
    // EOF/reconnect observes transport closure, not completion of evidence custody.
    wait_parked_frame(&path, &partial, "incomplete block")
        .await
        .map_err(|e| format!("partial parking: {e}"))?;
    let retained: (String, Vec<u8>) = database(&path)?.query_row(
        "SELECT id,raw FROM receipts WHERE raw=?1",
        [&partial[..]],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    assert_eq!(retained.1, partial);
    assert!(count(&path, "receipts")? >= 2);
    let evidence: String = database(&path)?.query_row(
        "SELECT evidence FROM parked WHERE json_extract(evidence,'$.targets[0]')=?1",
        [serde_json::from_str::<String>(&retained.0)?],
        |r| r.get(0),
    )?;
    let parked: serde_json::Value = serde_json::from_str(&evidence)?;
    assert_eq!(parked["reason"], "incomplete block");
    running.stop().await?;
    let mut loaded = Program::load(&path)?;
    loaded.set_loopback_destination_for_test("demo", reference.url.clone())?;
    let running = Running::start_program(loaded, "synthetic-api").await?;
    let (mut peer, _) = tokio::time::timeout(HANG_GUARD, listener.accept())
        .await
        .map_err(|e| format!("replacement accept: {e}"))??;
    quiet(&mut peer).await?;
    let after: Vec<u8> =
        database(&path)?.query_row("SELECT raw FROM receipts WHERE id=?1", [&retained.0], |r| {
            r.get(0)
        })?;
    assert_eq!(after, partial);
    running.stop().await?;
    reference.close().await?;
    Ok(())
}

#[tokio::test]
async fn law_tf2b_3_27_3_28_single_client_uses_configured_query_and_orl_deadlines()
-> Result<(), Error> {
    let settings = definition()
        .replace("send_ahead = false", "send_ahead = true")
        .replace("response_ms = 30000", "response_ms = 150")
        .replace("query_deadline_ms = 30000", "query_deadline_ms = 50");
    let (_temp, path, reference, running, mut peer, _listener) = client(&settings).await?;
    packet(&mut peer).await?;
    // A missing ORL uses response_ms, not the much longer transport timeout.
    // Deliberate upper bound on configured ORL response expiry.
    tokio::time::timeout(
        Duration::from_secs(2),
        vendor_support::state(&path, "Uncertain"),
    )
    .await??;
    let request = analyte_framing::hl7::encode(
        query(ALL, "late-client").as_bytes(),
        analyte_framing::hl7::Limit::new(1048576)?,
    )?;
    let (first, rest) = request.split_at(12);
    peer.write_all(first).await?;
    // The deadline runs from the first byte the program reads. A gap far beyond
    // the 50 ms budget stays late even if a slow runner reads the first part late.
    tokio::time::sleep(Duration::from_secs(2)).await;
    peer.write_all(rest).await?;
    let rsp = packet(&mut peer).await?;
    assert_eq!(field(&rsp, b"MSA", 1)?, b"AE");
    assert_eq!(field(&rsp, b"QAK", 2)?, b"AE");
    assert_eq!(count(&path, "machine_orders")?, 1);
    assert_eq!(count(&path, "machine_order_items")?, 2);
    quiet(&mut peer).await?;
    running.stop().await?;
    reference.close().await?;
    Ok(())
}

fn independent(settings: &str, endpoint: std::net::SocketAddr) -> String {
    settings.replace(
        "connection_mode = \"persistent\"",
        &format!(
            "connection_mode = \"independent_per_transaction\"\noutbound_endpoint = \"{endpoint}\""
        ),
    )
}

// PaLM TF-2b Rev 11 B.6 pp.79-80; ADR 0003 rules 1, 8, 9.
#[tokio::test]
async fn law_tf2b_b6_independent_query_followup_survives_inbound_close_and_restarts()
-> Result<(), Error> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let settings = independent(&definition(), listener.local_addr()?);
    let (_temp, path, mut reference, running, mut peer) = setup(
        vec![order("order-1", "000Ab12", Some("law"), 1)?],
        &settings,
        false,
    )
    .await?;
    assert!(
        // Deliberate quiet window: no outbound connection may be opened.
        tokio::time::timeout(Duration::from_millis(100), listener.accept())
            .await
            .is_err(),
        "outbound link is established only for a transaction"
    );
    send(&mut peer, query(CONTAINER, "independent").as_bytes()).await?;
    let rsp = packet(&mut peer).await?;
    assert_eq!(field(&rsp, b"QAK", 2)?, b"OK");
    let mut other = connect(&path, "law").await?;
    let malformed = query(CONTAINER, "parallel").replace("QPD|", "QPD|BROKEN");
    send(&mut other, malformed.as_bytes()).await?;
    assert_eq!(field(&packet(&mut other).await?, b"MSA", 1)?, b"AE");
    drop(other);
    quiet(&mut peer).await?;
    drop(peer);
    let (mut outbound, _) = tokio::time::timeout(HANG_GUARD, listener.accept()).await??;
    let work = packet(&mut outbound).await?;
    assert_eq!(field(&work, b"MSH", 9)?, b"OML^O33^OML_O33");
    orl(&mut outbound, &work, &["OK"], false).await?;
    // PaLM TF-2b B.6: the manager owns transaction closure; peer stays open.
    assert_eq!(
        tokio::time::timeout(HANG_GUARD, outbound.read_u8())
            .await?
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::UnexpectedEof
    );
    drop(outbound);
    vendor_support::state(&path, "Accepted").await?;
    wait_state(&path, "done", 1).await?;
    let awos = std::str::from_utf8(field(&work, b"OBR", 2)?)?;
    let mut result_peer = connect(&path, "law").await?;
    let result = law_results::result(
        "independent-result",
        &format!("{awos}^LAB"),
        "",
        "12.30",
        "F",
    )
    .replace("00012a^LAB", "000Ab12^LAB");
    send(&mut result_peer, result.as_bytes()).await?;
    assert_eq!(field(&packet(&mut result_peer).await?, b"MSA", 1)?, b"AA");
    reference.event().await?;
    wait_state(&path, "done", 2).await?;
    drop(result_peer);
    let mut peer = connect(&path, "law").await?;
    send(&mut peer, query(CONTAINER, "empty").as_bytes()).await?;
    assert_eq!(field(&packet(&mut peer).await?, b"QAK", 2)?, b"OK");
    drop(peer);
    let (mut outbound, _) = tokio::time::timeout(HANG_GUARD, listener.accept()).await??;
    let negative = packet(&mut outbound).await?;
    assert!(!negative.segments().any(|s| s.id() == b"OBR"));
    orl(&mut outbound, &negative, &[], false).await?;
    // PaLM TF-2b B.6: the manager owns transaction closure; peer stays open.
    assert_eq!(
        tokio::time::timeout(HANG_GUARD, outbound.read_u8())
            .await?
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::UnexpectedEof
    );
    drop(outbound);
    wait_audit(&path).await?;
    running.stop().await?;
    let running = Running::start(&path, &reference.url, "synthetic-api").await?;
    assert!(
        // Deliberate quiet window: no outbound connection may be opened.
        tokio::time::timeout(Duration::from_millis(250), listener.accept())
            .await
            .is_err(),
        "restart must not replay work"
    );
    running.stop().await?;
    reference.close().await?;
    Ok(())
}

// B.6 pp.79-80; ADR 0003 rule 8: each reservation has only one issuance attempt.
#[tokio::test]
async fn law_tf2b_b6_independent_failed_link_or_lost_orl_never_replays_on_restart()
-> Result<(), Error> {
    for failure in ["unreachable", "disconnect", "timeout"] {
        let unreachable = failure == "unreachable";
        // A bound, non-listening socket refuses connections while reserving the
        // address for restart. Releasing it lets parallel tests claim the port.
        let socket = tokio::net::TcpSocket::new_v4()?;
        socket.bind("127.0.0.1:0".parse()?)?;
        let endpoint = socket.local_addr()?;
        let mut reserved = None;
        let listener = if unreachable {
            reserved = Some(socket);
            None
        } else {
            Some(socket.listen(1024)?)
        };
        let settings = independent(&definition(), endpoint);
        let settings = if failure == "timeout" {
            settings.replace("response_ms = 30000", "response_ms = 100")
        } else {
            settings
        };
        let (temp, path, reference, running, mut peer) = setup(
            vec![order("order-1", "000Ab12", Some("law"), 1)?],
            &settings,
            false,
        )
        .await?;
        send(&mut peer, query(CONTAINER, "uncertain").as_bytes()).await?;
        assert_eq!(field(&packet(&mut peer).await?, b"QAK", 2)?, b"OK");
        drop(peer);
        let mut waiting_peer = None;
        if let Some(listener) = &listener {
            let (mut outbound, _) = tokio::time::timeout(HANG_GUARD, listener.accept()).await??;
            let work = packet(&mut outbound).await?;
            assert_eq!(field(&work, b"MSH", 9)?, b"OML^O33^OML_O33");
            if failure == "timeout" {
                waiting_peer = Some(outbound);
            } else {
                drop(outbound);
            }
        }
        vendor_support::state(&path, "Uncertain").await?;
        if let Some(mut waiting_peer) = waiting_peer {
            assert!(
                tokio::time::timeout(HANG_GUARD, waiting_peer.read_u8())
                    .await?
                    .is_err(),
                "expired transaction socket must close without another command"
            );
        }
        assert_eq!(count(&path, "machine_orders")?, 1);
        assert_eq!(count(&path, "machine_order_items")?, 1);
        running.stop().await?;
        let listener = match listener {
            Some(listener) => listener,
            None => reserved
                .take()
                .ok_or("reserved socket missing")?
                .listen(1024)?,
        };
        std::fs::write(
            temp.path().join("hl7.toml"),
            settings.replace("send_ahead = false", "send_ahead = true"),
        )?;
        let running = Running::start(&path, &reference.url, "synthetic-api").await?;
        assert!(
            // Deliberate quiet window: no outbound connection may be opened.
            tokio::time::timeout(Duration::from_millis(500), listener.accept())
                .await
                .is_err(),
            "a reserved command must never be retried"
        );
        vendor_support::state(&path, "Uncertain").await?;
        assert_eq!(count(&path, "machine_orders")?, 1);
        running.stop().await?;
        reference.close().await?;
    }
    Ok(())
}

#[tokio::test]
async fn law_tf2b_b6_adr0003_independent_restore_pause_sends_no_work() -> Result<(), Error> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let settings = independent(
        &definition().replace("send_ahead = false", "send_ahead = true"),
        listener.local_addr()?,
    );
    let (_temp, path, reference, running, mut peer) = setup(
        vec![order("order-1", "000Ab12", Some("law"), 1)?],
        &settings,
        true,
    )
    .await?;
    send(&mut peer, query(CONTAINER, "restore").as_bytes()).await?;
    let rsp = packet(&mut peer).await?;
    assert_eq!(field(&rsp, b"MSA", 1)?, b"AE");
    assert_eq!(field(&rsp, b"QAK", 2)?, b"AE");
    assert_eq!(count(&path, "machine_orders")?, 0);
    assert!(
        // Deliberate quiet window: no outbound connection may be opened.
        tokio::time::timeout(Duration::from_millis(250), listener.accept())
            .await
            .is_err()
    );
    running.stop().await?;
    reference.close().await?;
    Ok(())
}

// B.6 and 3.28.4/C.8: send-ahead/cancel share the same reserved outbound owner.
#[tokio::test]
async fn law_tf2b_b6_independent_send_ahead_and_cancel_keep_original_awos() -> Result<(), Error> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let settings = independent(
        &definition().replace("send_ahead = false", "send_ahead = true"),
        listener.local_addr()?,
    );
    let temp = tempfile::tempdir()?;
    let path = vendor_config(temp.path(), "law", &settings)?;
    let reference = Reference::start(
        &temp.path().join("receiver.db"),
        TcpListener::bind("127.0.0.1:0").await?,
        false,
    )
    .await?;
    reference
        .receiver
        .seed(vec![order("order-1", "000Ab12", Some("law"), 1)?])
        .await?;
    let mut loaded = program(&path, &reference.url)?;
    loaded.set_loopback_poll_interval_for_test("demo", Duration::from_millis(100))?;
    let running = Running::start_program(loaded, "synthetic-api").await?;
    // B.6: no inbound analyzer connection has ever been established.
    let (mut outbound, _) = tokio::time::timeout(HANG_GUARD, listener.accept()).await??;
    let sent = packet(&mut outbound).await?;
    let placer = field(&sent, b"OBR", 2)?.to_vec();
    orl(&mut outbound, &sent, &["OK"], false).await?;
    // PaLM TF-2b B.6: the manager owns transaction closure; peer stays open.
    assert_eq!(
        tokio::time::timeout(HANG_GUARD, outbound.read_u8())
            .await?
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::UnexpectedEof
    );
    drop(outbound);
    vendor_support::state(&path, "Accepted").await?;
    work::cancel_cache(&path, &reference).await?;
    let (mut outbound, _) = tokio::time::timeout(HANG_GUARD, listener.accept()).await??;
    let cancel = packet(&mut outbound).await?;
    assert_eq!(field(&cancel, b"ORC", 1)?, b"CA");
    assert_eq!(field(&cancel, b"OBR", 2)?, placer);
    orl(&mut outbound, &cancel, &["CR"], false).await?;
    drop(outbound);
    vendor_support::state(&path, "CancelConfirmed").await?;
    assert_eq!(count(&path, "machine_order_items")?, 1);
    running.stop().await?;
    reference.close().await?;
    Ok(())
}

// B.6 pp.79-80; ADR 0003 rule 8: restart cannot reissue an interrupted command.
#[tokio::test]
async fn law_tf2b_b6_independent_crash_awaiting_orl_recovers_uncertain_without_resend()
-> Result<(), Error> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let settings = independent(
        &definition().replace("send_ahead = false", "send_ahead = true"),
        listener.local_addr()?,
    );
    let (_temp, path, reference, running, peer) = setup(
        vec![order("order-1", "000Ab12", Some("law"), 1)?],
        &settings,
        false,
    )
    .await?;
    drop(peer);
    let (mut outbound, _) = tokio::time::timeout(HANG_GUARD, listener.accept()).await??;
    let sent = packet(&mut outbound).await?;
    assert_eq!(field(&sent, b"MSH", 9)?, b"OML^O33^OML_O33");
    vendor_support::state(&path, "Sending").await?;
    running.crash().await?;
    tokio::time::timeout(HANG_GUARD, async {
        let mut byte = [0];
        loop {
            match outbound.read(&mut byte).await {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
        }
    })
    .await?;
    vendor_support::state(&path, "Sending").await?;
    drop(outbound);
    let running = Running::start(&path, &reference.url, "synthetic-api").await?;
    let peer = connect(&path, "law").await?;
    vendor_support::state(&path, "Uncertain").await?;
    assert!(
        // Deliberate quiet window: no outbound connection may be opened.
        tokio::time::timeout(Duration::from_millis(500), listener.accept())
            .await
            .is_err()
    );
    assert_eq!(count(&path, "machine_order_items")?, 1);
    drop(peer);
    running.stop().await?;
    reference.close().await?;
    Ok(())
}

/// IHE `PaLM` TF-2b B.6 and clinical rule 8: one dialog owner per physical connection.
#[cfg(feature = "test-hooks")]
#[tokio::test]
async fn ihe_palm_tf2b_b6_rule8_reconnect_waits_for_previous_dialog_owner() -> Result<(), Error> {
    let settings = definition().replace("send_ahead = false", "send_ahead = true");
    let (hook, control) = analyte::runtime::test_hooks::Committed::new();
    let (_temp, path, reference, running, mut peer, listener) =
        client_with_setup(&settings, |program| {
            program.pause_connection_closing_for_test("law", hook)
        })
        .await?;
    packet(&mut peer).await?;
    vendor_support::state(&path, "Sending").await?;
    drop(peer);
    control.reached().await?;
    vendor_support::state(&path, "Uncertain").await?;
    // The reconnect backoff is 100 ms. Hold the real closing owner across that
    // boundary and observe the listener, rather than sleeping before accepting.
    assert!(
        tokio::time::timeout(Duration::from_millis(500), listener.accept())
            .await
            .is_err(),
        "IHE PaLM TF-2b B.6 / rule 8: reconnect opened before the previous dialog owner released its transport"
    );
    control.resume();
    let (mut peer, _) = tokio::time::timeout(HANG_GUARD, listener.accept()).await??;
    quiet(&mut peer).await?;
    assert_eq!(count(&path, "machine_orders")?, 1);
    assert_eq!(count(&path, "machine_order_items")?, 2);
    vendor_support::state(&path, "Uncertain").await?;
    running.stop().await?;
    reference.close().await?;
    Ok(())
}
