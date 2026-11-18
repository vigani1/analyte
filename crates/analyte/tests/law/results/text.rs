//! LAW text-kind refusal and FT layout evidence remain protected and undelivered.
use super::*;
use analyte_fixtures::ConfigEdit as _;
// HL7 2.5.1 §2.7.1, §2.7.6
#[tokio::test]
async fn text_is_parked_with_reason_and_raw() -> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let path = vendor_config(temp.path(), "law", &definition())?;
    let receiver_path = temp.path().join("reference.db");
    let reference = Reference::start(
        &receiver_path,
        TcpListener::bind("127.0.0.1:0").await?,
        false,
    )
    .await?;
    let running = Running::start(&path, &reference.url, "synthetic-api").await?;
    let mut peer = connect(&path, "law").await?;
    for (id, wire, reason) in [
        (
            "st",
            result("st", "\"\"", "awos-1^LAB", r"before\H\marked\N\after", "F")
                .rewrite("|NM|", "|ST|"),
            "escape is not applicable to ST",
        ),
        (
            "tx",
            result("tx", "\"\"", "awos-1^LAB", r"first\.br\second", "F").rewrite("|NM|", "|TX|"),
            "layout commands require FT",
        ),
        (
            "ft",
            result("ft", "\"\"", "awos-1^LAB", "12.30", "F") + "NTE|1|Z|first^second\r",
            "UnsupportedFormatting",
        ),
    ] {
        let ack = hl7(&mut peer, wire.as_bytes()).await?;
        assert!(!ack.contains(&format!("MSA|AA|{id}")), "{ack}");
        let raw: Vec<u8> = database(&path)?.query_row(
            "SELECT raw FROM receipts ORDER BY rowid DESC LIMIT 1",
            [],
            |r| r.get(0),
        )?;
        assert_eq!(
            raw,
            analyte_framing::hl7::encode(
                wire.as_bytes(),
                analyte_framing::hl7::Limit::new(1048576)?
            )?
        );
        let evidence: String =
            database(&path)?.query_row("SELECT group_concat(evidence) FROM parked", [], |r| {
                r.get(0)
            })?;
        let interpretation: String = database(&path)?.query_row(
            "SELECT evidence FROM interpretations ORDER BY rowid DESC LIMIT 1",
            [],
            |r| r.get(0),
        )?;
        if id == "ft" {
            assert!(evidence.contains(reason), "{id}: {evidence}");
        } else {
            assert!(interpretation.contains(reason), "{id}: {interpretation}");
        }
        assert!(evidence.contains("interpretation_failure"), "{evidence}");
        assert_eq!(count(&path, "outbox")?, 0);
    }
    running.stop().await?;
    assert_eq!(receiver_count(&receiver_path)?, 0);
    reference.close().await?;
    Ok(())
}
