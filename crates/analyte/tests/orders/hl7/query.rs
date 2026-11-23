//! Tests HL7 messages, acknowledgments and failure handling.
use super::vendor::{event, no_event, state};
use super::*;
use crate::support::HANG_GUARD;
use analyte_fixtures::ConfigEdit as _;
use tokio::{io::AsyncWriteExt, net::TcpStream};

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
    setup_observed(edition, ack_ms, query_ms, freshness, None, None).await
}
async fn setup_observed(
    edition: &str,
    ack_ms: u32,
    query_ms: u32,
    freshness: u64,
    hook: Option<Arc<analyte::runtime::test_hooks::Committed>>,
    order: Option<analyte_contract::Order>,
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
    let file = analyte_fixtures::definition_path(temp.path(), "hl7");
    let definition = std::fs::read_to_string(&file)?
        .rewrite(r#"edition = "v251""#, &format!(r#"edition = "{edition}""#));
    std::fs::write(
        file,
        format!(
            "{definition}
{}",
            include_str!("../../../../core/config/examples/hl7_orders.toml")
                .rewrite(
                    "acknowledgment_ms = 500",
                    &format!("acknowledgment_ms = {ack_ms}")
                )
                .rewrite("deadline_ms = 1000", &format!("deadline_ms = {query_ms}"))
        ),
    )?;
    let reference = Reference::start(
        &temp.path().join("receiver.db"),
        TcpListener::bind("127.0.0.1:0").await?,
        false,
    )
    .await?;
    let order = match order {
        Some(order) => order,
        None => analyte_contract::decode(include_bytes!(
            "../../../../../docs/contract/vectors/order.json"
        ))?,
    };
    reference.receiver.seed(vec![order]).await?;
    let mut loaded = program(&path, &reference.url)?;
    if let Some(hook) = hook {
        loaded.pause_input_processed_for_test("hl7", hook)?;
    }
    let service = Running::start_program(loaded, "synthetic-api").await?;
    let peer = connect(&path, "hl7").await?;
    tokio::time::timeout(HANG_GUARD, async {
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
    format!("MSH|^~\\&|SYN|LAB|HOST|LAB|202611151200||QRY^Q02^QRY_Q02|query1|P|{edition}\rQRD|20261115|D|D|tag|||100^LI|{sample}^^^^^^^LCL|ORD|TEST\r").into_bytes()
}
async fn send(peer: &mut TcpStream, payload: &[u8]) -> Result<(), Error> {
    peer.write_all(&analyte_framing::hl7::encode(
        payload,
        analyte_framing::hl7::Limit::new(1048576)?,
    )?)
    .await?;
    Ok(())
}

async fn ack(peer: &mut TcpStream, dsr: &[u8], code: &str, edition: &str) -> Result<(), Error> {
    let message = analyte_hl7::Message::parse(dsr, analyte_hl7::Options::default())?;
    let header = analyte_hl7::Msh::new(&message)?;
    let id = std::str::from_utf8(header.control_id())?;
    send(
        peer,
        format!(
            "MSH|^~\\&|SYN|LAB|HOST|LAB|202611151200||ACK^Q03^ACK|reply1|P|{edition}\rMSA|{code}|{id}\r"
        )
        .as_bytes(),
    )
    .await
}
fn text(bytes: &[u8]) -> Result<&str, Error> {
    Ok(std::str::from_utf8(bytes)?)
}

// HL7 2.3.1; HL7 2.5.1
#[tokio::test]
async fn found_query_reserves_before_dsr_sends_once() -> Result<(), Error> {
    for (edition, version) in [("v231", "2.3.1"), ("v251", "2.5.1")] {
        let (_temp, path, reference, service, mut peer) = setup(edition, 30000, 30000).await?;
        let qry = query("000Ab12", version);
        send(&mut peer, &qry).await?;
        let qck = read_mllp_packet(&mut peer, "oversized HL7 response").await?;
        assert!(text(&qck)?.contains("QCK^Q02"));
        assert!(text(&qck)?.contains("QAK||OK\r"));
        let raw: Vec<u8> =
            database(&path)?.query_row("SELECT raw FROM receipts LIMIT 1", [], |r| r.get(0))?;
        assert_eq!(
            raw,
            analyte_framing::hl7::encode(&qry, analyte_framing::hl7::Limit::new(1048576)?)?
        );
        let dsr = read_mllp_packet(&mut peer, "oversized HL7 response").await?;
        assert!(
            text(&dsr)?.contains(
                "DSP|1||000Ab12\rDSP|2||TEST\rDSP|3||R\rDSP|4||\rDSP|5||order-1\rDSP|6||\r"
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
        assert!(
            text(&read_mllp_packet(&mut peer, "oversized HL7 response").await?)?
                .contains("QAK||AE\r")
        );
        service.stop().await?;
        reference.close().await?;
    }
    Ok(())
}

// HL7 2.5.1
#[tokio::test]
async fn qck_nf_only_for_trusted_empty_result() -> Result<(), Error> {
    let (_temp, path, reference, service, mut peer) = setup("v251", 30000, 30000).await?;
    send(&mut peer, &query("absent", "2.5.1")).await?;
    assert!(
        text(&read_mllp_packet(&mut peer, "oversized HL7 response").await?)?.contains("QAK||NF\r")
    );
    reference.close().await?;
    for update in [
        "UPDATE feed SET complete=0",
        "UPDATE feed SET complete=1,trusted=1,polled=0",
        "DELETE FROM orders; DELETE FROM feed",
        "INSERT INTO feed(destination,cursor,complete,trusted,polled) VALUES ('demo','c',1,1,unixepoch()); INSERT INTO audit(evidence) VALUES ('{\"action\":\"store-restore\"}')",
    ] {
        database(&path)?.execute_batch(update)?;
        send(&mut peer, &query("absent", "2.5.1")).await?;
        assert!(
            text(&read_mllp_packet(&mut peer, "oversized HL7 response").await?)?
                .contains("QAK||AE\r")
        );
    }
    send(
        &mut peer,
        &String::from_utf8(query("000Ab12", "2.5.1"))?
            .rewrite("|ORD|TEST", "|RES|TEST")
            .into_bytes(),
    )
    .await?;
    assert!(
        text(&read_mllp_packet(&mut peer, "oversized HL7 response").await?)?.contains("QAK||AE\r")
    );
    assert_eq!(
        database(&path)?.query_row("SELECT count(*) FROM machine_orders", [], |r| r
            .get::<_, i64>(0))?,
        0
    );
    service.stop().await?;
    Ok(())
}

// HL7 2.5.1
#[tokio::test]
async fn failed_dsr_ack_is_specific_uncertain() -> Result<(), Error> {
    for (code, reason) in [
        ("AE", "HL7-DSR-ACK-AE"),
        ("AR", "HL7-DSR-ACK-AR"),
        ("lost", "HL7-DSR-ACK-timeout"),
        ("disconnect", "HL7-DSR-disconnect"),
    ] {
        let (_temp, path, reference, service, mut peer) =
            setup("v251", if code == "lost" { 500 } else { 30000 }, 30000).await?;
        send(&mut peer, &query("000Ab12", "2.5.1")).await?;
        read_mllp_packet(&mut peer, "oversized HL7 response").await?;
        let dsr = read_mllp_packet(&mut peer, "oversized HL7 response").await?;
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

// HL7 2.5.1; ADR 0004
#[tokio::test]
async fn query_deadline_counts_split_block_receive() -> Result<(), Error> {
    let (hook, control) = analyte::runtime::test_hooks::Committed::new();
    let (_temp, path, reference, service, mut peer) =
        setup_observed("v251", 30000, 1000, 30, Some(hook), None).await?;
    let wire = analyte_framing::hl7::encode(
        &query("000Ab12", "2.5.1"),
        analyte_framing::hl7::Limit::new(1048576)?,
    )?;
    peer.write_all(wire.get(..10).ok_or("prefix")?).await?;
    control.reached().await?;
    // The prefix is inside the dialog before the query budget starts expiring.
    tokio::time::sleep(Duration::from_millis(1100)).await;
    control.resume();
    peer.write_all(wire.get(10..).ok_or("suffix")?).await?;
    assert!(
        text(&read_mllp_packet(&mut peer, "oversized HL7 response").await?)?.contains("QAK||AE\r")
    );
    assert_eq!(
        database(&path)?.query_row("SELECT count(*) FROM machine_orders", [], |r| r
            .get::<_, i64>(0))?,
        0
    );
    service.stop().await?;
    reference.close().await?;
    Ok(())
}

// HL7 2.3.1 §2.18; HL7 2.5.1 §5.10
#[test]
fn synthetic_query_parses_in_both_editions() -> Result<(), Error> {
    for edition in ["2.3.1", "2.5.1"] {
        let message = analyte_hl7::Message::parse(
            &query("000Ab12", edition),
            analyte_hl7::Options::default(),
        )?;
        analyte_hl7::query::Request::new(&message)?;
    }
    Ok(())
}

// HL7 2.5.1
#[tokio::test]
async fn mismatched_dsr_ack_is_retained_and_uncertain() -> Result<(), Error> {
    let (temp, path, reference, service, mut peer) = setup("v251", 30000, 30000).await?;
    send(&mut peer, &query("000Ab12", "2.5.1")).await?;
    read_mllp_packet(&mut peer, "oversized HL7 response").await?;
    read_mllp_packet(&mut peer, "oversized HL7 response").await?;
    send(&mut peer,b"MSH|^~\\&|SYN|LAB|HOST|LAB|202611151200||ACK^Q03^ACK|reply1|P|2.5.1\rMSA|AA|wrong-control-id\r").await?;
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

// HL7 2.5.1
#[tokio::test]
async fn query_connection_still_delivers_oru_results() -> Result<(), Error> {
    let (_temp, path, reference, service, mut peer) = setup("v251", 30000, 30000).await?;
    let reply = hl7(&mut peer, ORU).await?;
    assert!(reply.contains("MSA|AA|message1"));
    wait_state(&path, "done", 1).await?;
    service.stop().await?;
    reference.close().await?;
    Ok(())
}

// HL7 2.5.1 §5.10
#[tokio::test]
async fn quantity_limit_refuses_partial_dsr() -> Result<(), Error> {
    let (_temp, path, reference, service, mut peer) = setup("v251", 30000, 30000).await?;
    let bytes = String::from_utf8(query("000Ab12", "2.5.1"))?.rewrite("100^LI", "1^LI");
    send(&mut peer, bytes.as_bytes()).await?;
    assert!(
        text(&read_mllp_packet(&mut peer, "oversized HL7 response").await?)?.contains("QAK||AE\r")
    );
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

// Plan §11
#[tokio::test]
async fn dsr_reserves_nothing_during_restore_pause() -> Result<(), Error> {
    let (_temp, path, reference, service, mut peer) = setup("v251", 30000, 30000).await?;
    database(&path)?.execute("INSERT INTO audit(evidence) VALUES (?1)",[serde_json::json!({"actor":"operator","action":"store-restore","targets":[],"time":jiff::Timestamp::now().as_second(),"reason":"synthetic restore"}).to_string()])?;
    send(&mut peer, &query("000Ab12", "2.5.1")).await?;
    assert!(
        text(&read_mllp_packet(&mut peer, "oversized HL7 response").await?)?.contains("QAK||AE\r")
    );
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
// Safety rule 9 (ADR 0003)
#[tokio::test]
async fn hl7_query_freshness_follows_destination_config() -> Result<(), Error> {
    for (freshness, expected) in [(10, "QAK||AE\r"), (120, "QAK||NF\r")] {
        let (_temp, path, reference, service, mut peer) =
            setup_with_freshness("v251", 30000, 30000, freshness).await?;
        reference.close().await?;
        database(&path)?.execute(
            "UPDATE feed SET polled=?1",
            [jiff::Timestamp::now().as_second() - 50],
        )?;
        send(&mut peer, &query("absent", "2.5.1")).await?;
        assert!(
            text(&read_mllp_packet(&mut peer, "oversized HL7 response").await?)?.contains(expected)
        );
        service.stop().await?;
    }
    Ok(())
}

// Contract v1: Order tests.
#[tokio::test]
async fn diluted_query_sends_no_display_work() -> Result<(), Error> {
    let mut value: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../../../../../docs/contract/vectors/order.json"
    ))?;
    value
        .get_mut("tests")
        .and_then(|tests| tests.get_mut(0))
        .and_then(serde_json::Value::as_object_mut)
        .ok_or("synthetic test")?
        .insert("dilution_ratio".into(), 5.into());
    let app_order = analyte_contract::decode(&serde_json::to_vec(&value)?)?;
    let (_temp, path, reference, service, mut peer) =
        setup_observed("v251", 30000, 30000, 30, None, Some(app_order)).await?;
    send(&mut peer, &query("000Ab12", "2.5.1")).await?;
    let qck = read_mllp_packet(&mut peer, "oversized HL7 response").await?;
    assert!(text(&qck)?.contains("QAK||AE"));
    state(&path, "Rejected").await?;
    let code: String =
        database(&path)?.query_row("SELECT source_code FROM machine_orders", [], |row| {
            row.get(0)
        })?;
    assert!(code.contains("unsupported-dilution"), "{code}");
    assert!(
        tokio::time::timeout(
            Duration::from_millis(200),
            read_mllp_packet(&mut peer, "unexpected display reply")
        )
        .await
        .is_err()
    );
    service.stop().await?;
    reference.close().await?;
    Ok(())
}
