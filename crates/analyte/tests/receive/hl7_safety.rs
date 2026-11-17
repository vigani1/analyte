//! Verifies unsafe HL7 reports remain durable without delivery or application success.
use super::*;
use analyte_fixtures::ConfigEdit as _;

// HL7 2.5.1 sections 2.10.2.2, 7.4.2.11 and 7.4.1.25; ADR 0003.
#[tokio::test]
async fn hl7_251_s2_10_2_2_s7_4_2_11_s7_4_1_25_unsafe_reports_are_parked() -> Result<(), Error> {
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
        assert!(ack.contains("MSA|AE|message1"), "{ack}");
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
