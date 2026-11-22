//! Tests ASTM query and send-ahead order exchanges.
use super::*;
use crate::support::HANG_GUARD;
use analyte_fixtures::ConfigEdit as _;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

pub(super) fn configure_orders(root: &Path, send: bool) -> Result<(), Error> {
    let path = analyte_fixtures::definition_path(root, "astm");
    let text = std::fs::read_to_string(&path)?;
    // Normal query outcomes must not depend on a loaded runner meeting the teaching deadline.
    let section = include_str!("../../../core/config/examples/orders.toml")
        .rewrite("deadline_ms = 1000", "deadline_ms = 30000")
        .rewrite("send_ahead = true", &format!("send_ahead = {send}"));
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

// CLSI LIS02-A2 §11
#[tokio::test]
async fn orders_empty_and_untrusted_differ_on_wire() -> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let path = config(temp.path(), true)?;
    configure_orders(temp.path(), false)?;
    let reference = seeded_reference(temp.path()).await?;
    let service = Running::start(&path, &reference.url, "synthetic-api").await?;
    let mut peer = connect(&path, "astm").await?;
    wait_cache(&path).await?;
    let reply = query(&mut peer, "000Ab12").await?;
    analyte_astm::parse(&reply, analyte_astm::Limits::default())?;
    assert!(
        reply
            .windows(b"O|1|000Ab12||^^^TEST|R".len())
            .any(|w| w == b"O|1|000Ab12||^^^TEST|R")
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

// Safety rule 8 (ADR 0003)
#[tokio::test]
async fn query_reply_disconnect_emits_one_uncertain() -> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let path = config(temp.path(), true)?;
    configure_orders(temp.path(), false)?;
    let reference = seeded_reference(temp.path()).await?;
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

pub(super) async fn machine_state(path: &Path, state: &str, count: i64) -> Result<(), Error> {
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

// ADR 0004
#[tokio::test]
async fn foreign_and_cancelled_orders_are_never_sent() -> Result<(), Error> {
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
    // ADR 0029: a test in another code system than the machine's is refused.
    let unmapped = analyte_contract::decode(
        source
            .rewrite("\"namespace\": \"synthetic\"", "\"namespace\": \"lab-app\"")
            .as_bytes(),
    )?;
    let cancelled = analyte_contract::decode(
        source
            .rewrite("order-1", "cancelled")
            .rewrite("requested", "cancel_requested")
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
    assert!(
        code.contains("is not in this machine's code system"),
        "{code}"
    );
    assert!(code.contains("lab-app"));
    drop(peer);
    service.stop().await?;
    reference.close().await?;
    Ok(())
}

// CLSI LIS01-A2
#[tokio::test]
async fn transfer_ack_means_sent_not_accepted() -> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let path = config(temp.path(), true)?;
    configure_orders(temp.path(), true)?;
    let reference = seeded_reference(temp.path()).await?;
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

// CLSI LIS02-A2 §11
#[tokio::test]
async fn deadline_counts_transfer_and_cancel_gets_nothing() -> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let path = config(temp.path(), true)?;
    configure_orders(temp.path(), false)?;
    let definition = analyte_fixtures::definition_path(temp.path(), "astm");
    std::fs::write(
        &definition,
        std::fs::read_to_string(&definition)?.rewrite("deadline_ms = 30000", "deadline_ms = 20"),
    )?;
    let reference = seeded_reference(temp.path()).await?;
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

// ADR 0004
#[tokio::test]
async fn missing_position_rejects_query_order() -> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let path = config(temp.path(), true)?;
    configure_orders(temp.path(), false)?;
    let definition = analyte_fixtures::definition_path(temp.path(), "astm");
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

// ADR 0004
#[tokio::test]
async fn disconnect_before_ack_keeps_uncertain_outcome() -> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let path = config(temp.path(), true)?;
    configure_orders(temp.path(), true)?;
    let reference = seeded_reference(temp.path()).await?;
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

/// The one order event the receiver stored says `rejected` with `code`, and
/// names the machine model.
fn assert_rejected_event(receiver_path: &Path, code: &str) -> Result<(), Error> {
    let bytes: Vec<u8> = rusqlite::Connection::open(receiver_path)?.query_row(
        "SELECT body FROM deliveries WHERE kind='order-events'",
        [],
        |r| r.get(0),
    )?;
    let body: serde_json::Value = serde_json::from_slice(&bytes)?;
    let text = |pointer: &str| body.pointer(pointer).and_then(serde_json::Value::as_str);
    assert_eq!(text("/state"), Some("rejected"));
    assert_eq!(text("/source_status"), Some(code));
    // ADR 0033: an order event names the machine's model, its definition's folder key.
    assert_eq!(text("/source/model"), Some("test/astm"));
    Ok(())
}

// LIS02 section 8.4.4 and section 8.4.16.1; explicit synthetic destinations.
async fn astm_context_exchange(
    send: bool,
    destinations: bool,
    specimen: Option<(&str, &str)>,
    expected: &[u8],
    rejection: Option<&str>,
) -> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let path = config(temp.path(), true)?;
    configure_orders(temp.path(), send)?;
    let definition = analyte_fixtures::definition_path(temp.path(), "astm");
    let mut text = std::fs::read_to_string(&definition)?;
    // ADR 0030: the specimen code system is declared with its destination.
    if destinations {
        text = text.rewrite(
            "[orders.query]",
            "specimen_system='synthetic'\n[orders.query]",
        );
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
        assert_rejected_event(&receiver_path, code)?;
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
// CLSI LIS02-A2 §8
#[tokio::test]
async fn o_record_forwards_position_and_specimen_exactly() -> Result<(), Error> {
    for send in [true, false] {
        astm_context_exchange(
            send,
            true,
            Some(("synthetic", "SER")),
            b"H|\\^&|||Analyte|||||||P|LIS2-A2\rP|1\rO|1|000Ab12|^03|^^^TEST|R||||||N||||SER\rL|1|N\r",
            None,
        )
        .await?;
    }
    Ok(())
}
// CLSI LIS02-A2 §8
#[tokio::test]
async fn o_record_without_destinations_stays_empty() -> Result<(), Error> {
    for send in [true, false] {
        astm_context_exchange(
            send,
            false,
            Some(("other", "UNKNOWN")),
            b"H|\\^&|||Analyte|||||||P|LIS2-A2\rP|1\rO|1|000Ab12||^^^TEST|R||||||N\rL|1|N\r",
            None,
        )
        .await?;
    }
    Ok(())
}
// CLSI LIS02-A2 §8; ADR 0030
#[tokio::test]
async fn o_record_foreign_specimen_rejects_whole_order() -> Result<(), Error> {
    for send in [true, false] {
        astm_context_exchange(
            send,
            true,
            Some(("other", "SER")),
            b"H|\\^&|||Analyte|||||||P|LIS2-A2\rL|1|Q\r",
            Some("rejected: foreign-specimen-type"),
        )
        .await?;
    }
    Ok(())
}
// CLSI LIS02-A2 §6 and §8.4.16.1; ADR 0030: a specimen code holding a
// delimiter cannot be carried, so the order is refused, never escaped.
#[tokio::test]
async fn o_record_unrepresentable_specimen_rejects_whole_order() -> Result<(), Error> {
    for send in [true, false] {
        astm_context_exchange(
            send,
            true,
            Some(("synthetic", "S^R")),
            b"H|\\^&|||Analyte|||||||P|LIS2-A2\rL|1|Q\r",
            Some("rejected: ASTM text is empty or contains unsupported bytes or delimiters"),
        )
        .await?;
    }
    Ok(())
}
// CLSI LIS02-A2 §8
#[tokio::test]
async fn o_record_missing_specimen_is_rejected() -> Result<(), Error> {
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

// Plan §11; Safety rule 8 (ADR 0003)
#[tokio::test]
async fn restore_blocks_send_ahead_and_query_orders() -> Result<(), Error> {
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

// CLSI LIS02-A2 §6.12, p. 12
#[tokio::test]
async fn nonproduction_queries_never_reserve_or_send() -> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let path = config(temp.path(), true)?;
    configure_orders(temp.path(), false)?;
    let reference = seeded_reference(temp.path()).await?;
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

fn context_order_value(specimen: Option<(&str, &str)>) -> Result<serde_json::Value, Error> {
    let mut value: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../../../../docs/contract/vectors/order-context.json"
    ))?;
    if let Some((namespace, code)) = specimen {
        *value.pointer_mut("/specimen_type").ok_or("specimen type")? =
            serde_json::json!({"namespace": namespace, "value": code});
    } else {
        value
            .as_object_mut()
            .ok_or("order")?
            .remove("specimen_type");
    }
    Ok(value)
}

async fn seeded_reference(root: &Path) -> Result<Reference, Error> {
    let reference = Reference::start(
        &root.join("receiver.db"),
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
    Ok(reference)
}
