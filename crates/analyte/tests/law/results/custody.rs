//! Tests LAW result receipt and durable delivery.
use super::*;
use crate::support::step;
use analyte_fixtures::ConfigEdit as _;

#[tokio::test]
async fn law_tf2b_3_29_4_b3_results_deliver_once_retransmit_and_correct_only_c() -> Result<(), Error>
{
    let temp = tempfile::tempdir()?;
    let path = vendor_config(temp.path(), "law", &definition())?;
    let receiver_path = temp.path().join("reference.db");
    let mut reference = Reference::start(
        &receiver_path,
        TcpListener::bind("127.0.0.1:0").await?,
        false,
    )
    .await?;
    let running = Running::start(&path, &reference.url, "synthetic-api").await?;
    let mut peer = connect(&path, "law").await?;
    // B.7.6 p.82: unknown non-null AWOS is rejected, not treated as unsolicited.
    let unknown = result("never-downloaded", "awos-1^LAB", "", "12.30", "F");
    assert!(
        hl7(&mut peer, unknown.as_bytes())
            .await?
            .contains("MSA|AR|never-downloaded")
    );
    assert_eq!(count(&path, "outbox")?, 0);
    let first = result("first", "\"\"", "awos-1^LAB", "12.30", "F");
    let ack = hl7(&mut peer, first.as_bytes()).await?;
    assert!(ack.contains("ACK^R22^ACK"), "{ack}");
    assert!(ack.contains("MSA|AA|first"), "{ack}");
    assert!(ack.contains("LAB-29^IHE"));
    check(&reference.event().await?, "awos-1", "LAB", 0, "12.30")?;
    wait_state(&path, "done", 1).await?;
    assert!(
        hl7(
            &mut peer,
            first.rewrite("|first|", "|retransmit|").as_bytes()
        )
        .await?
        .contains("MSA|AA|retransmit")
    );
    assert_eq!(count(&path, "outbox")?, 1);
    let conflict = result("conflict", "\"\"", "awos-1^LAB", "99.9", "F");
    assert!(
        hl7(&mut peer, conflict.as_bytes())
            .await?
            .contains("MSA|AR|conflict")
    );
    assert_eq!(count(&path, "outbox")?, 1);
    let correction = result("correction", "\"\"", "awos-1^LAB", "13.40", "C");
    assert!(
        hl7(&mut peer, correction.as_bytes())
            .await?
            .contains("MSA|AA|correction")
    );
    let event = reference.event().await?;
    check(&event, "awos-1", "LAB", 1, "13.40")?;
    let body: serde_json::Value = serde_json::from_slice(&event.bytes)?;
    assert_eq!(
        body.pointer("/results/0/parts/0/source_status")
            .and_then(|v| v.as_str()),
        Some("C")
    );
    assert_eq!(count(&path, "observation_revisions")?, 2);
    wait_state(&path, "done", 2).await?;
    running.stop().await?;
    assert_eq!(receiver_count(&receiver_path)?, 2);
    assert!(reference.events.try_recv().is_err());
    reference.close().await?;
    Ok(())
}

#[tokio::test]
async fn law_tf2b_3_29_4_unsolicited_uses_explicit_analyzer_order_not_invented_awos()
-> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let path = vendor_config(temp.path(), "law", &definition())?;
    let receiver_path = temp.path().join("reference.db");
    let mut reference = Reference::start(
        &receiver_path,
        TcpListener::bind("127.0.0.1:0").await?,
        false,
    )
    .await?;
    let running = step(
        "start unsolicited service",
        Running::start(&path, &reference.url, "synthetic-api"),
    )
    .await?;
    let mut peer = step("connect unsolicited peer", connect(&path, "law")).await?;
    let message = result("unsolicited", "\"\"", "run-1^ANALYZER", "12.30", "F");
    assert!(
        step(
            "receive unsolicited LAW AA",
            hl7(&mut peer, message.as_bytes())
        )
        .await?
        .contains("MSA|AA|unsolicited")
    );
    check(
        &step("receive unsolicited delivery", reference.event()).await?,
        "run-1",
        "ANALYZER",
        0,
        "12.30",
    )?;
    let unresolved = result("unresolved", "\"\"", "", "12.30", "F");
    assert!(
        step(
            "receive unresolved LAW AE",
            hl7(&mut peer, unresolved.as_bytes())
        )
        .await?
        .contains("MSA|AE|unresolved")
    );
    assert!(count(&path, "parked")? > 0);
    assert_eq!(count(&path, "outbox")?, 1);
    step("commit unsolicited done", wait_state(&path, "done", 1)).await?;
    step("stop unsolicited service", running.stop()).await?;
    assert_eq!(receiver_count(&receiver_path)?, 1);
    step("close unsolicited receiver", reference.close()).await?;
    Ok(())
}

#[tokio::test]
async fn law_tf2b_3_29_4_b7_uninterpretable_is_parked_with_error_ack_and_no_delivery()
-> Result<(), Error> {
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
    for (message, code) in [
        (
            result("unknown-test", "\"\"", "awos-1^LAB", "12.30", "F")
                .rewrite("SYN^Synthetic^99LAB", "UNKNOWN^Unknown^99LAB"),
            "AE",
        ),
        (
            result("bad-status", "\"\"", "awos-1^LAB", "12.30", "D"),
            "AE",
        ),
        (
            result("bad-tcd", "\"\"", "awos-1^LAB", "12.30", "F") + "TCD|OTHER^Other^99LAB\r",
            "AR",
        ),
    ] {
        let ack = hl7(&mut peer, message.as_bytes()).await?;
        assert!(ack.contains(&format!("MSA|{code}|")), "{ack}");
        assert!(ack.contains("ACK^R22^ACK") && ack.contains("ERR|"));
        let raw: Vec<u8> = database(&path)?.query_row(
            "SELECT raw FROM receipts ORDER BY rowid DESC LIMIT 1",
            [],
            |r| r.get(0),
        )?;
        assert_eq!(
            raw,
            analyte_framing::hl7::encode(
                message.as_bytes(),
                analyte_framing::hl7::Limit::new(1048576)?
            )?
        );
    }
    assert_eq!(count(&path, "receipts")?, 3);
    assert_eq!(count(&path, "interpretations")?, 3);
    assert!(count(&path, "parked")? >= 3);
    assert_eq!(count(&path, "outbox")?, 0);
    running.stop().await?;
    assert_eq!(receiver_count(&receiver_path)?, 0);
    reference.close().await?;
    Ok(())
}

#[tokio::test]
async fn law_tf2b_3_29_4_b3_b7_conflict_rejects_batch_and_complete_og_distinguishes_runs()
-> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let path = vendor_config(temp.path(), "law", &definition())?;
    let receiver_path = temp.path().join("reference.db");
    let mut reference = Reference::start(
        &receiver_path,
        TcpListener::bind("127.0.0.1:0").await?,
        false,
    )
    .await?;
    let running = Running::start(&path, &reference.url, "synthetic-api").await?;
    let mut peer = connect(&path, "law").await?;
    let first = result(
        "control-id-longer-than-generic-hl7-twenty",
        "\"\"",
        "awos-1^LAB",
        "12.30",
        "F",
    )
    .rewrite("^99LAB|1|12.30", "^99LAB|1^1^1|12.30");
    assert!(
        hl7(&mut peer, first.as_bytes())
            .await?
            .contains("MSA|AA|control-id-longer-than-generic-hl7-twenty")
    );
    check(&reference.event().await?, "awos-1", "LAB", 0, "12.30")?;
    // C.7 pp.102-103: OG.1 stays 1; changing OG.3 distinguishes another
    // observation within the same run, not a second final run.
    let rerun = first
        .rewrite("control-id-longer-than-generic-hl7-twenty", "rerun")
        .rewrite("|1^1^1|", "|1^1^2|");
    assert!(
        hl7(&mut peer, rerun.as_bytes())
            .await?
            .contains("MSA|AA|rerun")
    );
    check(&reference.event().await?, "awos-1", "LAB", 0, "12.30")?;
    let conflict = first
        .rewrite("control-id-longer-than-generic-hl7-twenty", "mixed")
        .rewrite("12.30", "99.9");
    let new = result("new", "\"\"", "awos-2^LAB", "10.10", "F");
    let (_, new_body) = new.split_once('\r').ok_or("synthetic header absent")?;
    let mixed = format!("{conflict}{}", new_body.replacen("SPM|1|", "SPM|2|", 1));
    let ack = hl7(&mut peer, mixed.as_bytes()).await?;
    assert!(ack.contains("MSA|AR|mixed"), "{ack}");
    assert_eq!(
        count(&path, "outbox")?,
        2,
        "no subset of an errored LAB-29 may be delivered"
    );
    assert_eq!(
        count(&path, "observation_revisions")?,
        2,
        "rejected batches must not register withheld history"
    );
    assert!(hl7(&mut peer, new.as_bytes()).await?.contains("MSA|AA|new"));
    check(&reference.event().await?, "awos-2", "LAB", 0, "10.10")?;
    let valid = result("mixed-mapping", "\"\"", "awos-3^LAB", "10.10", "F");
    let unknown = result("unknown", "\"\"", "awos-4^LAB", "10.10", "F")
        .rewrite("SYN^Synthetic^99LAB", "UNKNOWN^Unknown^99LAB");
    let (_, unknown_body) = unknown.split_once('\r').ok_or("synthetic header absent")?;
    let mixed_mapping = format!("{valid}{}", unknown_body.replacen("SPM|1|", "SPM|2|", 1));
    // HL7 2.5.1 section 2.15.5.2: the second OBX, not the first, has the unknown code.
    let ack = hl7(&mut peer, mixed_mapping.as_bytes()).await?;
    assert!(ack.contains("MSA|AE|mixed-mapping"));
    assert!(ack.contains("ERR||OBX^2^3|207^^HL70357|E"), "{ack}");
    assert_eq!(count(&path, "observation_revisions")?, 3);
    assert_eq!(count(&path, "outbox")?, 3);
    assert!(
        hl7(&mut peer, valid.as_bytes())
            .await?
            .contains("MSA|AA|mixed-mapping")
    );
    check(&reference.event().await?, "awos-3", "LAB", 0, "10.10")?;
    assert!(count(&path, "parked")? > 0);
    wait_state(&path, "done", 4).await?;
    running.stop().await?;
    assert_eq!(receiver_count(&receiver_path)?, 4);
    reference.close().await?;
    Ok(())
}

#[cfg(feature = "test-hooks")]
#[tokio::test]
async fn law_tf2b_3_29_4_adr0003_raw_commit_precedes_interpretation_and_ack() -> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let path = vendor_config(temp.path(), "law", &definition())?;
    let mut reference = Reference::start(
        &temp.path().join("reference.db"),
        TcpListener::bind("127.0.0.1:0").await?,
        false,
    )
    .await?;
    let mut loaded = program(&path, &reference.url)?;
    let (hook, control) = analyte::runtime::test_hooks::Committed::new();
    loaded.pause_law_committed_for_test("law", hook)?;
    let running = Running::start_program(loaded, "synthetic-api").await?;
    let mut peer = connect(&path, "law").await?;
    let wire = analyte_framing::hl7::encode(
        result("durable", "\"\"", "awos-1^LAB", "12.30", "F").as_bytes(),
        analyte_framing::hl7::Limit::new(1048576)?,
    )?;
    peer.write_all(wire.get(..12).ok_or("synthetic prefix missing")?)
        .await?;
    assert!(
        // Deliberate quiet window: no reply may arrive.
        tokio::time::timeout(Duration::from_millis(40), peer.read_u8())
            .await
            .is_err()
    );
    assert_eq!(count(&path, "receipts")?, 0);
    peer.write_all(wire.get(12..).ok_or("synthetic suffix missing")?)
        .await?;
    control.reached().await?;
    let raw: Vec<u8> = database(&path)?.query_row("SELECT raw FROM receipts", [], |r| r.get(0))?;
    assert_eq!(raw, wire);
    assert_eq!(count(&path, "interpretations")?, 0);
    assert_eq!(count(&path, "outbox")?, 0);
    assert!(
        // Deliberate quiet window: no reply may arrive.
        tokio::time::timeout(Duration::from_millis(80), peer.read_u8())
            .await
            .is_err()
    );
    control.resume();
    let ack = read_reply(&mut peer).await?;
    assert!(ack.contains("MSA|AA|durable"));
    assert_eq!(count(&path, "interpretations")?, 1);
    check(&reference.event().await?, "awos-1", "LAB", 0, "12.30")?;
    running.stop().await?;
    reference.close().await?;
    Ok(())
}

#[tokio::test]
async fn law_tf2b_3_29_4_adr0003_storage_failures_never_send_aa() -> Result<(), Error> {
    for table in ["receipts", "interpretations"] {
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
        database(&path)?.execute_batch(&format!("CREATE TRIGGER synthetic_storage_failure BEFORE INSERT ON {table} BEGIN SELECT RAISE(FAIL,'synthetic-storage-failure'); END;"))?;
        let wire = analyte_framing::hl7::encode(
            result("storage-failure", "\"\"", "awos-1^LAB", "12.30", "F").as_bytes(),
            analyte_framing::hl7::Limit::new(1048576)?,
        )?;
        peer.write_all(&wire).await?;
        let mut buffer = [0; 1024];
        let read = tokio::time::timeout(HANG_GUARD, peer.read(&mut buffer)).await??;
        assert_eq!(read, 0, "storage failure must close without AA");
        assert_eq!(
            count(&path, "receipts")?,
            i64::from(table == "interpretations")
        );
        assert_eq!(count(&path, "interpretations")?, 0);
        assert_eq!(count(&path, "outbox")?, 0);
        running.stop().await?;
        assert_eq!(receiver_count(&receiver_path)?, 0);
        reference.close().await?;
    }
    Ok(())
}

#[tokio::test]
async fn law_tf2b_3_29_4_adr0003_nearly_full_disk_closes_without_aa() -> Result<(), Error> {
    struct NoSpace;
    impl analyte_store::FreeSpace for NoSpace {
        fn available(&self, _: &Path) -> std::io::Result<u64> {
            Ok(0)
        }
    }
    let temp = tempfile::tempdir()?;
    let path = vendor_config(temp.path(), "law", &definition())?;
    let receiver_path = temp.path().join("reference.db");
    let reference = Reference::start(
        &receiver_path,
        TcpListener::bind("127.0.0.1:0").await?,
        false,
    )
    .await?;
    let loaded = program(&path, &reference.url)?;
    let credentials = secrets(&loaded, "synthetic-api")?;
    let (stop, stopped) = oneshot::channel();
    let task = tokio::spawn(service::serve_loopback_test(
        loaded,
        Box::new(NoSpace),
        async {
            stopped
                .await
                .map_err(analyte::runtime::Error::ScmStopSenderClosed)?;
            Ok(())
        },
        credentials,
        None,
    ));
    let mut peer = connect(&path, "law").await?;
    let wire = analyte_framing::hl7::encode(
        result("full", "\"\"", "awos-1^LAB", "12.30", "F").as_bytes(),
        analyte_framing::hl7::Limit::new(1048576)?,
    )?;
    peer.write_all(&wire).await?;
    assert_eq!(
        tokio::time::timeout(HANG_GUARD, peer.read(&mut [0; 1])).await??,
        0
    );
    assert_eq!(count(&path, "receipts")?, 0);
    assert_eq!(count(&path, "outbox")?, 0);
    stop.send(()).map_err(|()| "service stopped early")?;
    tokio::time::timeout(HANG_GUARD, task).await???;
    assert_eq!(receiver_count(&receiver_path)?, 0);
    reference.close().await?;
    Ok(())
}

#[tokio::test]
async fn law_tf2b_3_29_4_mllp_partial_block_deadline_parks_original_without_aa() -> Result<(), Error>
{
    let temp = tempfile::tempdir()?;
    let path = vendor_config(
        temp.path(),
        "law",
        &definition().rewrite("receive_timeout_ms = 30000", "receive_timeout_ms = 100"),
    )?;
    let reference = Reference::start(
        &temp.path().join("reference.db"),
        TcpListener::bind("127.0.0.1:0").await?,
        false,
    )
    .await?;
    let running = Running::start(&path, &reference.url, "synthetic-api").await?;
    let mut peer = connect(&path, "law").await?;
    let partial = b"\x0bMSH|^~\\&|A";
    peer.write_all(partial).await?;
    assert_eq!(
        tokio::time::timeout(HANG_GUARD, peer.read(&mut [0; 1])).await??,
        0
    );
    let raw: Vec<u8> = database(&path)?.query_row("SELECT raw FROM receipts", [], |r| r.get(0))?;
    assert_eq!(raw, partial);
    assert_eq!(count(&path, "parked")?, 1);
    assert_eq!(count(&path, "outbox")?, 0);
    running.stop().await?;
    reference.close().await?;
    Ok(())
}

#[tokio::test]
async fn law_tf2b_3_27_3_28_disabled_query_and_work_cannot_issue_work_or_acceptance()
-> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let path = vendor_config(temp.path(), "law", &definition())?;
    let reference = Reference::start(
        &temp.path().join("reference.db"),
        TcpListener::bind("127.0.0.1:0").await?,
        false,
    )
    .await?;
    let running = Running::start(&path, &reference.url, "synthetic-api").await?;
    let mut peer = connect(&path, "law").await?;
    let query = "MSH|^~\\&|A|AF|M|MF|20260101120000+0000||QBP^Q11^QBP_Q11|query1|P|2.5.1|||NE|AL||UNICODE UTF-8|||LAB-27^IHE\rQPD|WOS^Work Order Step^IHELAW|tag1|00012a^LAB\rRCP|I||R^Real Time^HL70394\r";
    let ack = hl7(&mut peer, query.as_bytes()).await?;
    assert!(
        ack.contains("RSP^K11^RSP_K11")
            && ack.contains("MSA|AE|query1")
            && ack.contains("QAK|tag1|AE"),
        "{ack}"
    );
    assert!(!ack.contains("OML"));
    let work = result("work1", "\"\"", "awos-1^LAB", "12.30", "F")
        .rewrite("OUL^R22^OUL_R22", "OML^O33^OML_O33")
        .rewrite("LAB-29", "LAB-28");
    // B.7.2 pp.80-81: unsupported trigger is rejected with correlated AR.
    let rejected = hl7(&mut peer, work.as_bytes()).await?;
    assert!(rejected.contains("MSA|AR|work1"), "{rejected}");
    assert!(!rejected.contains("MSA|AA|"));
    assert_eq!(count(&path, "machine_orders")?, 0);
    assert_eq!(count(&path, "receipts")?, 2);
    assert_eq!(count(&path, "outbox")?, 0);
    assert_eq!(count(&path, "parked")?, 2);
    running.stop().await?;
    reference.close().await?;
    Ok(())
}

#[tokio::test]
async fn law_tf2b_3_29_4_adr0003_parked_result_reprocesses_original_after_mapping_fix()
-> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let missing = definition().rewrite("machine_code=\"SYN\"", "machine_code=\"MISSING\"");
    let path = vendor_config(temp.path(), "law", &missing)?;
    let receiver_path = temp.path().join("receiver.db");
    let mut reference = Reference::start(
        &receiver_path,
        TcpListener::bind("127.0.0.1:0").await?,
        false,
    )
    .await?;
    let running = Running::start(&path, &reference.url, "synthetic-api").await?;
    let mut peer = connect(&path, "law").await?;
    // LAW permits 50-character control IDs; the generic HL7 entry point does not.
    // Reprocessing must retain LAW validation/mapping rather than fall back to HL7.
    let control = "R".repeat(50);
    let payload = result(&control, "\"\"", "awos-1^LAB", "12.30", "F");
    assert!(
        hl7(&mut peer, payload.as_bytes())
            .await?
            .contains(&format!("MSA|AE|{control}"))
    );
    assert_eq!(count(&path, "outbox")?, 0);
    let id: String = database(&path)?.query_row("SELECT id FROM receipts", [], |r| r.get(0))?;
    let id: analyte_model::ReceiptId = serde_json::from_str(&id)?;
    running.stop().await?;
    drop(peer);
    std::fs::write(temp.path().join("hl7.toml"), definition())?;
    let current = program(&path, &reference.url)?;
    let mut store = analyte::open_system_store(&current).await?;
    assert_repaired_mapping_preserves_receipt(&mut store, &current, id).await?;
    let running = Running::start(&path, &reference.url, "synthetic-api").await?;
    let mut peer = connect(&path, "law").await?;
    check(&reference.event().await?, "awos-1", "LAB", 0, "12.30")?;
    wait_state(&path, "done", 1).await?;
    assert_eq!(count(&path, "receipts")?, 1);
    assert_eq!(count(&path, "interpretations")?, 3);
    assert_eq!(count(&path, "machine_orders")?, 0);
    assert_eq!(receiver_count(&receiver_path)?, 1);
    assert!(
        // Deliberate quiet window: no reply may arrive.
        tokio::time::timeout(Duration::from_millis(100), peer.read_u8())
            .await
            .is_err(),
        "operator reprocessing cannot issue machine traffic"
    );
    // Reprocessing a retained non-C conflict must roll back its attempted audit
    // with the batch, then commit one failed version/audit and no revisions.
    let conflicting = result("reprocess-conflict", "\"\"", "awos-1^LAB", "99.90", "F");
    assert!(
        hl7(&mut peer, conflicting.as_bytes())
            .await?
            .contains("MSA|AR|reprocess-conflict")
    );
    let conflict_id: String = database(&path)?.query_row(
        "SELECT id FROM receipts ORDER BY rowid DESC LIMIT 1",
        [],
        |r| r.get(0),
    )?;
    let conflict_id: analyte_model::ReceiptId = serde_json::from_str(&conflict_id)?;
    running.stop().await?;
    drop(peer);
    let mut store = analyte::open_system_store(&current).await?;
    let rejected = analyte::operator::recovery::reprocess(
        &store,
        &current,
        analyte::operator::recovery::ReprocessTarget::Receipt(conflict_id),
        analyte_model::Identifier::new("synthetic-operator")?,
        analyte_model::Reason::new("review retained non-C conflict")?,
    )
    .await?;
    assert_eq!(rejected.version, 2);
    assert!(rejected.outcomes.is_empty());
    store
        .request::<_, FixtureError>(reprocess_deadline()?, move |s| {
            assert!(matches!(
                s.interpretation(conflict_id, 2)?
                    .ok_or(FixtureError::Corruption)?
                    .outcome,
                analyte_store::Reading::Failed(_)
            ));
            assert_eq!(
                s.audits(0)?.len(),
                3,
                "exactly one additional audit despite conflict rollback"
            );
            assert_conflict_audit(s, conflict_id)?;
            assert_eq!(s.deliveries(0)?.len(), 1);
            Ok(())
        })
        .await?;
    assert_eq!(count(&path, "observation_revisions")?, 1);
    store.shutdown(reprocess_deadline()?).await?;
    reference.close().await?;
    Ok(())
}

// PaLM TF-2b Rev 11 B.7.1-B.7.2 pp.80-81; C.1 pp.84-85.
