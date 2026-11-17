//! Tests machine receipt storage and acknowledgment ordering.
use super::*;
use analyte_fixtures::ConfigEdit as _;

#[tokio::test]
async fn plan11_status_reports_health_and_last_receipt_without_payloads_or_secrets()
-> Result<(), Error> {
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
#[tokio::test]
async fn plan11_cli_reprocess_receipt_parked_and_bounded_delivery_list_require_actor_reason()
-> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let path = config(temp.path(), false)?;
    let definition = std::fs::read_to_string(temp.path().join("machine.toml"))?;
    std::fs::write(
        temp.path().join("machine.toml"),
        definition.rewrite("machine_code=\"SYN\"", "machine_code=\"OTHER\""),
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
    std::fs::write(temp.path().join("machine.toml"), definition)?;
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
#[tokio::test]
async fn adr0003_rule3_connections_have_distinct_provenance() -> Result<(), Error> {
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
