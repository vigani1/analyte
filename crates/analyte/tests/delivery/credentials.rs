//! Tests HTTP delivery retries and stable delivery identities.
use super::*;
use analyte_fixtures::ConfigEdit as _;

// Plan §13; ADR 0008
#[tokio::test]
async fn retransmission_and_correction_deliver_once() -> Result<(), Error> {
    let directory = tempfile::tempdir()?;
    let path = config(directory.path(), true)?;
    let receiver_path = directory.path().join("reference.db");
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let mut reference = Reference::start(&receiver_path, listener, false).await?;
    let service = Running::start(&path, &reference.url, "synthetic-api").await?;
    let mut hl7_peer = connect(&path, "hl7").await?;
    let mut astm_peer = connect(&path, "astm").await?;
    assert!(hl7(&mut hl7_peer, ORU).await?.contains("MSA|AA|message1"));
    astm(&mut astm_peer, ASTM).await?;
    let first = reference.event().await?;
    let second = reference.event().await?;
    for event in [&first, &second] {
        let body: serde_json::Value = serde_json::from_slice(&event.bytes)?;
        let machine = body
            .pointer("/source/analyzer")
            .and_then(serde_json::Value::as_str)
            .ok_or("machine missing")?;
        assert!(["hl7", "astm"].contains(&machine));
        assert_content(
            event,
            machine,
            if machine == "hl7" { "00012a" } else { "00012A" },
            "12.30",
            "final",
            0,
        )?;
    }
    assert_ne!(first.id, second.id);
    wait_state(&path, "done", 2).await?;
    // The host closes a machine link idle past its read-idle limit, and the
    // delivery waits above have no bound below that limit on a loaded runner.
    let mut astm_peer = reconnect(&path, "astm", astm_peer).await?;
    let mut hl7_peer = reconnect(&path, "hl7", hl7_peer).await?;
    astm(&mut astm_peer, ASTM).await?;
    let correction = String::from_utf8(ORU.to_vec())?
        .rewrite("12.30", "<0.1")
        .rewrite("|||||F", "|||||C")
        .rewrite("message1", "message2");
    assert!(
        hl7(&mut hl7_peer, correction.as_bytes())
            .await?
            .contains("MSA|AA|message2")
    );
    let corrected = reference.event().await?;
    assert_content(&corrected, "hl7", "00012a", "<0.1", "corrected", 1)?;
    wait_state(&path, "done", 3).await?;
    let mut hl7_peer = reconnect(&path, "hl7", hl7_peer).await?;
    for mode in ["T", "D"] {
        let message = String::from_utf8(ORU.to_vec())?
            .rewrite("|P|2.5.1", &format!("|{mode}|2.5.1"))
            .rewrite("00012a", &format!("synthetic-{mode}"));
        assert!(
            hl7(&mut hl7_peer, message.as_bytes())
                .await?
                .contains("MSA|AA")
        );
    }
    // ADR 0010 (2026-11-26): a conflicting resend is stored and held for an
    // operator, so it is accepted; the receiver count below proves it was not
    // delivered.
    let conflict = String::from_utf8(ORU.to_vec())?.rewrite("12.30", "99.9");
    assert!(
        hl7(&mut hl7_peer, conflict.as_bytes())
            .await?
            .contains("MSA|AA|message1")
    );
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_analyte"))
        .arg("status")
        .arg(&*path)
        .output()?;
    assert!(output.status.success());
    let status = String::from_utf8(output.stdout)?;
    assert!(status.contains("machine=hl7 last_receipt=20"));
    assert!(status.contains("machine=astm last_receipt=20"));
    assert!(!status.contains("00012"));
    service.stop().await?;
    assert_eq!(receiver_count(&receiver_path)?, 3);
    let receiver = rusqlite::Connection::open(&receiver_path)?;
    for event in [&first, &second, &corrected] {
        let stored: Vec<u8> = receiver.query_row(
            "SELECT body FROM deliveries WHERE id=?1",
            [&event.id],
            |r| r.get(0),
        )?;
        assert_eq!(stored, event.bytes);
    }
    assert!(reference.events.try_recv().is_err());
    reference.close().await?;
    Ok(())
}

// ADR 0008
#[tokio::test]
async fn receiver_outage_retries_same_delivery_on_return() -> Result<(), Error> {
    let directory = tempfile::tempdir()?;
    let path = config(directory.path(), false)?;
    // A real outage refuses connections; an unserved listener would instead
    // queue half-finished attempts that the receiver later answers first.
    let socket = tokio::net::TcpSocket::new_v4()?;
    socket.bind("127.0.0.1:0".parse()?)?;
    let address = socket.local_addr()?;
    let service = Running::start(&path, &format!("http://{address}"), "synthetic-api").await?;
    let mut peer = connect(&path, "hl7").await?;
    assert!(hl7(&mut peer, ORU).await?.contains("MSA|AA"));
    let (id, bytes): (String, Vec<u8>) =
        database(&path)?.query_row("SELECT id,payload FROM outbox", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?;
    // Initial enqueue is also pending; prove an outage attempt actually failed.
    tokio::time::timeout(HANG_GUARD, async {
        loop {
            let attempts: i64 =
                database(&path)?
                    .query_row("SELECT count(*) FROM attempts", [], |row| row.get(0))?;
            if attempts > 0 {
                return Ok::<_, Error>(());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await??;
    let listener = socket.listen(1024)?;
    let mut reference =
        Reference::start(&directory.path().join("reference.db"), listener, false).await?;
    let event = reference.event().await?;
    assert_eq!(format!("\"{}\"", event.id), id);
    assert_eq!(event.bytes, bytes);
    assert_content(&event, "hl7", "00012a", "12.30", "final", 0)?;
    wait_state(&path, "done", 1).await?;
    service.stop().await?;
    reference.close().await?;
    Ok(())
}

// ADR 0008
#[tokio::test]
async fn wrong_api_key_pauses_destination_visibly() -> Result<(), Error> {
    let directory = tempfile::tempdir()?;
    let path = config(directory.path(), false)?;
    let receiver_path = directory.path().join("reference.db");
    let reference = Reference::start(
        &receiver_path,
        TcpListener::bind("127.0.0.1:0").await?,
        false,
    )
    .await?;
    let service = Running::start(&path, &reference.url, "synthetic-wrong-key").await?;
    let mut peer = connect(&path, "hl7").await?;
    assert!(hl7(&mut peer, ORU).await?.contains("MSA|AA"));
    wait_state(&path, "paused", 1).await?;
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_analyte"))
        .arg("status")
        .arg(&*path)
        .output()?;
    assert!(output.status.success());
    let status = String::from_utf8(output.stdout)?;
    assert!(status.contains("paused=true"));
    assert!(!status.contains("synthetic-wrong-key"));
    assert!(!status.contains("00012"));
    assert_eq!(receiver_count(&receiver_path)?, 0);
    service.stop().await?;
    reference.close().await?;
    Ok(())
}

// ADR 0008
#[tokio::test]
async fn interrupted_accepted_delivery_recovers_as_duplicate() -> Result<(), Error> {
    for refused in [true, false] {
        let directory = tempfile::tempdir()?;
        let path = config(directory.path(), false)?;
        let receiver_path = directory.path().join("reference.db");
        let (outage, url) = AppOutage::start(&receiver_path, refused).await?;
        let service = Running::start(&path, &url, "synthetic-api").await?;
        let mut peer = connect(&path, "hl7").await?;
        assert!(hl7(&mut peer, ORU).await?.contains("MSA|AA"));
        wait_outage_attempt(&path).await?;
        service.stop().await?;
        let mut reference = outage.recover(&receiver_path).await?;
        let program = program(&path, &url)?;
        let mut store = service::open_store(&program, Box::new(Space)).await?;
        let secrets = secrets(&program, "synthetic-api")?;
        let destinations = [analyte_sink::Destination::for_loopback_test(
            analyte_model::Identifier::new("demo")?,
            &url,
            analyte_model::Identifier::new("demo")?,
            analyte_sink::Limits {
                request_timeout: Duration::from_secs(30),
                store_timeout: analyte_fixtures::STORE_BUDGET,
                backoff_min: Duration::from_secs(1),
                backoff_max: Duration::from_secs(3),
                freshness: Duration::from_secs(30),
                poll_interval: Duration::from_secs(1),
                page_size: 100,
                max_pages: 100,
            },
        )?];
        // A prior refusal may have scheduled a future retry. Wait under a deadline
        // until the real worker claims it; cancel only after the receiver commits.
        let original = tokio::time::timeout(HANG_GUARD, async {
        let worker = analyte_sink::Worker::for_destination(&store, &destinations[0], &secrets);
        loop {
            let attempt = worker.deliver_once();
            tokio::pin!(attempt);
            tokio::select! {
                event = reference.event() => return event,
                outcome = &mut attempt => {
                    if outcome? != analyte_sink::DeliveryOutcome::Idle { return Err("attempt unexpectedly completed".into()); }
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await??;
        assert!(original.outcome.contains("accepted"));
        assert_eq!(receiver_count(&receiver_path)?, 1);
        wait_state(&path, "in_flight", 1).await?;
        // Dropping the attempt above models an interrupted owner, not a completed HTTP retry.
        store
            .shutdown(
                std::time::Instant::now()
                    .checked_add(analyte_fixtures::STORE_BUDGET)
                    .ok_or("deadline")?,
            )
            .await?;
        let restarted = Running::start(&path, &url, "synthetic-api").await?;
        let duplicate = reference.event().await?;
        assert!(duplicate.outcome.contains("duplicate"));
        assert_eq!(duplicate.id, original.id);
        assert_eq!(duplicate.bytes, original.bytes);
        wait_state(&path, "done", 1).await?;
        assert_eq!(receiver_count(&receiver_path)?, 1);
        let audit: String = database(&path)?.query_row(
            "SELECT evidence FROM audit ORDER BY sequence DESC LIMIT 1",
            [],
            |r| r.get(0),
        )?;
        assert!(audit.contains("ADR 0008"));
        assert!(audit.contains(&original.id));
        let interrupted: i64 = database(&path)?.query_row(
            "SELECT count(*) FROM attempts WHERE outcome=?1",
            ["\"uncertain (interrupted)\""],
            |r| r.get(0),
        )?;
        assert_eq!(interrupted, 1);
        restarted.stop().await?;
        reference.close().await?;
    }
    Ok(())
}

// ADR 0008
#[tokio::test]
async fn shutdown_during_http_wait_resends_same_delivery() -> Result<(), Error> {
    let directory = tempfile::tempdir()?;
    let path = config(directory.path(), false)?;
    // A 3-second request deadline plus the 2-second store deadline fits the 5-second drain.
    timeouts::set_request_timeout(directory.path(), "hl7", 3)?;
    let receiver_path = directory.path().join("reference.db");
    let mut reference = Reference::start(
        &receiver_path,
        TcpListener::bind("127.0.0.1:0").await?,
        true,
    )
    .await?;
    let service = Running::start(&path, &reference.url, "synthetic-api").await?;
    let mut peer = connect(&path, "hl7").await?;
    assert!(hl7(&mut peer, ORU).await?.contains("MSA|AA"));
    let original = reference.event().await?;
    assert!(original.outcome.contains("accepted"));
    // The receiver has committed, but its answer is deliberately withheld. The
    // request deadline expires inside the shutdown drain and preserves a retry.
    tokio::time::timeout(HANG_GUARD, service.stop()).await??;
    let restarted = Running::start(&path, &reference.url, "synthetic-api").await?;
    let duplicate = reference.event().await?;
    assert_eq!(duplicate.id, original.id);
    assert_eq!(duplicate.bytes, original.bytes);
    assert!(duplicate.outcome.contains("duplicate"));
    wait_state(&path, "done", 1).await?;
    assert_eq!(receiver_count(&receiver_path)?, 1);
    restarted.stop().await?;
    reference.close().await?;
    Ok(())
}

// ADR 0008
#[test]
fn key_rotation_loads_each_named_key() -> Result<(), Error> {
    use analyte_sink::SecretsSource as _;
    let directory = tempfile::tempdir()?;
    let path = config(directory.path(), false)?;
    let text = std::fs::read_to_string(&path)? + "\n[signing_rotation]\ndemo = ['next-signing']\n";
    std::fs::write(&path, text)?;
    let program = Program::load(&path)?;
    let secrets = secrets(&program, "synthetic-api")?;
    assert_eq!(
        secrets
            .credentials(&analyte_model::Identifier::new("demo")?)?
            .signing_secrets
            .len(),
        2
    );
    let error = analyte::installation::secrets::Environment::load_with(&program, |name| {
        if name == "ANALYTE_SECRET_NEXT_SIGNING" {
            None
        } else {
            Some(
                if name.ends_with("API") {
                    "synthetic-api"
                } else {
                    SECRET
                }
                .into(),
            )
        }
    })
    .err()
    .ok_or("missing startup error")?;
    assert_eq!(
        error.to_string(),
        "credential next-signing is missing or empty"
    );
    Ok(())
}

// CLSI LIS01-A2; HL7 2.5.1; ADR 0003
#[tokio::test]
async fn recordings_store_before_ack_and_deliver_once() -> Result<(), Error> {
    let directory = tempfile::tempdir()?;
    let path = config(directory.path(), true)?;
    let receiver_path = directory.path().join("reference.db");
    let mut reference = Reference::start(
        &receiver_path,
        TcpListener::bind("127.0.0.1:0").await?,
        false,
    )
    .await?;
    let service = Running::start(&path, &reference.url, "synthetic-api").await?;
    for name in ["astm", "hl7"] {
        let (header, events) = recording(name)?;
        assert_eq!(
            header.evidence,
            analyte_transport::recording::Evidence::Synthetic
        );
        let loaded = Program::load(&path)?;
        let settings = loaded
            .machines()
            .iter()
            .find(|m| m.id().as_str() == name)
            .ok_or("machine absent")?
            .snapshot()
            .effective();
        let deadline =
            Duration::from_millis(u64::from(settings.stack().timeouts().send_timeout_ms()));
        let mut peer = connect(&path, name).await?;
        if name == "astm" {
            analyte_sim::replay(
                &mut peer,
                events.get(..4).ok_or("prefix missing")?,
                &[],
                deadline,
            )
            .await?;
            // The frame ACK is already observable: a separate store connection must see the exact journal.
            let raw: Vec<u8> = database(&path)?.query_row(
                "SELECT raw FROM frames ORDER BY sequence LIMIT 1",
                [],
                |r| r.get(0),
            )?;
            assert_eq!(raw, events.get(2).ok_or("frame missing")?.bytes);
            analyte_sim::replay(
                &mut peer,
                events.get(4..).ok_or("EOT missing")?,
                &[],
                deadline,
            )
            .await?;
        } else {
            analyte_sim::replay(&mut peer, &events, &header.exceptions, deadline).await?;
            let wire = &events.first().ok_or("message absent")?.bytes;
            let raw: Vec<u8> = database(&path)?.query_row(
                "SELECT raw FROM receipts WHERE raw=?1",
                [wire],
                |r| r.get(0),
            )?;
            assert_eq!(&raw, wire);
        }
        let event = reference.event().await?;
        assert_content(
            &event,
            name,
            if name == "hl7" { "00012a" } else { "00012A" },
            "12.30",
            "final",
            0,
        )?;
    }
    wait_state(&path, "done", 2).await?;
    assert_eq!(receiver_count(&receiver_path)?, 2);
    // Restart must not create a second delivery from either durable receipt.
    service.stop().await?;
    let restarted = Running::start(&path, &reference.url, "synthetic-api").await?;
    let peer = connect(&path, "hl7").await?;
    wait_state(&path, "done", 2).await?;
    restarted.stop().await?;
    drop(peer);
    assert_eq!(receiver_count(&receiver_path)?, 2);
    reference.close().await?;
    Ok(())
}

// CLSI LIS01-A2 §6.3.3, §6.5.1
#[tokio::test]
async fn broken_checksum_is_retained_nacked_and_undelivered() -> Result<(), Error> {
    failure_recording_case("astm-broken-checksum", false).await
}
// CLSI LIS01-A2 §8.1, §6.3.1.2
#[tokio::test]
async fn split_and_merged_reads_ack_only_complete_frames() -> Result<(), Error> {
    failure_recording_case("astm-split-merged", true).await
}
// CLSI LIS01-A2 §6.3.2, §6.3.4
#[tokio::test]
async fn lost_ack_repeats_ack_without_second_delivery() -> Result<(), Error> {
    failure_recording_case("astm-lost-ack", true).await
}
// ADR 0003; CLSI LIS01-A2 §6.3.1.2
#[tokio::test]
async fn disconnect_mid_frame_keeps_bytes_without_ack() -> Result<(), Error> {
    failure_recording_case("astm-disconnect", false).await
}

// ADR 0003; CLSI LIS01-A2 §6.3.2, §6.3.4
#[cfg(feature = "test-hooks")]
#[tokio::test]
async fn restart_after_store_before_ack_delivers_once() -> Result<(), Error> {
    use tokio::io::AsyncReadExt;
    let directory = tempfile::tempdir()?;
    let path = config(directory.path(), true)?;
    let receiver_path = directory.path().join("reference.db");
    let mut reference = Reference::start(
        &receiver_path,
        TcpListener::bind("127.0.0.1:0").await?,
        false,
    )
    .await?;
    let (hook, control) = analyte::runtime::test_hooks::Committed::new();
    let mut loaded = program(&path, &reference.url)?;
    loaded.pause_astm_committed_for_test("astm", hook)?;
    let service = Running::start_program(loaded, "synthetic-api").await?;
    let (header, before_ack) = recording("astm-restart-before-ack")?;
    let deadline = recording_deadline(&path)?;
    let mut peer = connect(&path, "astm").await?;
    analyte_sim::replay(&mut peer, &before_ack, &header.exceptions, deadline).await?;
    control.reached().await?;
    let expected = &before_ack.get(2).ok_or("frame absent")?.bytes;
    assert_eq!(recording_frames(&path)?, vec![expected.clone()]);
    assert_eq!(receiver_count(&receiver_path)?, 0);
    let db = database(&path)?;
    let original_transfer: String =
        db.query_row("SELECT transfer FROM frames", [], |r| r.get(0))?;
    assert_eq!(
        db.query_row("SELECT count(*) FROM outbox", [], |r| r.get::<_, i64>(0))?,
        0
    );
    let mut byte = [0];
    assert!(
        // Deliberate quiet window: no reply may arrive.
        tokio::time::timeout(Duration::from_millis(50), peer.read(&mut byte))
            .await
            .is_err(),
        "ACK escaped paused durable boundary"
    );
    control.stop();
    assert_eq!(
        tokio::time::timeout(HANG_GUARD, peer.read(&mut byte)).await??,
        0,
        "connection must close without sending the frame ACK"
    );
    service.stop().await?;
    assert_eq!(recording_frames(&path)?, vec![expected.clone()]);
    let received = receiver_count(&receiver_path)?;
    let details = if received == 0 {
        Vec::new()
    } else {
        let db = rusqlite::Connection::open(&receiver_path)?;
        let mut statement = db.prepare("SELECT kind,body FROM deliveries LIMIT 2")?;
        statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?
    };
    assert_eq!(
        received, 0,
        "unexpected delivery after stopping before ACK: {details:?}"
    );
    drop(peer);
    let restarted = Running::start(&path, &reference.url, "synthetic-api").await?;
    let mut peer = connect(&path, "astm").await?;
    let (header, retransmission) = recording("astm")?;
    analyte_sim::replay(&mut peer, &retransmission, &header.exceptions, deadline).await?;
    recording_eof(&mut peer).await?;
    let delivered = reference.event().await?;
    assert_content(&delivered, "astm", "00012A", "12.30", "final", 0)?;
    wait_state(&path, "done", 1).await?;
    restarted.stop().await?;
    // Distinct transfer evidence is retained; only the completed transfer becomes a result.
    let mut expected_frames = vec![expected.clone()];
    expected_frames.extend(
        retransmission
            .iter()
            .filter(|event| event.bytes.first() == Some(&2))
            .map(|event| event.bytes.clone()),
    );
    assert_eq!(recording_frames(&path)?, expected_frames);
    let original: Vec<u8> = database(&path)?.query_row(
        "SELECT raw FROM frames WHERE transfer=?1",
        [&original_transfer],
        |r| r.get(0),
    )?;
    assert_eq!(&original, expected);
    assert_eq!(
        database(&path)?.query_row("SELECT count(*) FROM observation_revisions", [], |r| r
            .get::<_, i64>(0))?,
        1
    );
    assert_eq!(
        database(&path)?.query_row("SELECT count(*) FROM outbox", [], |r| r.get::<_, i64>(0))?,
        1
    );
    assert_eq!(receiver_count(&receiver_path)?, 1);
    reference.close().await?;
    Ok(())
}

// ADR 0037; CLSI LIS01-A2 §8.3.3; LIS02-A2 §8.4.3: a de-identified copy of an
// upload replays through the program and delivers the same result, under the
// stand-in sample ID.
#[tokio::test]
async fn de_identified_upload_delivers_the_same_result() -> Result<(), Error> {
    use analyte::operator::captures::deidentify;
    let directory = tempfile::tempdir()?;
    let path = config(directory.path(), true)?;
    let receiver_path = directory.path().join("reference.db");
    let mut reference = Reference::start(
        &receiver_path,
        TcpListener::bind("127.0.0.1:0").await?,
        false,
    )
    .await?;
    let service = Running::start(&path, &reference.url, "synthetic-api").await?;
    let (header, events) = recording("astm")?;
    let copy = deidentify::de_identify(&header, &events, None)?;
    let order = copy
        .events
        .iter()
        .find(|event| event.bytes.windows(4).any(|w| w == b"O|1|"))
        .ok_or("order frame")?;
    let text = std::str::from_utf8(&order.bytes)?;
    let sample = text
        .split('|')
        .nth(2)
        .and_then(|field| field.split('\r').next())
        .ok_or("sample ID")?;
    assert_ne!(sample, "00012A");
    assert_eq!(sample.len(), 6);
    assert!(sample.starts_with("000"));
    let mut peer = connect(&path, "astm").await?;
    let deadline = recording_deadline(&path)?;
    analyte_sim::replay(&mut peer, &copy.events, &copy.header.exceptions, deadline).await?;
    recording_eof(&mut peer).await?;
    let delivered = reference.event().await?;
    assert_content(&delivered, "astm", sample, "12.30", "final", 0)?;
    wait_state(&path, "done", 1).await?;
    service.stop().await?;
    reference.close().await?;
    Ok(())
}
