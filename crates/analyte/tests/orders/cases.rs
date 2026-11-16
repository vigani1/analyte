//! Tests ASTM query and send-ahead order exchanges.
//! Kept whole: ASTM order context cases share one query and send-ahead exchange fixture.
use super::*;
use crate::support::HANG_GUARD;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

fn configure_orders(root: &Path, send: bool) -> Result<(), Error> {
    let path = root.join("astm.toml");
    let text = std::fs::read_to_string(&path)?;
    // Normal query outcomes must not depend on a loaded runner meeting the teaching deadline.
    let section = include_str!("../../../core/config/examples/orders.toml")
        .replace("deadline_ms = 1000", "deadline_ms = 30000")
        .replace("send_ahead = true", &format!("send_ahead = {send}"))
        .replace(
            "namespace=\"demo\",value=\"DEMO\"",
            "namespace=\"synthetic\",value=\"TEST\"",
        );
    std::fs::write(path, format!("{text}\n{section}"))?;
    Ok(())
}

async fn query(peer: &mut TcpStream, sample: &str) -> Result<Vec<u8>, Error> {
    astm(
        peer,
        format!("H|\\^&|||SYN|||||||P|1\rQ|1|^{sample}||ALL||||||||O\rL|1|N\r").as_bytes(),
    )
    .await?;
    receive_orders(peer).await
}

async fn receive_orders(peer: &mut TcpStream) -> Result<Vec<u8>, Error> {
    tokio::time::timeout(HANG_GUARD, async {
        assert_eq!(peer.read_u8().await?, 5);
        peer.write_all(b"\x06").await?;
        let mut payload = Vec::new();
        loop {
            let byte = peer.read_u8().await?;
            if byte == 4 {
                return Ok::<_, Error>(payload);
            }
            assert_eq!(byte, 2);
            let mut frame = vec![byte];
            while !frame.ends_with(b"\r\n") {
                if frame.len() > 64000 {
                    return Err("oversized frame".into());
                }
                frame.push(peer.read_u8().await?);
            }
            let end = frame
                .iter()
                .position(|b| matches!(b, 3 | 23))
                .ok_or("missing frame ending")?;
            payload.extend_from_slice(frame.get(2..end).ok_or("invalid frame")?);
            if payload.len() > 1_048_576 {
                return Err("oversized reply".into());
            }
            peer.write_all(b"\x06").await?;
        }
    })
    .await?
}

/// Waits until a cached order exists and a complete, trusted feed pass has been
/// committed. A row alone can precede that commit; a query answered in between
/// is correctly refused as untrusted, which made these tests race on slow hosts.
async fn wait_cache(path: &Path) -> Result<(), Error> {
    wait_feed(path).await?;
    tokio::time::timeout(HANG_GUARD, async {
        loop {
            if database(path)?
                .query_row("SELECT count(*) FROM orders", [], |r| r.get::<_, i64>(0))?
                > 0
            {
                return Ok::<_, Error>(());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?
}

#[tokio::test]
async fn lis02_section11_fresh_cache_orders_empty_and_untrusted_are_distinct_on_wire()
-> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let path = config(temp.path(), true)?;
    configure_orders(temp.path(), false)?;
    let reference = Reference::start(
        &temp.path().join("receiver.db"),
        TcpListener::bind("127.0.0.1:0").await?,
        false,
    )
    .await?;
    reference
        .receiver
        .seed(vec![analyte_contract::decode(include_bytes!(
            "../../../../docs/contract/vectors/order.json"
        ))?])
        .await?;
    let service = Running::start(&path, &reference.url, "synthetic-api").await?;
    let mut peer = connect(&path, "astm").await?;
    wait_cache(&path).await?;
    let reply = query(&mut peer, "000Ab12").await?;
    analyte_astm::parse(&reply, analyte_astm::Limits::default())?;
    assert!(
        reply
            .windows(b"O|1|000Ab12||^^^SEND|R".len())
            .any(|w| w == b"O|1|000Ab12||^^^SEND|R")
    );
    assert!(reply.ends_with(b"L|1|N\r"));
    machine_state(&path, "Sent", 1).await?;
    wait_state(&path, "done", 1).await?;
    assert!(query(&mut peer, "absent").await?.ends_with(b"L|1|I\r"));
    database(&path)?.execute("UPDATE feed SET trusted=0", [])?;
    assert!(query(&mut peer, "absent").await?.ends_with(b"L|1|Q\r"));
    assert_eq!(
        database(&path)?.query_row("SELECT count(*) FROM outbox", [], |r| r.get::<_, i64>(0))?,
        1
    );
    drop(peer);
    service.stop().await?;
    reference.close().await?;
    Ok(())
}

#[tokio::test]
async fn rule8_query_response_reserves_without_event_then_disconnect_emits_one_uncertain()
-> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let path = config(temp.path(), true)?;
    configure_orders(temp.path(), false)?;
    let reference = Reference::start(
        &temp.path().join("receiver.db"),
        TcpListener::bind("127.0.0.1:0").await?,
        false,
    )
    .await?;
    reference
        .receiver
        .seed(vec![analyte_contract::decode(include_bytes!(
            "../../../../docs/contract/vectors/order.json"
        ))?])
        .await?;
    let service = Running::start(&path, &reference.url, "synthetic-api").await?;
    let mut peer = connect(&path, "astm").await?;
    wait_cache(&path).await?;
    astm(
        &mut peer,
        b"H|\\^&|||SYN|||||||P|1\rQ|1|^000Ab12||ALL||||||||O\rL|1|N\r",
    )
    .await?;
    assert_eq!(tokio::time::timeout(HANG_GUARD, peer.read_u8()).await??, 5);
    machine_state(&path, "Sending", 1).await?;
    assert_eq!(
        database(&path)?.query_row("SELECT count(*) FROM outbox", [], |r| r.get::<_, i64>(0))?,
        0
    );
    drop(peer);
    machine_state(&path, "Uncertain", 1).await?;
    wait_state(&path, "done", 1).await?;
    service.stop().await?;
    reference.close().await?;
    Ok(())
}

async fn machine_state(path: &Path, state: &str, count: i64) -> Result<(), Error> {
    tokio::time::timeout(HANG_GUARD, async {
        loop {
            if database(path)?.query_row(
                "SELECT count(*) FROM machine_orders WHERE state=?1",
                [state],
                |r| r.get::<_, i64>(0),
            )? == count
            {
                return Ok::<_, Error>(());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?
}

#[tokio::test]
async fn adr0004_rule8_astm_send_timeout_is_uncertain_and_restart_never_resends()
-> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let path = config(temp.path(), true)?;
    configure_orders(temp.path(), true)?;
    let definition = temp.path().join("astm.toml");
    std::fs::write(
        &definition,
        std::fs::read_to_string(&definition)?
            .replace("send_timeout_ms = 15000", "send_timeout_ms = 50").replace("deviations = []", "deviations = [{name=\"synthetic-fast-timeout\",key=\"dialog.send_timeout_ms\",reason=\"Shortened loopback failure-path timer\"}]"),
    )?;
    let receiver_path = temp.path().join("receiver.db");
    let reference = Reference::start(
        &receiver_path,
        TcpListener::bind("127.0.0.1:0").await?,
        false,
    )
    .await?;
    reference
        .receiver
        .seed(vec![analyte_contract::decode(include_bytes!(
            "../../../../docs/contract/vectors/order.json"
        ))?])
        .await?;
    let service = Running::start(&path, &reference.url, "synthetic-api").await?;
    let mut peer = connect(&path, "astm").await?;
    assert_eq!(tokio::time::timeout(HANG_GUARD, peer.read_u8()).await??, 5);
    machine_state(&path, "Uncertain", 1).await?;
    wait_state(&path, "done", 1).await?;
    assert_eq!(receiver_count(&receiver_path)?, 1);
    drop(peer);
    service.stop().await?;
    let service = Running::start(&path, &reference.url, "synthetic-api").await?;
    let mut peer = connect(&path, "astm").await?;
    assert!(
        // Deliberate quiet window: no reply may arrive.
        tokio::time::timeout(Duration::from_millis(1200), peer.read_u8())
            .await
            .is_err()
    );
    machine_state(&path, "Uncertain", 1).await?;
    drop(peer);
    service.stop().await?;
    let id: i64 = database(&path)?.query_row("SELECT id FROM machine_orders", [], |r| r.get(0))?;
    for arguments in [
        vec!["orders".into(), "list".into()],
        vec![
            "orders".into(),
            "reconcile".into(),
            id.to_string(),
            "--as".into(),
            "accepted".into(),
            "--reason".into(),
            "verified synthetic work list".into(),
        ],
    ] {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_analyte"))
            .arg("--config")
            .arg(path.as_os_str())
            .arg("--actor")
            .arg("synthetic-operator")
            .args(arguments)
            .output()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    machine_state(&path, "Accepted", 1).await?;
    let audits: Vec<String> = database(&path)?.prepare("SELECT evidence FROM audit WHERE evidence LIKE '%synthetic-operator%' ORDER BY sequence")?.query_map([],|r|r.get(0))?.collect::<Result<_,_>>()?;
    assert_eq!(audits.len(), 2);
    assert!(audits.first().is_some_and(|s| s.contains("orders-list")));
    assert!(
        audits
            .last()
            .is_some_and(|s| s.contains("verified synthetic work list"))
    );
    let service = Running::start(&path, &reference.url, "synthetic-api").await?;
    wait_state(&path, "done", 2).await?;
    assert_eq!(receiver_count(&receiver_path)?, 2);
    service.stop().await?;
    reference.close().await?;
    Ok(())
}

#[tokio::test]
async fn adr0004_unmapped_and_cancelled_orders_are_never_sent_and_rejection_reaches_app()
-> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let path = config(temp.path(), true)?;
    configure_orders(temp.path(), true)?;
    let receiver_path = temp.path().join("receiver.db");
    let reference = Reference::start(
        &receiver_path,
        TcpListener::bind("127.0.0.1:0").await?,
        false,
    )
    .await?;
    let source = include_str!("../../../../docs/contract/vectors/order.json");
    let unmapped = analyte_contract::decode(source.replace("TEST", "UNKNOWN").as_bytes())?;
    let cancelled = analyte_contract::decode(
        source
            .replace("order-1", "cancelled")
            .replace("requested", "cancel_requested")
            .as_bytes(),
    )?;
    reference.receiver.seed(vec![unmapped, cancelled]).await?;
    let service = Running::start(&path, &reference.url, "synthetic-api").await?;
    let mut peer = connect(&path, "astm").await?;
    machine_state(&path, "Rejected", 1).await?;
    assert!(
        // Deliberate quiet window: no reply may arrive.
        tokio::time::timeout(Duration::from_millis(1200), peer.read_u8())
            .await
            .is_err()
    );
    wait_state(&path, "done", 1).await?;
    assert_eq!(receiver_count(&receiver_path)?, 1);
    let code: String =
        database(&path)?.query_row("SELECT source_code FROM machine_orders", [], |r| r.get(0))?;
    assert!(code.contains("unmapped test"));
    assert!(code.contains("UNKNOWN"));
    drop(peer);
    service.stop().await?;
    reference.close().await?;
    Ok(())
}

#[tokio::test]
async fn lis01_transfer_ack_is_sent_not_application_acceptance_and_disconnect_is_uncertain()
-> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let path = config(temp.path(), true)?;
    configure_orders(temp.path(), true)?;
    let reference = Reference::start(
        &temp.path().join("receiver.db"),
        TcpListener::bind("127.0.0.1:0").await?,
        false,
    )
    .await?;
    reference
        .receiver
        .seed(vec![analyte_contract::decode(include_bytes!(
            "../../../../docs/contract/vectors/order.json"
        ))?])
        .await?;
    let service = Running::start(&path, &reference.url, "synthetic-api").await?;
    let mut peer = connect(&path, "astm").await?;
    let received = receive_orders(&mut peer).await?;
    analyte_astm::parse(&received, analyte_astm::Limits::default())?;
    assert!(received.windows(7).any(|w| w == b"000Ab12"));
    machine_state(&path, "Sent", 1).await?;
    drop(peer);
    service.stop().await?;
    let service = Running::start(&path, &reference.url, "synthetic-api").await?;
    machine_state(&path, "Sent", 1).await?;
    wait_state(&path, "done", 1).await?;
    assert_eq!(
        database(&path)?.query_row("SELECT count(*) FROM outbox", [], |r| r.get::<_, i64>(0))?,
        1
    );
    service.stop().await?;
    reference.close().await?;
    Ok(())
}

#[tokio::test]
async fn lis02_section11_deadline_includes_transfer_time_and_cancel_gets_no_order_response()
-> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let path = config(temp.path(), true)?;
    configure_orders(temp.path(), false)?;
    let definition = temp.path().join("astm.toml");
    std::fs::write(
        &definition,
        std::fs::read_to_string(&definition)?.replace("deadline_ms = 30000", "deadline_ms = 20"),
    )?;
    let reference = Reference::start(
        &temp.path().join("receiver.db"),
        TcpListener::bind("127.0.0.1:0").await?,
        false,
    )
    .await?;
    reference
        .receiver
        .seed(vec![analyte_contract::decode(include_bytes!(
            "../../../../docs/contract/vectors/order.json"
        ))?])
        .await?;
    let service = Running::start(&path, &reference.url, "synthetic-api").await?;
    let mut peer = connect(&path, "astm").await?;
    wait_cache(&path).await?;
    peer.write_all(b"\x05").await?;
    assert_eq!(tokio::time::timeout(HANG_GUARD, peer.read_u8()).await??, 6);
    tokio::time::sleep(Duration::from_millis(40)).await;
    let request = b"H|\\^&|||SYN|||||||P|1\rQ|1|^000Ab12||ALL||||||||O\rL|1|N\r";
    let mut number = analyte_framing::astm::FrameNumber::FIRST;
    let limit = analyte_framing::astm::Limit::new(64000)?;
    for record in request.split_inclusive(|b| *b == b'\r') {
        for frame in analyte_framing::astm::encode(record, number, limit)? {
            number = number.next();
            peer.write_all(&frame).await?;
            assert_eq!(tokio::time::timeout(HANG_GUARD, peer.read_u8()).await??, 6);
        }
    }
    peer.write_all(b"\x04").await?;
    assert!(receive_orders(&mut peer).await?.ends_with(b"L|1|Q\r"));
    astm(
        &mut peer,
        b"H|\\^&|||SYN|||||||P|1\rQ|1|^000Ab12||||||||||A\rL|1|N\r",
    )
    .await?;
    assert!(
        // Deliberate quiet window: no reply may arrive.
        tokio::time::timeout(Duration::from_millis(100), peer.read_u8())
            .await
            .is_err()
    );
    drop(peer);
    service.stop().await?;
    reference.close().await?;
    Ok(())
}

#[tokio::test]
async fn adr0004_required_position_absent_rejects_query_order_and_delivers_specific_event()
-> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let path = config(temp.path(), true)?;
    configure_orders(temp.path(), false)?;
    let definition = temp.path().join("astm.toml");
    let mut text = std::fs::read_to_string(&definition)?;
    text.push_str("\nposition = {group=\"order\",record=\"O\",field=4,component=1,repetition=1}\n");
    std::fs::write(&definition, text)?;
    let receiver_path = temp.path().join("receiver.db");
    let reference = Reference::start(
        &receiver_path,
        TcpListener::bind("127.0.0.1:0").await?,
        false,
    )
    .await?;
    reference
        .receiver
        .seed(vec![analyte_contract::decode(include_bytes!(
            "../../../../docs/contract/vectors/order.json"
        ))?])
        .await?;
    let service = Running::start(&path, &reference.url, "synthetic-api").await?;
    let mut peer = connect(&path, "astm").await?;
    wait_cache(&path).await?;
    assert!(query(&mut peer, "000Ab12").await?.ends_with(b"L|1|Q\r"));
    machine_state(&path, "Rejected", 1).await?;
    wait_state(&path, "done", 1).await?;
    let reason: String =
        database(&path)?.query_row("SELECT source_code FROM machine_orders", [], |r| r.get(0))?;
    assert!(reason.contains("required specimen position was not supplied"));
    assert_eq!(receiver_count(&receiver_path)?, 1);
    drop(peer);
    service.stop().await?;
    reference.close().await?;
    Ok(())
}

#[tokio::test]
async fn adr0004_disconnect_before_astm_ack_preserves_uncertain_outcome_and_events()
-> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let path = config(temp.path(), true)?;
    configure_orders(temp.path(), true)?;
    let reference = Reference::start(
        &temp.path().join("receiver.db"),
        TcpListener::bind("127.0.0.1:0").await?,
        false,
    )
    .await?;
    reference
        .receiver
        .seed(vec![analyte_contract::decode(include_bytes!(
            "../../../../docs/contract/vectors/order.json"
        ))?])
        .await?;
    let service = Running::start(&path, &reference.url, "synthetic-api").await?;
    let mut peer = connect(&path, "astm").await?;
    assert_eq!(tokio::time::timeout(HANG_GUARD, peer.read_u8()).await??, 5);
    drop(peer);
    machine_state(&path, "Uncertain", 1).await?;
    wait_state(&path, "done", 1).await?;
    service.stop().await?;
    reference.close().await?;
    Ok(())
}

// LIS02 section 8.4.4 and section 8.4.16.1; explicit synthetic destinations.
async fn astm_context_exchange(
    send: bool,
    destinations: bool,
    specimen: Option<&str>,
    expected: &[u8],
    rejection: Option<&str>,
) -> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let path = config(temp.path(), true)?;
    configure_orders(temp.path(), send)?;
    let definition = temp.path().join("astm.toml");
    let mut text=std::fs::read_to_string(&definition)?.replace("[orders.query]","specimen_types=[{app={namespace='synthetic',value='SERUM'},machine='SER'}]\n[orders.query]");
    if destinations {
        text.push_str("\nposition={group='order',record='O',field=4,component=2,repetition=1}\nspecimen_type={group='order',record='O',field=16,component=1,repetition=1}\n");
    }
    std::fs::write(definition, text)?;
    let receiver_path = temp.path().join("receiver.db");
    let reference = Reference::start(
        &receiver_path,
        TcpListener::bind("127.0.0.1:0").await?,
        false,
    )
    .await?;
    let value = context_order_value(specimen)?;
    reference
        .receiver
        .seed(vec![analyte_contract::decode(&serde_json::to_vec(
            &value,
        )?)?])
        .await?;
    let service = Running::start(&path, &reference.url, "synthetic-api").await?;
    let mut peer = connect(&path, "astm").await?;
    wait_cache(&path).await?;
    if !send {
        assert_astm_context(&query(&mut peer, "000Ab12").await?, expected)?;
    } else if rejection.is_none() {
        assert_astm_context(&receive_orders(&mut peer).await?, expected)?;
    }
    if let Some(code) = rejection {
        machine_state(&path, "Rejected", 1).await?;
        assert!(
            // Deliberate quiet window: no reply may arrive.
            tokio::time::timeout(Duration::from_millis(1200), peer.read_u8())
                .await
                .is_err()
        );
        wait_state(&path, "done", 1).await?;
        let bytes: Vec<u8> = rusqlite::Connection::open(&receiver_path)?.query_row(
            "SELECT body FROM deliveries WHERE kind='order-events'",
            [],
            |r| r.get(0),
        )?;
        let body: serde_json::Value = serde_json::from_slice(&bytes)?;
        assert_eq!(
            body.get("state").and_then(serde_json::Value::as_str),
            Some("rejected")
        );
        assert_eq!(
            body.get("source_status")
                .and_then(serde_json::Value::as_str),
            Some(code)
        );
    } else {
        machine_state(&path, "Sent", 1).await?;
        wait_state(&path, "done", 1).await?;
    }
    assert_eq!(receiver_count(&receiver_path)?, 1);
    assert_eq!(
        database(&path)?.query_row("SELECT count(*) FROM outbox", [], |r| r.get::<_, i64>(0))?,
        1
    );
    drop(peer);
    service.stop().await?;
    reference.close().await?;
    Ok(())
}
#[tokio::test]
async fn lis02_section8_o_record_send_ahead_and_query_forward_configured_position_and_specimen_exactly()
-> Result<(), Error> {
    for send in [true, false] {
        astm_context_exchange(
            send,
            true,
            Some("SERUM"),
            b"H|\\^&|||Analyte|||||||P|LIS2-A2\rP|1\rO|1|000Ab12|^03|^^^SEND|R||||||N||||SER\rL|1|N\r",
            None,
        )
        .await?;
    }
    Ok(())
}
#[tokio::test]
async fn lis02_section8_o_record_no_destinations_leave_context_empty_even_when_supplied_unknown()
-> Result<(), Error> {
    for send in [true, false] {
        astm_context_exchange(
            send,
            false,
            Some("UNKNOWN"),
            b"H|\\^&|||Analyte|||||||P|LIS2-A2\rP|1\rO|1|000Ab12||^^^SEND|R||||||N\rL|1|N\r",
            None,
        )
        .await?;
    }
    Ok(())
}
#[tokio::test]
async fn lis02_section8_o_record_unmapped_specimen_rejects_send_ahead_and_query_without_partial_order()
-> Result<(), Error> {
    for send in [true, false] {
        astm_context_exchange(
            send,
            true,
            Some("UNKNOWN"),
            b"H|\\^&|||Analyte|||||||P|LIS2-A2\rL|1|Q\r",
            Some("rejected: unmapped-specimen-type"),
        )
        .await?;
    }
    Ok(())
}
#[tokio::test]
async fn lis02_section8_o_record_missing_required_specimen_rejects_without_guessing()
-> Result<(), Error> {
    for send in [true, false] {
        astm_context_exchange(
            send,
            true,
            None,
            b"H|\\^&|||Analyte|||||||P|LIS2-A2\rL|1|Q\r",
            Some("rejected: missing-specimen-type"),
        )
        .await?;
    }
    Ok(())
}

#[tokio::test]
async fn plan11_rule8_restore_blocks_astm_send_ahead_and_query_orders_on_wire() -> Result<(), Error>
{
    for send in [true, false] {
        let temp = tempfile::tempdir()?;
        let path = config(temp.path(), true)?;
        configure_orders(temp.path(), send)?;
        let reference = Reference::start(
            &temp.path().join("receiver.db"),
            TcpListener::bind("127.0.0.1:0").await?,
            false,
        )
        .await?;
        reference
            .receiver
            .seed(vec![analyte_contract::decode(include_bytes!(
                "../../../../docs/contract/vectors/order.json"
            ))?])
            .await?;
        // Initialize and stop before restoring, so no machine has received work.
        let service = Running::start(&path, &reference.url, "synthetic-api").await?;
        let ready = connect(&path, "hl7").await?;
        wait_cache(&path).await?;
        drop(ready);
        let refused = std::process::Command::new(env!("CARGO_BIN_EXE_analyte"))
            .arg("--config")
            .arg(&*path)
            .args([
                "restore",
                "--from",
                "unused.db",
                "--reason",
                "synthetic refusal",
            ])
            .output()?;
        assert!(!refused.status.success());
        assert!(String::from_utf8_lossy(&refused.stderr).contains("InUse"));
        service.stop().await?;
        let backup = temp.path().join("backup.db");
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_analyte"))
            .arg("--config")
            .arg(&*path)
            .arg("backup")
            .arg(&backup)
            .output()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_analyte"))
            .arg("--config")
            .arg(&*path)
            .arg("restore")
            .arg("--from")
            .arg(&backup)
            .args(["--reason", "synthetic restore"])
            .output()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let service = Running::start(&path, &reference.url, "synthetic-api").await?;
        let mut peer = connect(&path, "astm").await?;
        wait_cache(&path).await?;
        if send {
            assert!(
                // Deliberate quiet window: no reply may arrive.
                tokio::time::timeout(Duration::from_millis(1200), peer.read_u8())
                    .await
                    .is_err()
            );
        } else {
            let reply = query(&mut peer, "000Ab12").await?;
            assert!(reply.ends_with(b"L|1|Q\r"));
            assert!(!reply.windows(2).any(|w| w == b"O|"));
        }
        assert_eq!(
            database(&path)?.query_row("SELECT count(*) FROM machine_orders", [], |r| r
                .get::<_, i64>(0))?,
            0
        );
        drop(peer);
        service.stop().await?;
        reference.close().await?;
    }
    Ok(())
}

#[tokio::test]
async fn lis02_6_12_p12_nonproduction_queries_never_reserve_or_send() -> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let path = config(temp.path(), true)?;
    configure_orders(temp.path(), false)?;
    let reference = Reference::start(
        &temp.path().join("receiver.db"),
        TcpListener::bind("127.0.0.1:0").await?,
        false,
    )
    .await?;
    reference
        .receiver
        .seed(vec![analyte_contract::decode(include_bytes!(
            "../../../../docs/contract/vectors/order.json"
        ))?])
        .await?;
    let service = Running::start(&path, &reference.url, "synthetic-api").await?;
    let mut peer = connect(&path, "astm").await?;
    wait_cache(&path).await?;
    // LIS02-A2 section 6.12 p.12: non-production queries cannot issue patient work.
    for (index, processing) in ["T", "D", "Q"].into_iter().enumerate() {
        astm(
            &mut peer,
            format!("H|\\^&|||SYN|||||||{processing}|1\rQ|1|^000Ab12||ALL||||||||O\rL|1|N\r")
                .as_bytes(),
        )
        .await?;
        let reply = receive_orders(&mut peer).await?;
        assert!(reply.ends_with(b"L|1|Q\r"));
        assert!(!reply.windows(2).any(|w| w == b"O|"));
        assert_eq!(
            database(&path)?.query_row(
                "SELECT count(*) FROM receipts WHERE length(raw)>0",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            i64::try_from(index)? + 1
        );
        assert_eq!(
            database(&path)?.query_row("SELECT count(*) FROM machine_orders", [], |r| r
                .get::<_, i64>(0))?,
            0
        );
    }
    drop(peer);
    service.stop().await?;
    reference.close().await?;
    Ok(())
}

fn assert_astm_context(actual: &[u8], expected: &[u8]) -> Result<(), Error> {
    let (header, body) = actual.split_at(
        actual
            .iter()
            .position(|b| *b == b'\r')
            .ok_or("header missing")?,
    );
    let time = header.rsplit(|b| *b == b'|').next().ok_or("time missing")?;
    assert_eq!(time.len(), 19);
    assert!(time.ends_with(b"+0000"));
    assert!(
        time.get(..14)
            .ok_or("short timestamp")?
            .iter()
            .all(u8::is_ascii_digit)
    );
    let mut normalized = header
        .get(..header.len().checked_sub(20).ok_or("short header")?)
        .ok_or("header")?
        .to_vec();
    normalized.extend_from_slice(body);
    assert_eq!(normalized, expected);
    Ok(())
}

fn context_order_value(specimen: Option<&str>) -> Result<serde_json::Value, Error> {
    let mut value: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../../../../docs/contract/vectors/order-context.json"
    ))?;
    if let Some(code) = specimen {
        *value
            .pointer_mut("/specimen_type/value")
            .ok_or("specimen value")? = code.into();
    } else {
        value
            .as_object_mut()
            .ok_or("order")?
            .remove("specimen_type");
    }
    Ok(value)
}
