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

// HL7 2.5.1 §2.9.2.1 b and §2.15.5, table 0357 code 203; ADR 0003; AGENTS.md
// safety rules 1 and 2: an MSH-12 we do not recognise is stored and parked, then
// answered AR with its reason, and the connection carries the next message.
#[tokio::test]
async fn unsupported_version_is_rejected_and_link_stays_open() -> Result<(), Error> {
    let original = String::from_utf8(ORU.to_vec())?;
    let unsupported = original
        .rewrite("|2.5.1\r", "|9.9\r")
        .rewrite("|message1|", "|message0|");
    let temp = tempfile::tempdir()?;
    let path = config(temp.path(), false)?;
    let program = Program::load(&path)?;
    let (shutdown, service) = start_service(&path)?;
    let mut peer = connect(&program).await?;
    let limit = analyte_framing::hl7::Limit::new(1048576)?;
    let wire = analyte_framing::hl7::encode(unsupported.as_bytes(), limit)?;
    peer.write_all(&wire).await?;
    let ack = String::from_utf8(read_ack(&mut peer).await?)?;
    assert!(ack.contains("|P|2.5.1|"), "{ack}");
    assert!(ack.contains("MSA|AR|message0\r"), "{ack}");
    assert!(ack.contains("ERR||MSH^1^12|203|E|"), "{ack}");

    peer.write_all(&analyte_framing::hl7::encode(ORU, limit)?)
        .await?;
    let ack = String::from_utf8(read_ack(&mut peer).await?)?;
    assert!(ack.contains("MSA|AA|message1"), "{ack}");

    let store = stored(&program)?;
    assert!(store.receipts("")?.iter().any(|receipt| {
        store
            .receipt(receipt.id)
            .ok()
            .flatten()
            .is_some_and(|r| r.raw == wire)
    }));
    assert!(
        store
            .parked(0)?
            .iter()
            .any(|parked| parked.evidence.contains("MSH-12"))
    );
    shutdown.send(()).map_err(|()| "shutdown")?;
    tokio::time::timeout(HANG_GUARD, service).await???;
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
