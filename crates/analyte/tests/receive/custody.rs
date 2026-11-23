//! Tests machine receipt storage and acknowledgment ordering.
use super::*;
use analyte_fixtures::ConfigEdit as _;

// Plan §11
#[tokio::test]
async fn status_reports_health_without_payloads_or_secrets() -> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let path = config(temp.path(), false)?;
    let program = Program::load(&path)?;
    let mut store = service::open_store(&program, Box::new(Space(u64::MAX))).await?;
    let lines = analyte::operator::status::snapshot(&program, &store).await?;
    assert_eq!(lines.len(), 4);
    assert_eq!(lines[0], "order_sending_paused_after_restore=false");
    assert_eq!(
        lines.get(1).ok_or("missing AWOS count")?,
        "analyzer_cancelled_awos=0"
    );
    let destination = lines.get(2).ok_or("missing health")?;
    for field in [
        "pending_count=0",
        "pending_bytes=0",
        "oldest_pending_age_seconds=None",
        "paused=false",
        "last_poll=None",
        "cache_trusted=false",
    ] {
        assert!(destination.contains(field));
    }
    assert_eq!(
        lines.last().ok_or("missing machine")?,
        "machine=synthetic last_receipt=unknown"
    );
    store
        .shutdown(
            std::time::Instant::now()
                .checked_add(analyte_fixtures::STORE_BUDGET)
                .ok_or("deadline")?,
        )
        .await?;
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_analyte"))
        .arg("status")
        .arg(&path)
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout)?;
    assert!(text.contains("destination=demo"));
    assert!(!text.contains("demo-api"));
    assert!(!text.contains("demo-signing"));
    Ok(())
}

// Plan section 11 and ADR 0003: real CLI commands require actor/reason and reuse
// the live stored-byte interpretation; neither reprocess nor resend touches orders.
// Plan §11
#[tokio::test]
async fn cli_commands_require_actor_and_reason() -> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let path = config(temp.path(), false)?;
    let definition =
        std::fs::read_to_string(analyte_fixtures::definition_path(temp.path(), "synthetic"))?;
    analyte_fixtures::write_definition(
        temp.path(),
        "synthetic",
        // A wrong order ID selector parks the result until the file is fixed.
        definition.rewrite("record=\"OBR\",field=3", "record=\"OBR\",field=2"),
    )?;
    let program = Program::load(&path)?;
    let (shutdown, stop) = oneshot::channel();
    let owner = tokio::spawn(serve(
        Program::load(&path)?,
        Box::new(Space(u64::MAX)),
        async {
            stop.await?;
            Ok(())
        },
    ));
    let mut peer = connect(&program).await?;
    peer.write_all(&analyte_framing::hl7::encode(
        ORU,
        analyte_framing::hl7::Limit::new(1048576)?,
    )?)
    .await?;
    assert!(String::from_utf8(read_ack(&mut peer).await?)?.contains("MSA|AE"));
    shutdown.send(()).map_err(|_| "shutdown lost")?;
    tokio::time::timeout(HANG_GUARD, owner).await???;
    let store = stored(&program)?;
    let receipt = store.receipts("")?.first().ok_or("receipt missing")?.id;
    let parked = store.parked(0)?.first().ok_or("parked missing")?.id;
    let raw = store.receipt(receipt)?.ok_or("receipt missing")?.raw;
    drop(store);
    analyte_fixtures::write_definition(temp.path(), "synthetic", definition)?;
    let receipt = uuid::Uuid::from(receipt).to_string();
    let parked = parked.to_string();
    for args in [
        vec!["parked", "reprocess", &parked],
        vec!["receipts", "reprocess", &receipt],
        vec!["deliveries", "list"],
    ] {
        for suffix in [
            vec!["--reason", "mapping repaired"],
            vec!["--actor", "operator"],
        ] {
            let output = std::process::Command::new(env!("CARGO_BIN_EXE_analyte"))
                .arg("--config")
                .arg(&path)
                .args(&args)
                .args(suffix)
                .output()?;
            assert!(!output.status.success());
        }
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_analyte"))
            .arg("--config")
            .arg(&path)
            .args(&args)
            .args(["--actor", "operator", "--reason", "mapping repaired"])
            .output()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        if args.first() == Some(&"deliveries") {
            assert!(!output.stdout.is_empty());
        }
    }
    let store = stored(&program)?;
    let receipt = analyte_model::ReceiptId::try_from(uuid::Uuid::parse_str(&receipt)?)?;
    assert_eq!(store.receipt(receipt)?.ok_or("receipt missing")?.raw, raw);
    assert!(
        store
            .parked_item(parked.parse()?)?
            .ok_or("parked missing")?
            .resolution
            .is_some()
    );
    assert_eq!(store.deliveries(0)?.len(), 1);
    assert_eq!(store.audits(0)?.len(), 3);
    assert!(
        store
            .audits(0)?
            .iter()
            .all(|(_, a)| a.actor.as_str() == "operator")
    );
    Ok(())
}

// ADR 0003 / AGENTS.md safety rule 3: provenance belongs to the connection that read bytes.
// ADR 0003 rule 3
#[tokio::test]
async fn connections_have_distinct_provenance() -> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let path = config(temp.path(), false)?;
    let program = Program::load(&path)?;
    let (shutdown, service) = start_service(&path)?;
    let mut peer = connect(&program).await?;
    for control in ["message1", "message2"] {
        let raw = String::from_utf8(ORU.to_vec())?.rewrite("message1", control);
        peer.write_all(&analyte_framing::hl7::encode(
            raw.as_bytes(),
            analyte_framing::hl7::Limit::new(1048576)?,
        )?)
        .await?;
        read_ack(&mut peer).await?;
    }
    drop(peer);
    let mut peer = connect(&program).await?;
    let raw = String::from_utf8(ORU.to_vec())?.rewrite("message1", "message3");
    peer.write_all(&analyte_framing::hl7::encode(
        raw.as_bytes(),
        analyte_framing::hl7::Limit::new(1048576)?,
    )?)
    .await?;
    read_ack(&mut peer).await?;
    let store = stored(&program)?;
    let receipts = store.receipts("")?;
    assert_eq!(receipts.len(), 3);
    let metadata = receipts
        .iter()
        .map(|r| {
            store
                .receipt(r.id)?
                .ok_or("receipt absent")
                .map(|r| r.metadata)
                .map_err(Error::from)
        })
        .collect::<Result<Vec<_>, Error>>()?;
    assert_eq!(metadata[0].connection, metadata[1].connection);
    assert_ne!(metadata[0].connection, metadata[2].connection);
    assert_ne!(metadata[0].connection, metadata[0].machine);
    drop(store);
    shutdown.send(()).map_err(|()| "shutdown receiver gone")?;
    tokio::time::timeout(HANG_GUARD, service).await???;
    Ok(())
}

// ADR 0003 rules 1 and 2, LIS01-A2 §6.3 (each frame ACKed after its commit): when the
// disk fills mid-transfer, the ACKed frames are parked by their transfer, and operator
// reprocessing rebuilds them into a receipt with their exact bytes.
// ADR 0003; CLSI LIS01-A2 §6.3
#[tokio::test]
async fn disk_full_mid_transfer_rebuilds_receipt() -> Result<(), Error> {
    disk_full_rebuilds(false).await
}

// ADR 0003 rules 1 and 2, LIS01-A2 §6.3 and §6.4 (EOT): every frame was ACKed, so a
// receipt write refused at EOT parks the transfer, and operator reprocessing rebuilds
// the complete receipt and delivers its result.
// ADR 0003; CLSI LIS01-A2 §6.4
#[tokio::test]
async fn disk_full_at_eot_rebuilds_and_delivers() -> Result<(), Error> {
    disk_full_rebuilds(true).await
}

async fn disk_full_rebuilds(at_eot: bool) -> Result<(), Error> {
    use std::sync::atomic::{AtomicU64, Ordering::SeqCst};
    const ASTM: &[u8] = b"H|\\^&||||||||||P|LIS2-A2|20261115120000\rP|1\rO|1|00012A\rR|1|^^^SYN|12.30|mg/L||N||F\rL|1|N\r";
    let temp = tempfile::tempdir()?;
    let path = config(temp.path(), true)?;
    let program = Program::load(&path)?;
    let space = std::sync::Arc::new(AtomicU64::new(u64::MAX));
    let (shutdown, stop) = oneshot::channel();
    let owner = tokio::spawn(serve(
        Program::load(&path)?,
        Box::new(analyte_fixtures::AdjustableSpace(space.clone())),
        async {
            stop.await?;
            Ok(())
        },
    ));
    let mut peer = connect(&program).await?;
    peer.write_all(b"\x05").await?;
    assert_eq!(tokio::time::timeout(HANG_GUARD, peer.read_u8()).await??, 6);
    let mut acknowledged = Vec::new();
    let mut number = analyte_framing::astm::FrameNumber::FIRST;
    for (index, record) in ASTM.split_inclusive(|b| *b == b'\r').enumerate() {
        if !at_eot && index == 2 {
            space.store(0, SeqCst);
        }
        let limit = analyte_framing::astm::Limit::new(64000)?;
        for frame in analyte_framing::astm::encode(record, number, limit)? {
            number = number.next();
            peer.write_all(&frame).await?;
            let reply = tokio::time::timeout(HANG_GUARD, peer.read_u8()).await?;
            if space.load(SeqCst) == 0 {
                assert!(!matches!(reply, Ok(6)), "a refused frame was acknowledged");
                break;
            }
            assert_eq!(reply?, 6, "a frame sent with ample space was refused");
            acknowledged.extend_from_slice(&frame);
        }
        if space.load(SeqCst) == 0 {
            break;
        }
    }
    if at_eot {
        space.store(0, SeqCst);
        peer.write_all(b"\x04").await?;
    }
    // The failed store ends the session; the host parks the transfer on close.
    tokio::time::timeout(HANG_GUARD, peer.read_to_end(&mut Vec::new())).await??;
    space.store(u64::MAX, SeqCst);
    shutdown.send(()).map_err(|()| "shutdown lost")?;
    tokio::time::timeout(HANG_GUARD, owner).await???;
    let store = stored(&program)?;
    assert!(
        store.receipts("")?.is_empty(),
        "no receipt before reprocess"
    );
    let parked = store
        .parked(0)?
        .into_iter()
        .find(|p| {
            p.details()
                .is_ok_and(|d| d.kind.as_str() == "incomplete_transfer")
        })
        .ok_or("transfer was not parked")?
        .id;
    drop(store);
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_analyte"))
        .arg("--config")
        .arg(&path)
        .args(["parked", "reprocess", &parked.to_string()])
        .args(["--actor", "operator", "--reason", "disk space restored"])
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let store = stored(&program)?;
    let receipts = store.receipts("")?;
    let [receipt] = receipts.as_slice() else {
        return Err("one rebuilt receipt expected".into());
    };
    assert_eq!(
        store.receipt(receipt.id)?.ok_or("receipt missing")?.raw,
        acknowledged
    );
    // A transfer cut mid-message cannot be mapped, so it stays parked for review.
    let item = store.parked_item(parked)?.ok_or("parked missing")?;
    assert_eq!(item.resolution.is_some(), at_eot);
    assert_eq!(store.deliveries(0)?.len(), usize::from(at_eot));
    Ok(())
}
