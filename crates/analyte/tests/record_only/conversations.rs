//! Holds one synthetic analyzer conversation per record-only capture profile (ADR 0034).
use super::{Error, Recorder};
use analyte_framing::astm::{FrameNumber, Limit};
use std::sync::atomic::Ordering;
use std::time::Duration;

const ACK: u8 = 6;
const NAK: u8 = 21;
const ENQ: u8 = 5;
const EOT: u8 = 4;

/// Sends one LIS01 transfer of `records`, one record per frame, and expects
/// ACK to ENQ and to every frame.
async fn astm_transfer(run: &mut Recorder, records: &[&[u8]]) -> Result<(), Error> {
    run.send(&[ENQ]).await?;
    assert_eq!(run.byte().await?, ACK, "ENQ accepted");
    let mut number = FrameNumber::FIRST;
    for record in records {
        for frame in analyte_framing::astm::encode(record, number, Limit::new(64000)?)? {
            number = number.next();
            run.send(&frame).await?;
            assert_eq!(run.byte().await?, ACK, "frame acknowledged");
        }
    }
    run.send(&[EOT]).await?;
    Ok(())
}

// CLSI LIS01-A2 6.2, 6.3 and 6.5.2 (establishment, frames and their
// acknowledgment, timers); LIS02-A2 12.3 (termination code Q); ADR 0034.
#[tokio::test]
async fn astm_record_only_acknowledges_keeps_every_byte_and_answers_queries_with_q()
-> Result<(), Error> {
    let mut run = Recorder::start("astm", "").await?;
    astm_transfer(
        &mut run,
        &[
            b"H|\\^&|||Analyzer|||||||P|LIS2-A2|20261123120000\r",
            b"P|1\r",
            b"O|1|000123||^^^SYN|R\r",
            b"R|1|^^^SYN|5.2|mg/L||N||F\r",
            b"L|1|N\r",
        ],
    )
    .await?;
    // A frame whose checksum is wrong is refused with NAK, and still kept.
    run.send(&[ENQ]).await?;
    assert_eq!(run.byte().await?, ACK);
    let mut frame = analyte_framing::astm::encode(
        b"H|\\^&|||Analyzer\r",
        FrameNumber::FIRST,
        Limit::new(64000)?,
    )?
    .next()
    .ok_or("frame")?;
    let checksum = frame.len().checked_sub(4).ok_or("frame length")?;
    let digit = frame.get_mut(checksum).ok_or("checksum")?;
    *digit = if *digit == b'0' { b'1' } else { b'0' };
    run.send(&frame).await?;
    assert_eq!(
        run.byte().await?,
        NAK,
        "a bad checksum is never acknowledged"
    );
    run.send(&[EOT]).await?;
    // A query: the host answers that it cannot answer now, never "no information".
    astm_transfer(
        &mut run,
        &[
            b"H|\\^&|||Analyzer|||||||P|LIS2-A2|20261123120100\r",
            b"Q|1|^000124||ALL||||||||O\r",
            b"L|1|N\r",
        ],
    )
    .await?;
    assert_eq!(run.byte().await?, ENQ, "the host opens its answer");
    run.send(&[ACK]).await?;
    let mut text = Vec::new();
    loop {
        let first = run.byte().await?;
        if first == EOT {
            break;
        }
        let mut frame = vec![first];
        frame.extend(run.until(b"\n").await?);
        text.extend_from_slice(&frame);
        run.send(&[ACK]).await?;
    }
    let answer = String::from_utf8(text)?;
    assert!(answer.contains("L|1|Q\r"), "{answer:?}");
    assert!(
        !answer.contains("L|1|I"),
        "never no information: {answer:?}"
    );
    assert!(
        !answer.contains("\rO|"),
        "no order is ever sent: {answer:?}"
    );
    run.finish(["tcp_server", "astm_lis01", "astm_link", "astm_lis02"], 0)
        .await?;
    Ok(())
}

// ADR 0003 rule 1 and ADR 0034: with no room in the store, ENQ is refused.
#[tokio::test]
async fn astm_record_only_refuses_enq_without_storage() -> Result<(), Error> {
    let mut run = Recorder::start("astm", "").await?;
    run.space.store(0, Ordering::Release);
    run.send(&[ENQ]).await?;
    let mut reply = [0u8; 1];
    let read = tokio::time::timeout(
        analyte_fixtures::HANG_GUARD,
        tokio::io::AsyncReadExt::read(&mut run.peer, &mut reply),
    )
    .await?;
    // Either the chunk could not be stored (the link closes) or ENQ got NAK;
    // never ACK.
    match read {
        Ok(0) => {}
        Ok(_) => assert_eq!(reply[0], NAK, "no room: never ACK"),
        Err(error) => assert!(matches!(
            error.kind(),
            std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted
        )),
    }
    assert_eq!(
        run.count("SELECT count(*) FROM receipts")?,
        0,
        "nothing was kept, so nothing was acknowledged"
    );
    run.stop().await
}

/// Sends one MLLP block and reads the host's acknowledgment.
async fn hl7_exchange(run: &mut Recorder, message: &[u8]) -> Result<String, Error> {
    let wire = analyte_framing::hl7::encode(message, analyte_framing::hl7::Limit::new(1048576)?)?;
    run.send(&wire).await?;
    Ok(String::from_utf8(run.until(b"\x1c\r").await?)?)
}

// HL7 2.5.1 2.9.2 (original acknowledgment) and 2.3.1 2.22 (a query the host
// cannot process is answered AE); MLLP Release 1; ADR 0034.
#[tokio::test]
async fn hl7_record_only_accepts_results_and_answers_queries_with_ae() -> Result<(), Error> {
    let mut run = Recorder::start("hl7", "").await?;
    let result = hl7_exchange(
        &mut run,
        b"MSH|^~\\&|ANALYZER|LAB|LIS|LAB|20261123120000||ORU^R01^ORU_R01|result1|P|2.5.1\rPID|1||P1\rOBR|1|S1\rOBX|1|NM|SYN||5.2|mg/L|||||F\r",
    )
    .await?;
    assert!(result.contains("MSA|AA|result1"), "{result:?}");
    let query = hl7_exchange(
        &mut run,
        b"MSH|^~\\&|ANALYZER|LAB|LIS|LAB|20261123120100||QRY^Q02|query1|P|2.3.1\rQRD|20261123120100|R|D|query1|||RD|000124|OTH|||T\rQRF|ANALYZER|||||RCT|COR|ALL\r",
    )
    .await?;
    assert!(query.contains("MSA|AE|query1"), "{query:?}");
    assert!(
        !query.contains("MSA|AA"),
        "never an empty acceptance: {query:?}"
    );
    run.finish(["tcp_server", "mllp", "hl7_receiver", "hl7_v2"], 0)
        .await?;
    Ok(())
}

// YHLO LIS Protocol Manual V1.1 sections 2.3.1-2.3.2, 2.4.1; HL7 2.3.1 section 2.12.
#[tokio::test]
async fn hl7_record_only_keeps_all_iflash_result_categories() -> Result<(), Error> {
    let mut run = Recorder::start("hl7", "").await?;
    for (id, category) in [("00071", "0"), ("00072", "1"), ("00073", "2")] {
        // A payload byte from the analyzer's local code page is opaque to capture.
        let mut raw = format!(
            "MSH|^~\\&|YHLO|iFlash3000|||20261123120000||ORU^R01|{id}|P|2.3.1||||{category}||ASCII\rOBR|1|000124||SYN|"
        )
        .into_bytes();
        raw.extend_from_slice(b"\xe9\r");
        let reply = hl7_exchange(&mut run, &raw).await?;
        assert!(reply.contains(&format!("\rMSA|AA|{id}\r")), "{reply:?}");
        assert_eq!(run.kept()?, run.sent, "receipt must already be durable");
    }
    run.finish(["tcp_server", "mllp", "hl7_receiver", "hl7_v2"], 0)
        .await?;
    Ok(())
}

// BCI Link guide 161150-486 A 3-1 to 3-7 (Literal packet, checksum and
// acknowledgment before ETX); ADR 0034.
#[tokio::test]
async fn vidas_record_only_acknowledges_the_upload_and_keeps_it() -> Result<(), Error> {
    use analyte_framing::vidas::{Checksum, Config, HexOptions, Mode};
    let mut run = Recorder::start("vidas", "acknowledgment = \"literal\"\n").await?;
    run.send(&[ENQ]).await?;
    assert_eq!(run.byte().await?, ACK, "upload accepted");
    let packet = Config::new(
        Mode::Literal,
        80,
        1920,
        &[],
        Checksum {
            include_stx: false,
            include_suffix: false,
            hex: HexOptions {
                uppercase: false,
                accept_either_case: false,
            },
        },
    )?
    .encode(b"mtrsl|pi|si|ci00012a|rtSYN|tt|td|qlpositive|qn<0.1|y3mg/L|")?;
    run.send(&packet).await?;
    assert_eq!(
        run.byte().await?,
        ACK,
        "the packet is acknowledged once kept"
    );
    run.send(&[3, EOT]).await?;
    run.finish(
        [
            "tcp_server",
            "vidas_literal",
            "vidas_literal",
            "vidas_literal",
        ],
        0,
    )
    .await?;
    Ok(())
}

// BCI Link LIS guide 161150-486 C pp. 3-1 to 3-4; ADR 0034.
#[tokio::test]
async fn vidas_record_only_keeps_bad_packet_before_nak_and_accepts_retry() -> Result<(), Error> {
    use analyte_framing::vidas::{ETX, NAK};
    for mode in ["literal", "alternate"] {
        let answers = format!("acknowledgment = \"{mode}\"\n");
        let mut run = Recorder::start("vidas", &answers).await?;
        run.send(&[ENQ]).await?;
        assert_eq!(run.byte().await?, ACK);
        let tail = if mode == "alternate" { &[ETX][..] } else { &[] };

        run.send(b"\x02\x1eA\x1d00").await?;
        if !tail.is_empty() {
            run.send(tail).await?;
        }
        assert_eq!(run.byte().await?, NAK);
        assert_eq!(run.kept()?, run.sent);

        run.send(b"\x02\x1eA\x1d7c").await?;
        if !tail.is_empty() {
            run.send(tail).await?;
        }
        assert_eq!(run.byte().await?, ACK);
        assert_eq!(run.kept()?, run.sent);
        if mode == "literal" {
            run.send(&[ETX]).await?;
        }
        run.send(&[EOT]).await?;
        run.finish(
            [
                "tcp_server",
                "vidas_literal",
                "vidas_literal",
                "vidas_literal",
            ],
            0,
        )
        .await?;
    }
    Ok(())
}

/// One poll as the Autolyser sees it: STX, its ACK, the request byte and EOT.
async fn autolyser_poll(run: &mut Recorder) -> Result<(), Error> {
    assert_eq!(run.byte().await?, 2, "STX opens the poll");
    run.send(&[ACK]).await?;
    assert_eq!(
        run.byte().await?,
        b'R',
        "only the next report is ever asked for"
    );
    assert_eq!(run.byte().await?, EOT);
    Ok(())
}

fn autolyser_report() -> Result<Vec<u8>, Error> {
    use analyte_autolyser::syntax::{Field, List, PatientCode, Report, Sample, Value};
    Ok(analyte_framing::autolyser::encode(
        Report::new(
            &PatientCode::from_eight(b"00012aBC")?,
            List::Routine,
            Sample::Serum,
            None,
            &[Value {
                code: Field::new(b"SYN ")?,
                text: Field::new(b"<0.1   ")?,
            }],
        )?
        .raw,
    ))
}

// Autolyser Operator's Manual Rev18 pp. 175-179 (R next report, L last
// report, A archive; checksum envelope); ADR 0010 Autolyser row; ADR 0034.
#[tokio::test]
async fn autolyser_record_only_asks_only_for_the_next_report() -> Result<(), Error> {
    let mut run = Recorder::start("autolyser", "poll_seconds = 1\ncontrols = false\n").await?;
    autolyser_poll(&mut run).await?;
    let report = autolyser_report()?;
    run.send(&report).await?;
    // After the lab's interval, the next R; a NAK means no report is waiting.
    autolyser_poll(&mut run).await?;
    run.send(&[NAK]).await?;
    autolyser_poll(&mut run).await?;
    let mut corrupt = report;
    *corrupt.get_mut(0).ok_or("fixture")? = b'9';
    run.send(&corrupt).await?;
    // A corrupt response stops the asking: no L, no A, no further R.
    run.silent(Duration::from_millis(2500)).await?;
    let uncertain = run.count("SELECT count(*) FROM poll_intents WHERE state='uncertain'")?;
    assert_eq!(uncertain, 1, "the R guard waits for an operator");
    run.finish(["tcp_server", "autolyser", "autolyser", "autolyser"], 1)
        .await?;
    Ok(())
}

// ADR 0034 decisions 6 and 10: a message kept by a record-only connection is
// never reprocessed, and once the machine is connected normally, startup
// recovery leaves it alone: nothing is read, parked or delivered from it.
#[tokio::test]
async fn recorded_messages_are_never_read_later() -> Result<(), Error> {
    let mut run = Recorder::start("astm", "").await?;
    astm_transfer(&mut run, &[b"H|\\^&|||Analyzer\r", b"L|1|N\r"]).await?;
    let finished = run
        .finish(["tcp_server", "astm_lis01", "astm_link", "astm_lis02"], 0)
        .await?;
    let program = analyte::installation::config::Program::load(&finished.path)?;
    let mut store = analyte::open_system_store(&program).await?;
    let db = rusqlite::Connection::open(finished.directory.path().join("data/analyte.db"))?;
    let metadata: String = db.query_row("SELECT metadata FROM receipts LIMIT 1", [], |row| {
        row.get(0)
    })?;
    let metadata: analyte_model::ReceiptMetadata = serde_json::from_str(&metadata)?;
    let refused = analyte::operator::recovery::reprocess(
        &store,
        &program,
        analyte::operator::recovery::ReprocessTarget::Receipt(metadata.id),
        analyte_model::Identifier::new("it-test")?,
        analyte_model::Reason::new("synthetic check")?,
    )
    .await;
    store
        .shutdown(std::time::Instant::now() + analyte_fixtures::HANG_GUARD)
        .await?;
    let refused = refused.err().ok_or("a recorded message was reprocessed")?;
    assert!(refused.to_string().contains("record-only"), "{refused}");
    // Connect the same machine normally: a ready ASTM definition and a lab app.
    let root = finished.directory.path();
    std::fs::write(
        analyte_fixtures::definition_path(root, "recorder"),
        analyte_fixtures::astm_tcp(),
    )?;
    let site = std::fs::read_to_string(root.join("site.toml"))?;
    let endpoint = site
        .lines()
        .find(|line| line.starts_with("endpoint"))
        .ok_or("endpoint")?;
    std::fs::write(
        root.join("site.toml"),
        include_str!("../../../core/config/examples/site.toml")
            .replace("endpoint = \"127.0.0.1:5600\"", endpoint),
    )?;
    let text = std::fs::read_to_string(&finished.path)?.replace("record.toml", "machine.toml");
    std::fs::write(&finished.path, text)?;
    let mut program = analyte::installation::config::Program::load(&finished.path)?;
    assert!(
        !program
            .machines()
            .first()
            .ok_or("machine")?
            .snapshot()
            .effective()
            .records_only()
    );
    program.set_loopback_destination_for_test("demo", "http://127.0.0.1:1".into())?;
    let secrets = analyte::installation::secrets::Environment::load_with(&program, |name| {
        Some(
            if name.ends_with("API") {
                "synthetic-api"
            } else {
                analyte_fixtures::SECRET
            }
            .into(),
        )
    })?;
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let (ready, started) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(analyte::runtime::service::serve_loopback_test(
        program,
        Box::new(analyte_fixtures::AdjustableSpace(std::sync::Arc::new(
            std::sync::atomic::AtomicU64::new(u64::MAX),
        ))),
        async {
            stopped.await?;
            Ok(())
        },
        secrets,
        Some(ready),
    ));
    tokio::time::timeout(analyte_fixtures::HANG_GUARD, started).await??;
    stop.send(()).map_err(|()| "service ended early")?;
    tokio::time::timeout(analyte_fixtures::HANG_GUARD, task).await???;
    for sql in [
        "SELECT count(*) FROM interpretations",
        "SELECT count(*) FROM parked",
        "SELECT count(*) FROM outbox",
    ] {
        let count: i64 = db.query_row(sql, [], |row| row.get(0))?;
        assert_eq!(count, 0, "{sql}");
    }
    Ok(())
}
