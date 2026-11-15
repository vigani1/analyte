//! LIS02-A2 §§8,11,12 and ADR 0004, using synthetic loopback peers only.
use super::*;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

fn configure_orders(root: &Path, send: bool) -> Result<(), Error> {
    let path = root.join("astm.toml");
    let text = std::fs::read_to_string(&path)?;
    let section = include_str!("../../../config/examples/orders.toml")
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
    tokio::time::timeout(Duration::from_secs(5), async {
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

async fn wait_cache(path: &Path) -> Result<(), Error> {
    tokio::time::timeout(Duration::from_secs(5), async {
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
    let service = Running::start(&path, &reference.url, "synthetic-api")?;
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
    assert!(query(&mut peer, "absent").await?.ends_with(b"L|1|I\r"));
    database(&path)?.execute("UPDATE feed SET trusted=0", [])?;
    assert!(query(&mut peer, "absent").await?.ends_with(b"L|1|Q\r"));
    drop(peer);
    service.stop().await?;
    reference.close().await?;
    Ok(())
}

async fn machine_state(path: &Path, state: &str, count: i64) -> Result<(), Error> {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if database(path)?.query_row(
                "SELECT count(*) FROM machine_orders WHERE state=?1",
                [format!("\"{state}\"")],
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
    let service = Running::start(&path, &reference.url, "synthetic-api")?;
    let mut peer = connect(&path, "astm").await?;
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), peer.read_u8()).await??,
        5
    );
    machine_state(&path, "Uncertain", 1).await?;
    wait_state(&path, "done", 2).await?;
    assert_eq!(receiver_count(&receiver_path)?, 2);
    drop(peer);
    service.stop().await?;
    let service = Running::start(&path, &reference.url, "synthetic-api")?;
    let mut peer = connect(&path, "astm").await?;
    assert!(
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
            .arg(&path)
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
    let service = Running::start(&path, &reference.url, "synthetic-api")?;
    wait_state(&path, "done", 3).await?;
    assert_eq!(receiver_count(&receiver_path)?, 3);
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
    let service = Running::start(&path, &reference.url, "synthetic-api")?;
    let mut peer = connect(&path, "astm").await?;
    machine_state(&path, "Rejected", 1).await?;
    assert!(
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
    let service = Running::start(&path, &reference.url, "synthetic-api")?;
    let mut peer = connect(&path, "astm").await?;
    let received = receive_orders(&mut peer).await?;
    analyte_astm::parse(&received, analyte_astm::Limits::default())?;
    assert!(received.windows(7).any(|w| w == b"000Ab12"));
    machine_state(&path, "Sent", 1).await?;
    drop(peer);
    service.stop().await?;
    let service = Running::start(&path, &reference.url, "synthetic-api")?;
    machine_state(&path, "Uncertain", 1).await?;
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
        std::fs::read_to_string(&definition)?.replace("deadline_ms = 1000", "deadline_ms = 20"),
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
    let service = Running::start(&path, &reference.url, "synthetic-api")?;
    let mut peer = connect(&path, "astm").await?;
    wait_cache(&path).await?;
    peer.write_all(b"\x05").await?;
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), peer.read_u8()).await??,
        6
    );
    tokio::time::sleep(Duration::from_millis(40)).await;
    let request = b"H|\\^&|||SYN|||||||P|1\rQ|1|^000Ab12||ALL||||||||O\rL|1|N\r";
    for frame in analyte_framing::astm::encode(
        request,
        analyte_framing::astm::FrameNumber::FIRST,
        analyte_framing::astm::Limit::new(64000)?,
    )? {
        peer.write_all(&frame).await?;
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), peer.read_u8()).await??,
            6
        );
    }
    peer.write_all(b"\x04").await?;
    assert!(receive_orders(&mut peer).await?.ends_with(b"L|1|Q\r"));
    astm(
        &mut peer,
        b"H|\\^&|||SYN|||||||P|1\rQ|1|^000Ab12||||||||||A\rL|1|N\r",
    )
    .await?;
    assert!(
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
    let service = Running::start(&path, &reference.url, "synthetic-api")?;
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
    let service = Running::start(&path, &reference.url, "synthetic-api")?;
    let mut peer = connect(&path, "astm").await?;
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), peer.read_u8()).await??,
        5
    );
    drop(peer);
    machine_state(&path, "Uncertain", 1).await?;
    wait_state(&path, "done", 2).await?;
    service.stop().await?;
    reference.close().await?;
    Ok(())
}
