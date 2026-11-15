//! HL7 2.3.1 section 2.18 / 2.5.1 section 5.10 and ADR 0003/0004/0008.
use super::vendor_support::{event, no_event, state};
use super::*;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

async fn setup(
    edition: &str,
    ack_ms: u32,
    query_ms: u32,
) -> Result<(tempfile::TempDir, Fixture, Reference, Running, TcpStream), Error> {
    setup_with_freshness(edition, ack_ms, query_ms, 30).await
}
async fn setup_with_freshness(
    edition: &str,
    ack_ms: u32,
    query_ms: u32,
    freshness: u64,
) -> Result<(tempfile::TempDir, Fixture, Reference, Running, TcpStream), Error> {
    let temp = tempfile::tempdir()?;
    let path = config(temp.path(), false)?;
    let site = temp.path().join("hl7-site.toml");
    std::fs::write(
        &site,
        format!(
            "{}\nfreshness_seconds = {freshness}\n",
            std::fs::read_to_string(&site)?
        ),
    )?;
    let file = temp.path().join("hl7.toml");
    let definition = std::fs::read_to_string(&file)?
        .replace(r#"edition = "v251""#, &format!(r#"edition = "{edition}""#));
    std::fs::write(
        file,
        format!(
            "{definition}
{}",
            include_str!("../../../config/examples/hl7_orders.toml")
                .replace(
                    "acknowledgment_ms = 500",
                    &format!("acknowledgment_ms = {ack_ms}")
                )
                .replace("deadline_ms = 1000", &format!("deadline_ms = {query_ms}"))
        ),
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
    let peer = connect(&path, "hl7").await?;
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if database(&path)?.query_row(
                "SELECT count(*) FROM feed WHERE complete=1 AND trusted=1",
                [],
                |r| r.get::<_, i64>(0),
            )? == 1
            {
                return Ok::<_, Error>(());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await??;
    Ok((temp, path, reference, service, peer))
}
fn query(sample: &str, edition: &str) -> Vec<u8> {
    format!("MSH|^~\\&|SYN|LAB|HOST|LAB|202611151200||QRY^Q02|query1|P|{edition}\rQRD|20261115|D|D|tag|||100^LI|{sample}^^^^^^^LCL|ORD|TEST\r").into_bytes()
}
async fn send(peer: &mut TcpStream, payload: &[u8]) -> Result<(), Error> {
    peer.write_all(&analyte_framing::mllp::encode(
        payload,
        analyte_framing::mllp::Limit::new(1048576)?,
    )?)
    .await?;
    Ok(())
}
async fn packet(peer: &mut TcpStream) -> Result<Vec<u8>, Error> {
    tokio::time::timeout(Duration::from_secs(5), async {
        assert_eq!(peer.read_u8().await?, 11);
        let mut bytes = Vec::new();
        loop {
            let byte = peer.read_u8().await?;
            if byte == 28 {
                assert_eq!(peer.read_u8().await?, 13);
                return Ok::<_, Error>(bytes);
            }
            if bytes.len() >= 1048576 {
                return Err("oversized HL7 response".into());
            }
            bytes.push(byte);
        }
    })
    .await?
}
async fn ack(peer: &mut TcpStream, dsr: &[u8], code: &str, edition: &str) -> Result<(), Error> {
    let message = analyte_hl7::Message::parse(dsr, analyte_hl7::Options::default())?;
    let header = analyte_hl7::Msh::new(&message)?;
    let id = std::str::from_utf8(header.control_id())?;
    send(
        peer,
        format!(
            "MSH|^~\\&|SYN|LAB|HOST|LAB|202611151200||ACK^Q03|reply1|P|{edition}\rMSA|{code}|{id}\r"
        )
        .as_bytes(),
    )
    .await
}
fn text(bytes: &[u8]) -> Result<&str, Error> {
    Ok(std::str::from_utf8(bytes)?)
}

#[tokio::test]
async fn hl7_231_251_query_found_reserves_before_dsr_ack_aa_sends_once() -> Result<(), Error> {
    for (edition, version) in [("v231", "2.3.1"), ("v251", "2.5.1")] {
        let (_temp, path, reference, service, mut peer) = setup(edition, 30000, 30000).await?;
        let qry = query("000Ab12", version);
        send(&mut peer, &qry).await?;
        let qck = packet(&mut peer).await?;
        assert!(text(&qck)?.contains("QCK^Q02"));
        assert!(text(&qck)?.contains("QAK||OK\r"));
        let raw: Vec<u8> =
            database(&path)?.query_row("SELECT raw FROM receipts LIMIT 1", [], |r| r.get(0))?;
        assert_eq!(
            raw,
            analyte_framing::mllp::encode(&qry, analyte_framing::mllp::Limit::new(1048576)?)?
        );
        let dsr = packet(&mut peer).await?;
        assert!(
            text(&dsr)?.contains(
                "DSP|1||000Ab12\rDSP|2||SEND\rDSP|3||R\rDSP|4||\rDSP|5||order-1\rDSP|6||\r"
            )
        );
        state(&path, "Sending").await?;
        no_event(&path).await?;
        let (count, coverage): (i64, String) = database(&path)?.query_row(
            "SELECT count(*),tests FROM machine_order_items",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        assert_eq!(count, 1);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&coverage)?,
            serde_json::json!([{"namespace":"synthetic","value":"TEST"}])
        );
        ack(&mut peer, &dsr, "AA", version).await?;
        event(
            &path,
            &_temp.path().join("receiver.db"),
            "Sent",
            "HL7-DSR-ACK-AA",
        )
        .await?;
        send(&mut peer, &qry).await?;
        assert!(text(&packet(&mut peer).await?)?.contains("QAK||AE\r"));
        service.stop().await?;
        reference.close().await?;
    }
    Ok(())
}

#[tokio::test]
async fn hl7_251_qck_trusted_empty_nf_untrusted_stale_and_unknown_filter_ae() -> Result<(), Error> {
    let (_temp, path, reference, service, mut peer) = setup("v251", 30000, 30000).await?;
    send(&mut peer, &query("absent", "2.5.1")).await?;
    assert!(text(&packet(&mut peer).await?)?.contains("QAK||NF\r"));
    reference.close().await?;
    for update in [
        "UPDATE feed SET complete=0",
        "UPDATE feed SET complete=1,trusted=1,polled=0",
        "DELETE FROM orders; DELETE FROM feed",
        "INSERT INTO feed(destination,cursor,complete,trusted,polled) VALUES ('demo','c',1,1,unixepoch()); INSERT INTO audit(evidence) VALUES ('{\"action\":\"store-restore\"}')",
    ] {
        database(&path)?.execute_batch(update)?;
        send(&mut peer, &query("absent", "2.5.1")).await?;
        assert!(text(&packet(&mut peer).await?)?.contains("QAK||AE\r"));
    }
    send(
        &mut peer,
        &String::from_utf8(query("000Ab12", "2.5.1"))?
            .replace("|ORD|TEST", "|RES|TEST")
            .into_bytes(),
    )
    .await?;
    assert!(text(&packet(&mut peer).await?)?.contains("QAK||AE\r"));
    assert_eq!(
        database(&path)?.query_row("SELECT count(*) FROM machine_orders", [], |r| r
            .get::<_, i64>(0))?,
        0
    );
    service.stop().await?;
    Ok(())
}

#[tokio::test]
async fn hl7_251_dsr_ack_ae_ar_lost_and_disconnect_are_specific_uncertain() -> Result<(), Error> {
    for (code, reason) in [
        ("AE", "HL7-DSR-ACK-AE"),
        ("AR", "HL7-DSR-ACK-AR"),
        ("lost", "HL7-DSR-ACK-timeout"),
        ("disconnect", "HL7-DSR-disconnect"),
    ] {
        let (_temp, path, reference, service, mut peer) =
            setup("v251", if code == "lost" { 500 } else { 30000 }, 30000).await?;
        send(&mut peer, &query("000Ab12", "2.5.1")).await?;
        packet(&mut peer).await?;
        let dsr = packet(&mut peer).await?;
        if matches!(code, "AE" | "AR") {
            ack(&mut peer, &dsr, code, "2.5.1").await?;
        }
        if code == "disconnect" {
            drop(peer);
        }
        event(
            &path,
            &_temp.path().join("receiver.db"),
            "Uncertain",
            reason,
        )
        .await?;
        service.stop().await?;
        reference.close().await?;
    }
    Ok(())
}

#[tokio::test]
async fn hl7_251_adr0004_total_query_deadline_includes_split_block_receive_time()
-> Result<(), Error> {
    let (_temp, path, reference, service, mut peer) = setup("v251", 30000, 1000).await?;
    let wire = analyte_framing::mllp::encode(
        &query("000Ab12", "2.5.1"),
        analyte_framing::mllp::Limit::new(1048576)?,
    )?;
    peer.write_all(wire.get(..10).ok_or("prefix")?).await?;
    tokio::time::sleep(Duration::from_millis(1100)).await;
    peer.write_all(wire.get(10..).ok_or("suffix")?).await?;
    assert!(text(&packet(&mut peer).await?)?.contains("QAK||AE\r"));
    assert_eq!(
        database(&path)?.query_row("SELECT count(*) FROM machine_orders", [], |r| r
            .get::<_, i64>(0))?,
        0
    );
    service.stop().await?;
    reference.close().await?;
    Ok(())
}

#[test]
fn hl7_231_251_synthetic_query_obeys_section_2_18_and_5_10() -> Result<(), Error> {
    for edition in ["2.3.1", "2.5.1"] {
        let message = analyte_hl7::Message::parse(
            &query("000Ab12", edition),
            analyte_hl7::Options::default(),
        )?;
        analyte_hl7::query::Request::new(&message)?;
    }
    Ok(())
}

#[tokio::test]
async fn hl7_251_dsr_mismatched_application_ack_is_retained_and_uncertain() -> Result<(), Error> {
    let (temp, path, reference, service, mut peer) = setup("v251", 30000, 30000).await?;
    send(&mut peer, &query("000Ab12", "2.5.1")).await?;
    packet(&mut peer).await?;
    packet(&mut peer).await?;
    send(&mut peer,b"MSH|^~\\&|SYN|LAB|HOST|LAB|202611151200||ACK^Q03|reply1|P|2.5.1\rMSA|AA|wrong-control-id\r").await?;
    event(
        &path,
        &temp.path().join("receiver.db"),
        "Uncertain",
        "HL7-DSR-ACK-invalid",
    )
    .await?;
    assert_eq!(
        database(&path)?.query_row("SELECT count(*) FROM receipts", [], |r| r.get::<_, i64>(0))?,
        2
    );
    service.stop().await?;
    reference.close().await?;
    Ok(())
}

#[tokio::test]
async fn hl7_251_order_query_connection_still_delivers_oru_results() -> Result<(), Error> {
    let (_temp, path, reference, service, mut peer) = setup("v251", 30000, 30000).await?;
    let reply = hl7(&mut peer, ORU).await?;
    assert!(reply.contains("MSA|AA|message1"));
    wait_state(&path, "done", 1).await?;
    service.stop().await?;
    reference.close().await?;
    Ok(())
}

#[tokio::test]
async fn hl7_251_section5_10_quantity_limit_refuses_partial_dsr_before_reservation()
-> Result<(), Error> {
    let (_temp, path, reference, service, mut peer) = setup("v251", 30000, 30000).await?;
    let bytes = String::from_utf8(query("000Ab12", "2.5.1"))?.replace("100^LI", "1^LI");
    send(&mut peer, bytes.as_bytes()).await?;
    assert!(text(&packet(&mut peer).await?)?.contains("QAK||AE\r"));
    assert_eq!(
        database(&path)?.query_row("SELECT count(*) FROM machine_orders", [], |r| r
            .get::<_, i64>(0))?,
        0
    );
    no_event(&path).await?;
    service.stop().await?;
    reference.close().await?;
    Ok(())
}

#[tokio::test]
async fn plan11_hl7_dsr_answer_reserves_nothing_during_restore_pause() -> Result<(), Error> {
    let (_temp, path, reference, service, mut peer) = setup("v251", 30000, 30000).await?;
    database(&path)?.execute("INSERT INTO audit(evidence) VALUES (?1)",[serde_json::json!({"actor":"operator","action":"store-restore","targets":[],"time":jiff::Timestamp::now().as_second(),"reason":"synthetic restore"}).to_string()])?;
    send(&mut peer, &query("000Ab12", "2.5.1")).await?;
    assert!(text(&packet(&mut peer).await?)?.contains("QAK||AE\r"));
    for table in [
        "machine_orders",
        "machine_order_items",
        "machine_order_item_history",
        "outbox",
    ] {
        assert_eq!(
            database(&path)?.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                .get::<_, i64>(0))?,
            0
        );
    }
    service.stop().await?;
    reference.close().await?;
    Ok(())
}

// ADR 0004 / rule 9 / HL7 2.5.1 section 5.10: the configured age decides NF versus AE.
#[tokio::test]
async fn rule9_hl7_query_freshness_follows_destination_config() -> Result<(), Error> {
    for (freshness, expected) in [(10, "QAK||AE\r"), (120, "QAK||NF\r")] {
        let (_temp, path, reference, service, mut peer) =
            setup_with_freshness("v251", 30000, 30000, freshness).await?;
        reference.close().await?;
        database(&path)?.execute(
            "UPDATE feed SET polled=?1",
            [jiff::Timestamp::now().as_second() - 50],
        )?;
        send(&mut peer, &query("absent", "2.5.1")).await?;
        assert!(text(&packet(&mut peer).await?)?.contains(expected));
        service.stop().await?;
    }
    Ok(())
}
