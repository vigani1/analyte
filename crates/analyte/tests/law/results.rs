//! Synthetic LAW program/reference-receiver matrix, TF-2b 3.29.4, B.3 and B.7.
use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn definition() -> String {
    include_str!("../../../config/examples/law.toml").to_owned()
}

pub(super) fn result(id: &str, awos: &str, filler: &str, value: &str, status: &str) -> String {
    let mut fields = vec![""; 29];
    for (index, value) in [
        (0, "1"),
        (1, "NM"),
        (2, "SYN^Synthetic^99LAB"),
        (3, "1"),
        (4, value),
        (5, "mg/L^mg/L^UCUM"),
        (6, "0-20"),
        (7, "N^Normal^HL70078"),
        (10, status),
        (15, "operator^Synthetic"),
        (17, "instrument^99LAB~module^99LAB"),
        (18, "20260101120000"),
        (28, "RSLT"),
    ] {
        if let Some(field) = fields.get_mut(index) {
            *field = value;
        }
    }
    format!(
        "MSH|^~\\&|A|AF|M|MF|20260101120000+0000||OUL^R22^OUL_R22|{id}|P|2.5.1|||NE|AL||UNICODE UTF-8|||LAB-29^IHE\rSPM|1|sample-01&99LAB||SER^Serum^HL70487|||||||P^^HL70369\rSAC|||00012a^LAB\rOBR||{awos}|{filler}|SYN^Synthetic^99LAB\rORC|SC||||CM\rOBX|{}\r",
        fields.join("|")
    )
}

fn count(path: &Path, table: &str) -> Result<i64, Error> {
    // Callers supply only literal test-owned table names.
    Ok(database(path)?.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))?)
}

fn check(
    event: &Captured,
    order: &str,
    namespace: &str,
    revision: u64,
    value: &str,
) -> Result<(), Error> {
    assert!(event.outcome.contains("accepted"));
    let body: serde_json::Value = serde_json::from_slice(&event.bytes)?;
    assert_eq!(
        body.pointer("/results/0/sample/id")
            .and_then(|v| v.as_str()),
        Some("00012a")
    );
    assert_eq!(
        body.pointer("/results/0/identity/resolved/observation/order/value")
            .and_then(|v| v.as_str()),
        Some(order)
    );
    assert_eq!(
        body.pointer("/results/0/identity/resolved/observation/order/namespace")
            .and_then(|v| v.as_str()),
        Some(namespace)
    );
    assert_eq!(
        body.pointer("/results/0/identity/resolved/revision")
            .and_then(serde_json::Value::as_u64),
        Some(revision)
    );
    assert_eq!(
        body.pointer("/results/0/parts/0/values/0/original")
            .and_then(|v| v.as_str()),
        Some(value)
    );
    assert_eq!(
        body.pointer("/results/0/parts/0/units/original")
            .and_then(|v| v.as_str()),
        Some("mg/L^mg/L^UCUM")
    );
    Ok(())
}

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
    let running = Running::start(&path, &reference.url, "synthetic-api")?;
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
            first.replace("|first|", "|retransmit|").as_bytes()
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
    let running = Running::start(&path, &reference.url, "synthetic-api")?;
    let mut peer = connect(&path, "law").await?;
    let message = result("unsolicited", "\"\"", "run-1^ANALYZER", "12.30", "F");
    assert!(
        hl7(&mut peer, message.as_bytes())
            .await?
            .contains("MSA|AA|unsolicited")
    );
    check(&reference.event().await?, "run-1", "ANALYZER", 0, "12.30")?;
    let unresolved = result("unresolved", "\"\"", "", "12.30", "F");
    assert!(
        hl7(&mut peer, unresolved.as_bytes())
            .await?
            .contains("MSA|AE|unresolved")
    );
    assert!(count(&path, "parked")? > 0);
    assert_eq!(count(&path, "outbox")?, 1);
    wait_state(&path, "done", 1).await?;
    running.stop().await?;
    assert_eq!(receiver_count(&receiver_path)?, 1);
    reference.close().await?;
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
    let running = Running::start(&path, &reference.url, "synthetic-api")?;
    let mut peer = connect(&path, "law").await?;
    for (message, code) in [
        (
            result("unknown-test", "\"\"", "awos-1^LAB", "12.30", "F")
                .replace("SYN^Synthetic^99LAB", "UNKNOWN^Unknown^99LAB"),
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
            analyte_framing::mllp::encode(
                message.as_bytes(),
                analyte_framing::mllp::Limit::new(1048576)?
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
    let running = Running::start(&path, &reference.url, "synthetic-api")?;
    let mut peer = connect(&path, "law").await?;
    let first = result(
        "control-id-longer-than-generic-hl7-twenty",
        "\"\"",
        "awos-1^LAB",
        "12.30",
        "F",
    )
    .replace("^99LAB|1|12.30", "^99LAB|1^1^1|12.30");
    assert!(
        hl7(&mut peer, first.as_bytes())
            .await?
            .contains("MSA|AA|control-id-longer-than-generic-hl7-twenty")
    );
    check(&reference.event().await?, "awos-1", "LAB", 0, "12.30")?;
    // C.7 pp.102-103: OG.1 stays 1; changing OG.3 distinguishes another
    // observation within the same run, not a second final run.
    let rerun = first
        .replace("control-id-longer-than-generic-hl7-twenty", "rerun")
        .replace("|1^1^1|", "|1^1^2|");
    assert!(
        hl7(&mut peer, rerun.as_bytes())
            .await?
            .contains("MSA|AA|rerun")
    );
    check(&reference.event().await?, "awos-1", "LAB", 0, "12.30")?;
    let conflict = first
        .replace("control-id-longer-than-generic-hl7-twenty", "mixed")
        .replace("12.30", "99.9");
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
        .replace("SYN^Synthetic^99LAB", "UNKNOWN^Unknown^99LAB");
    let (_, unknown_body) = unknown.split_once('\r').ok_or("synthetic header absent")?;
    let mixed_mapping = format!("{valid}{}", unknown_body.replacen("SPM|1|", "SPM|2|", 1));
    assert!(
        hl7(&mut peer, mixed_mapping.as_bytes())
            .await?
            .contains("MSA|AE|mixed-mapping")
    );
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
    let (hook, control) = analyte::test_hooks::Committed::new();
    loaded.pause_law_committed_for_test("law", hook)?;
    let running = Running::start_program(loaded, "synthetic-api")?;
    let mut peer = connect(&path, "law").await?;
    let wire = analyte_framing::mllp::encode(
        result("durable", "\"\"", "awos-1^LAB", "12.30", "F").as_bytes(),
        analyte_framing::mllp::Limit::new(1048576)?,
    )?;
    peer.write_all(wire.get(..12).ok_or("synthetic prefix missing")?)
        .await?;
    assert!(
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

async fn read_reply(peer: &mut tokio::net::TcpStream) -> Result<String, Error> {
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut bytes = Vec::new();
        while !bytes.ends_with(b"\x1c\r") {
            if bytes.len() >= 65536 {
                return Err("reply bound".into());
            }
            bytes.push(peer.read_u8().await?);
        }
        Ok::<_, Error>(String::from_utf8(bytes)?)
    })
    .await?
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
        let running = Running::start(&path, &reference.url, "synthetic-api")?;
        let mut peer = connect(&path, "law").await?;
        database(&path)?.execute_batch(&format!("CREATE TRIGGER synthetic_storage_failure BEFORE INSERT ON {table} BEGIN SELECT RAISE(FAIL,'synthetic-storage-failure'); END;"))?;
        let wire = analyte_framing::mllp::encode(
            result("storage-failure", "\"\"", "awos-1^LAB", "12.30", "F").as_bytes(),
            analyte_framing::mllp::Limit::new(1048576)?,
        )?;
        peer.write_all(&wire).await?;
        let mut buffer = [0; 1024];
        let read = tokio::time::timeout(Duration::from_secs(5), peer.read(&mut buffer)).await??;
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
            stopped.await?;
            Ok(())
        },
        credentials,
    ));
    let mut peer = connect(&path, "law").await?;
    let wire = analyte_framing::mllp::encode(
        result("full", "\"\"", "awos-1^LAB", "12.30", "F").as_bytes(),
        analyte_framing::mllp::Limit::new(1048576)?,
    )?;
    peer.write_all(&wire).await?;
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), peer.read(&mut [0; 1])).await??,
        0
    );
    assert_eq!(count(&path, "receipts")?, 0);
    assert_eq!(count(&path, "outbox")?, 0);
    stop.send(()).map_err(|()| "service stopped early")?;
    tokio::time::timeout(Duration::from_secs(7), task).await???;
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
        &definition().replace("receive_timeout_ms = 30000", "receive_timeout_ms = 100"),
    )?;
    let reference = Reference::start(
        &temp.path().join("reference.db"),
        TcpListener::bind("127.0.0.1:0").await?,
        false,
    )
    .await?;
    let running = Running::start(&path, &reference.url, "synthetic-api")?;
    let mut peer = connect(&path, "law").await?;
    let partial = b"\x0bMSH|^~\\&|A";
    peer.write_all(partial).await?;
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), peer.read(&mut [0; 1])).await??,
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
    let running = Running::start(&path, &reference.url, "synthetic-api")?;
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
        .replace("OUL^R22^OUL_R22", "OML^O33^OML_O33")
        .replace("LAB-29", "LAB-28");
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
    let missing = definition().replace("machine_code=\"SYN\"", "machine_code=\"MISSING\"");
    let path = vendor_config(temp.path(), "law", &missing)?;
    let receiver_path = temp.path().join("receiver.db");
    let mut reference = Reference::start(
        &receiver_path,
        TcpListener::bind("127.0.0.1:0").await?,
        false,
    )
    .await?;
    let running = Running::start(&path, &reference.url, "synthetic-api")?;
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
    let until = || {
        std::time::Instant::now()
            .checked_add(Duration::from_secs(5))
            .ok_or("deadline")
    };
    let (parked, original, version1) = store
        .request(until()?, move |s| {
            let parked = s
                .parked(0)?
                .first()
                .ok_or(analyte_store::Error::Corruption)?
                .id;
            let original = s.receipt(id)?.ok_or(analyte_store::Error::UnknownReceipt)?;
            let version1 = serde_json::to_string(
                &s.interpretation(id, 1)?
                    .ok_or(analyte_store::Error::Corruption)?,
            )?;
            Ok((parked, original, version1))
        })
        .await?;
    let commit = analyte::recovery::reprocess(
        &store,
        &current,
        analyte::recovery::ReprocessTarget::Parked(parked),
        analyte_model::Identifier::new("synthetic-operator")?,
        analyte_model::Reason::new("repair LAW test mapping")?,
    )
    .await?;
    assert_eq!(commit.version, 2);
    assert!(matches!(
        commit.outcomes.as_slice(),
        [analyte_store::RevisionOutcome::New(_)]
    ));
    let repeated = analyte::recovery::reprocess(
        &store,
        &current,
        analyte::recovery::ReprocessTarget::Receipt(id),
        analyte_model::Identifier::new("synthetic-operator")?,
        analyte_model::Reason::new("verify retained result idempotency")?,
    )
    .await?;
    assert_eq!(repeated.version, 3);
    assert!(matches!(
        repeated.outcomes.as_slice(),
        [analyte_store::RevisionOutcome::Retransmission(_)]
    ));
    store
        .request(until()?, move |s| {
            let retained = s.receipt(id)?.ok_or(analyte_store::Error::UnknownReceipt)?;
            assert_eq!(retained.raw, original.raw);
            assert_eq!(retained.config, original.config);
            assert_eq!(
                serde_json::to_string(&retained.metadata)?,
                serde_json::to_string(&original.metadata)?
            );
            assert_eq!(
                serde_json::to_string(
                    &s.interpretation(id, 1)?
                        .ok_or(analyte_store::Error::Corruption)?
                )?,
                version1
            );
            assert!(
                s.parked_item(parked)?
                    .ok_or(analyte_store::Error::Corruption)?
                    .resolution
                    .is_some()
            );
            assert_eq!(s.deliveries(0)?.len(), 1);
            assert_eq!(s.audits(0)?.len(), 2);
            Ok(())
        })
        .await?;
    store.shutdown(until()?).await?;
    let running = Running::start(&path, &reference.url, "synthetic-api")?;
    let mut peer = connect(&path, "law").await?;
    check(&reference.event().await?, "awos-1", "LAB", 0, "12.30")?;
    wait_state(&path, "done", 1).await?;
    assert_eq!(count(&path, "receipts")?, 1);
    assert_eq!(count(&path, "interpretations")?, 3);
    assert_eq!(count(&path, "machine_orders")?, 0);
    assert_eq!(receiver_count(&receiver_path)?, 1);
    assert!(
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
    let rejected = analyte::recovery::reprocess(
        &store,
        &current,
        analyte::recovery::ReprocessTarget::Receipt(conflict_id),
        analyte_model::Identifier::new("synthetic-operator")?,
        analyte_model::Reason::new("review retained non-C conflict")?,
    )
    .await?;
    assert_eq!(rejected.version, 2);
    assert!(rejected.outcomes.is_empty());
    store
        .request(until()?, move |s| {
            assert!(matches!(
                s.interpretation(conflict_id, 2)?
                    .ok_or(analyte_store::Error::Corruption)?
                    .outcome,
                analyte_store::Reading::Failed(_)
            ));
            assert_eq!(
                s.audits(0)?.len(),
                3,
                "exactly one additional audit despite conflict rollback"
            );
            assert_eq!(s.deliveries(0)?.len(), 1);
            Ok(())
        })
        .await?;
    assert_eq!(count(&path, "observation_revisions")?, 1);
    store.shutdown(until()?).await?;
    reference.close().await?;
    Ok(())
}

// PaLM TF-2b Rev 11 B.7.1-B.7.2 pp.80-81; C.1 pp.84-85.
#[tokio::test]
async fn law_tf2b_c1_b7_second_obx_error_reports_actual_occurrence() -> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let path = vendor_config(temp.path(), "law", &definition())?;
    let reference = Reference::start(
        &temp.path().join("receiver.db"),
        TcpListener::bind("127.0.0.1:0").await?,
        false,
    )
    .await?;
    let running = Running::start(&path, &reference.url, "synthetic-api")?;
    let mut peer = connect(&path, "law").await?;
    let first = result("second-obx", "\"\"", "run^ANALYZER", "1.0", "F");
    let obx = first.split("OBX|").nth(1).ok_or("synthetic OBX missing")?;
    let second = format!("OBX|{}", obx.replacen("1|NM|", "2|NM|", 1));
    for (payload, location) in [
        (
            format!("{first}{}", second.replace("|F|", "|D|")),
            "OBX^2^11",
        ),
        (
            format!("{first}{}", second.replacen("OBX|2|", "OBX|3|", 1)),
            "OBX^2^1",
        ),
        (
            format!("{}{second}", first.replace("|F|", "|D|")),
            "OBX^1^11",
        ),
    ] {
        let ack = hl7(&mut peer, payload.as_bytes()).await?;
        assert!(ack.contains("MSA|AE|second-obx"), "{ack}");
        assert!(ack.contains(&format!("ERR||{location}|")), "{ack}");
        assert_eq!(count(&path, "outbox")?, 0);
        assert_eq!(count(&path, "observation_revisions")?, 0);
        let raw: Vec<u8> = database(&path)?.query_row(
            "SELECT raw FROM receipts ORDER BY rowid DESC LIMIT 1",
            [],
            |r| r.get(0),
        )?;
        assert_eq!(
            raw,
            analyte_framing::mllp::encode(
                payload.as_bytes(),
                analyte_framing::mllp::Limit::new(1048576)?
            )?
        );
    }
    running.stop().await?;
    reference.close().await?;
    Ok(())
}

// ADR 0011; PaLM TF-2b Rev 11 C.7 Figure C.7-1 p.111, Table C.7-18 p.110.
#[tokio::test]
async fn law_adr0011_c7_all_status_moves_and_unverified_delivery() -> Result<(), Error> {
    for before in ["P", "R", "F", "C", "X"] {
        for after in ["P", "R", "F", "C", "X"] {
            let temp = tempfile::tempdir()?;
            let path = vendor_config(temp.path(), "law", &definition())?;
            let reference = Reference::start(
                &temp.path().join("receiver.db"),
                TcpListener::bind("127.0.0.1:0").await?,
                false,
            )
            .await?;
            let mut running = Running::start(&path, &reference.url, "synthetic-api")?;
            let mut peer = connect(&path, "law").await?;
            let first = result("before", "\"\"", "lifecycle^ANALYZER", "1.0", before);
            assert!(
                hl7(&mut peer, first.as_bytes())
                    .await?
                    .contains("MSA|AA|before")
            );
            if before == "R" && after == "F" {
                running.stop().await?;
                running = Running::start(&path, &reference.url, "synthetic-api")?;
                peer = connect(&path, "law").await?;
            }
            let permitted = matches!(
                (before, after),
                ("P", "P" | "R" | "F") | ("R", "R" | "F") | ("F", "C") | ("C", "C")
            );
            let second = result("after", "\"\"", "lifecycle^ANALYZER", "2.0", after);
            let ack = hl7(&mut peer, second.as_bytes()).await?;
            assert!(
                ack.contains(&format!(
                    "MSA|{}|after",
                    if permitted { "AA" } else { "AR" }
                )),
                "{before}->{after}: {ack}"
            );
            assert_eq!(
                count(&path, "observation_revisions")?,
                if permitted { 2 } else { 1 },
                "{before}->{after}"
            );
            let deliveries = i64::from(before != "R") + i64::from(permitted && after != "R");
            assert_eq!(count(&path, "outbox")?, deliveries, "{before}->{after}");
            if permitted && before == "R" && after == "F" {
                let row =
                    database(&path)?.query_row("SELECT payload FROM outbox LIMIT 1", [], |r| {
                        r.get::<_, Vec<u8>>(0)
                    })?;
                let body: serde_json::Value = serde_json::from_slice(&row)?;
                assert_eq!(
                    body.pointer("/results/0/identity/resolved/revision")
                        .and_then(serde_json::Value::as_u64),
                    Some(1)
                );
                assert_eq!(
                    body.pointer("/results/0/parts/0/source_status")
                        .and_then(|v| v.as_str()),
                    Some("F")
                );
            }
            running.stop().await?;
            reference.close().await?;
        }
    }
    Ok(())
}

// C.7 Figure C.7-1 p.111 and OG.1 pp.102-103; ADR 0011.
#[tokio::test]
async fn law_adr0011_c7_only_one_final_run_in_history_or_batch() -> Result<(), Error> {
    for batch in [false, true] {
        let temp = tempfile::tempdir()?;
        let path = vendor_config(temp.path(), "law", &definition())?;
        let mut reference = Reference::start(
            &temp.path().join("receiver.db"),
            TcpListener::bind("127.0.0.1:0").await?,
            false,
        )
        .await?;
        let running = Running::start(&path, &reference.url, "synthetic-api")?;
        let mut peer = connect(&path, "law").await?;
        let first = result("run-one", "\"\"", "order^ANALYZER", "1.0", "F")
            .replace("^99LAB|1|1.0", "^99LAB|1^1^1|1.0");
        let second = result("run-two", "\"\"", "order^ANALYZER", "2.0", "F")
            .replace("^99LAB|1|2.0", "^99LAB|2^1^1|2.0");
        if batch {
            let (_, body) = second.split_once('\r').ok_or("header")?;
            let mixed = format!("{first}{}", body.replacen("SPM|1|", "SPM|2|", 1));
            assert!(
                hl7(&mut peer, mixed.as_bytes())
                    .await?
                    .contains("MSA|AR|run-one")
            );
            assert_eq!(count(&path, "outbox")?, 0);
            assert_eq!(count(&path, "observation_revisions")?, 0);
        } else {
            assert!(
                hl7(&mut peer, first.as_bytes())
                    .await?
                    .contains("MSA|AA|run-one")
            );
            reference.event().await?;
            running.stop().await?;
            let running = Running::start(&path, &reference.url, "synthetic-api")?;
            let mut peer = connect(&path, "law").await?;
            assert!(
                hl7(&mut peer, second.as_bytes())
                    .await?
                    .contains("MSA|AR|run-two")
            );
            assert!(
                hl7(&mut peer, second.replace("|F|", "|C|").as_bytes())
                    .await?
                    .contains("MSA|AR|run-two")
            );
            assert_eq!(count(&path, "outbox")?, 1);
            assert_eq!(count(&path, "observation_revisions")?, 1);
            running.stop().await?;
            reference.close().await?;
            continue;
        }
        running.stop().await?;
        reference.close().await?;
    }
    Ok(())
}
