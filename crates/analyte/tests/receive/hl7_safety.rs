//! Verifies unsafe HL7 reports remain durable without delivery or application success.
use super::*;
use analyte_fixtures::ConfigEdit as _;

// HL7 2.5.1 §2.9.2, §2.10.2.2, §7.4.2.11, §7.4.1.25; ADR 0003; ADR 0010
// (2026-11-26): an unsafe report is stored and held for an operator, so the
// machine gets AA while nothing is delivered.
#[tokio::test]
async fn unsafe_reports_are_parked() -> Result<(), Error> {
    let original = String::from_utf8(ORU.to_vec())?;
    for raw in [
        original.rewrite("|||||F", "|||||UNKNOWN"),
        original.rewrite("|||||F", "|||||"),
        original.rewrite("|2.5.1\r", "|2.5.1||fragment\r"),
        format!("{original}DSC|fragment\r"),
    ] {
        let temp = tempfile::tempdir()?;
        let path = config(temp.path(), false)?;
        let program = Program::load(&path)?;
        let (shutdown, stop) = oneshot::channel();
        let service = tokio::spawn(serve(
            Program::load(&path)?,
            Box::new(Space(u64::MAX)),
            async {
                stop.await?;
                Ok(())
            },
        ));
        let mut peer = connect(&program).await?;
        let wire = analyte_framing::hl7::encode(
            raw.as_bytes(),
            analyte_framing::hl7::Limit::new(1048576)?,
        )?;
        peer.write_all(&wire).await?;
        let ack = String::from_utf8(read_ack(&mut peer).await?)?;
        assert!(ack.contains("MSA|AA|message1"), "{ack}");
        assert!(!ack.contains("ERR|"), "{ack}");
        let store = stored(&program)?;
        let receipts = store.receipts("")?;
        let receipt = receipts.first().ok_or("missing retained receipt")?;
        assert_eq!(store.receipt(receipt.id)?.ok_or("raw")?.raw, wire);
        assert!(!store.parked(0)?.is_empty());
        assert!(store.deliveries(0)?.is_empty());
        shutdown.send(()).map_err(|()| "shutdown")?;
        tokio::time::timeout(HANG_GUARD, service).await???;
    }
    Ok(())
}

// ADR 0003 store before acknowledging; AGENTS.md safety rules 2 and 3. A message
// near the store record bound gets a final ACK, so the machine never retransmits.
// HL7 2.5.1 §2.9.2; ADR 0010 (2026-11-26): the larger one is stored and parked
// with its specific reason, so it is accepted too.
#[tokio::test]
async fn near_limit_message_is_answered() -> Result<(), Error> {
    let original = String::from_utf8(ORU.to_vec())?;
    for (size, held) in [(6_000, false), (15_000, true)] {
        let temp = tempfile::tempdir()?;
        let path = config(temp.path(), false)?;
        let machine = analyte_fixtures::definition_path(temp.path(), "synthetic");
        std::fs::write(
            &machine,
            std::fs::read_to_string(&machine)?
                .rewrite("limit = 1048576", "limit = 16384")
                .rewrite("bytes = 1048576", "bytes = 16384"),
        )?;
        std::fs::write(
            &path,
            std::fs::read_to_string(&path)?
                .rewrite("record_bytes = 2097152", "record_bytes = 16384"),
        )?;
        let raw = original
            .rewrite("NM|SYN", "ST|SYN")
            .rewrite("12.30", &"A".repeat(size));
        let program = Program::load(&path)?;
        let (shutdown, service) = start_service(&path)?;
        let mut peer = connect(&program).await?;
        let wire =
            analyte_framing::hl7::encode(raw.as_bytes(), analyte_framing::hl7::Limit::new(16384)?)?;
        peer.write_all(&wire).await?;
        let ack = String::from_utf8(read_ack(&mut peer).await?)?;
        assert!(ack.contains("MSA|AA|message1"), "{ack}");
        let store = stored(&program)?;
        assert_eq!(store.receipts("")?.len(), 1);
        let parked = store.parked(0)?;
        assert_eq!(
            parked.iter().any(|p| p.evidence.contains("record_bytes")),
            held,
            "only the oversized interpretation is parked, with a specific reason"
        );
        shutdown.send(()).map_err(|()| "shutdown")?;
        tokio::time::timeout(HANG_GUARD, service).await???;
    }
    Ok(())
}
