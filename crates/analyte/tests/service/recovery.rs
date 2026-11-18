//! Tests store recovery after forced process exit.
use super::*;
use crate::support::HANG_GUARD;
use analyte_fixtures::ConfigEdit as _;
use std::io::Read as _;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const ROOT: &str = "ANALYTE_FORCED_EXIT_TEST_ROOT";
const URL: &str = "ANALYTE_FORCED_EXIT_TEST_URL";

struct Child(std::process::Child);
impl Drop for Child {
    fn drop(&mut self) {
        // Always reap the test child, including assertion failures and receiver timeouts.
        if !matches!(self.0.try_wait(), Ok(Some(_))) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

#[tokio::test]
async fn forced_exit_child() -> Result<(), Error> {
    let Some(root) = std::env::var_os(ROOT) else {
        return Ok(());
    };
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .try_init()?;
    let root = std::path::PathBuf::from(root);
    let url = std::env::var(URL)?;
    let path = config(&root, true)?;
    let definition = root.join("astm.toml");
    let orders = include_str!("../../../core/config/examples/orders.toml")
        // This test exercises crash recovery, not query response expiry.
        .rewrite("deadline_ms = 1000", "deadline_ms = 30000")
        .rewrite("send_ahead = true", "send_ahead = false")
        .rewrite(
            "namespace=\"demo\",value=\"DEMO\"",
            "namespace=\"synthetic\",value=\"TEST\"",
        );
    std::fs::write(
        &definition,
        format!("{}\n{orders}", std::fs::read_to_string(&definition)?),
    )?;
    let mut loaded = program(&path, &url)?;
    loaded.stall_store_on_shutdown_for_test()?;
    let running = Running::start_program(loaded, "synthetic-api").await?;
    // A row can precede the trusted feed commit; only that commit permits an order answer.
    wait_feed(&path).await?;
    wait_database_count(&path, "SELECT count(*) FROM orders", [], 1).await?;
    let mut astm_peer = connect(&path, "astm").await?;
    // LIS02 §11: reserve the query answer; withhold ENQ acknowledgment so it stays Sending.
    astm(
        &mut astm_peer,
        b"H|\\^&|||SYN|||||||P|1\rQ|1|^000Ab12||ALL||||||||O\rL|1|N\r",
    )
    .await?;
    assert_eq!(
        tokio::time::timeout(HANG_GUARD, astm_peer.read_u8()).await??,
        5
    );
    let mut hl7_peer = connect(&path, "hl7").await?;
    assert!(hl7(&mut hl7_peer, ORU).await?.contains("MSA|AA"));
    wait_state(&path, "in_flight", 1).await?;
    assert_eq!(
        database(&path)?.query_row("SELECT state FROM machine_orders", [], |r| r
            .get::<_, String>(0))?,
        "Sending"
    );
    // The parent confirms that the receiver committed while its HTTP response is withheld.
    // Track the shutdown interval separately from child setup and order polling.
    std::fs::write(root.join("shutdown-ready"), b"ready")?;
    tokio::time::timeout(HANG_GUARD, async {
        while !root.join("stop-request").exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;
    running.stop().await?;
    Err("stuck store unexpectedly completed graceful shutdown".into())
}

fn receipts(path: &Path) -> Result<Vec<(String, Vec<u8>)>, Error> {
    let db = database(path)?;
    Ok(db
        .prepare("SELECT id,raw FROM receipts ORDER BY id")?
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<_, _>>()?)
}

// Plan §11; ADR 0003; AGENTS.md
#[tokio::test]
async fn forced_exit_preserves_receipts_and_orders() -> Result<(), Error> {
    let directory = tempfile::tempdir()?;
    let receiver_path = directory.path().join("reference.db");
    let mut reference = Reference::start(
        &receiver_path,
        TcpListener::bind("127.0.0.1:0").await?,
        true,
    )
    .await?;
    reference
        .receiver
        .seed(vec![analyte_contract::decode(include_bytes!(
            "../../../../docs/contract/vectors/order.json"
        ))?])
        .await?;
    let mut child = Child(
        std::process::Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "service_recovery::forced_exit_child",
                "--nocapture",
            ])
            .env(ROOT, directory.path())
            .env(URL, &reference.url)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()?,
    );
    let original = tokio::select! {
        result = reference.event() => result?,
        result = async {
            loop {
                if let Some(status) = child.0.try_wait()? {
                    let mut log = String::new();
                    child.0.stderr.take().ok_or("missing child log")?.read_to_string(&mut log)?;
                    return Err::<(), Error>(format!("child exited before receiver commit ({status}): {log}").into());
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        } => { result?; return Err("child monitor unexpectedly completed".into()); }
    };
    assert!(original.outcome.contains("accepted"));
    assert_forced_shutdown(&mut child, directory.path()).await?;
    let path = directory.path().join("program.toml");
    let before = receipts(&path)?;
    let wire = analyte_framing::hl7::encode(ORU, analyte_framing::hl7::Limit::new(1_048_576)?)?;
    assert!(before.iter().any(|(_, raw)| raw == &wire));
    assert_eq!(
        database(&path)?.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))?,
        "ok"
    );
    assert_eq!(
        database(&path)?.query_row("SELECT state FROM machine_orders", [], |r| r
            .get::<_, String>(0))?,
        "Sending"
    );
    let mut loaded = Program::load(&path)?;
    loaded.set_loopback_destination_for_test("demo", reference.url.clone())?;
    let restarted = Running::start_program(loaded, "synthetic-api").await?;
    let duplicate = reference.event().await?;
    assert!(duplicate.outcome.contains("duplicate"));
    assert_eq!(duplicate.id, original.id);
    assert_eq!(duplicate.bytes, original.bytes);
    wait_state(&path, "done", 2).await?; // Result retry and one Uncertain order event.
    assert_eq!(receipts(&path)?, before);
    assert_recovered_order_and_delivery_counts(&path, &receiver_path)?;
    let mut peer = connect(&path, "astm").await?;
    // LIS02 §11 and safety rule 8: reconnecting never replays the reserved machine order.
    assert!(
        // Deliberate quiet window: no reply may arrive.
        tokio::time::timeout(Duration::from_millis(200), peer.read_u8())
            .await
            .is_err()
    );
    peer.shutdown().await?;
    restarted.stop().await?;
    reference.close().await?;
    Ok(())
}

async fn assert_forced_shutdown(child: &mut Child, root: &Path) -> Result<(), Error> {
    let started = std::time::Instant::now();
    while !root.join("shutdown-ready").exists() {
        if started.elapsed() > HANG_GUARD {
            return Err("child did not reach shutdown boundary".into());
        }
        if child.0.try_wait()?.is_some() {
            return Err("child exited before shutdown boundary".into());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    // Deliberate shutdown bound, also asserted below.
    let shutdown_started = std::time::Instant::now();
    std::fs::write(root.join("stop-request"), b"stop")?;
    let status = loop {
        if let Some(status) = child.0.try_wait()? {
            break status;
        }
        if shutdown_started.elapsed() > Duration::from_secs(3) {
            return Err("child exceeded shutdown bound plus margin".into());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    };
    assert_eq!(status.code(), Some(3));
    assert!(shutdown_started.elapsed() < Duration::from_secs(3));
    let mut log = String::new();
    child
        .0
        .stderr
        .take()
        .ok_or("missing child log")?
        .read_to_string(&mut log)?;
    assert_eq!(log.matches("shutdown bound exceeded").count(), 1);
    assert!(log.contains("store"));
    assert!(!log.contains("00012a"));
    Ok(())
}

fn assert_recovered_order_and_delivery_counts(
    path: &Path,
    receiver_path: &Path,
) -> Result<(), Error> {
    let db = database(path)?;
    let (state, reason): (String, String) =
        db.query_row("SELECT state,source_code FROM machine_orders", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?;
    assert_eq!(state, "Uncertain");
    assert_eq!(reason, "host-restart");
    assert_eq!(
        db.query_row("SELECT count(*) FROM observation_revisions", [], |r| r
            .get::<_, i64>(0))?,
        1
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM attempts WHERE outcome=?1",
            ["\"uncertain (interrupted)\""],
            |r| r.get::<_, i64>(0)
        )?,
        1
    );
    // One result at the receiver despite the HTTP retry; the other document is an order event.
    let receiver = rusqlite::Connection::open(receiver_path)?;
    assert_eq!(
        receiver.query_row(
            "SELECT count(*) FROM deliveries WHERE kind='results'",
            [],
            |r| r.get::<_, i64>(0)
        )?,
        1
    );
    assert_eq!(
        receiver.query_row(
            "SELECT count(*) FROM deliveries WHERE kind='order-events'",
            [],
            |r| r.get::<_, i64>(0)
        )?,
        1
    );
    Ok(())
}

const KILLED_ROOT: &str = "ANALYTE_KILLED_TEST_ROOT";
const KILLED_URL: &str = "ANALYTE_KILLED_TEST_URL";

#[tokio::test]
async fn killed_between_store_and_ack_child() -> Result<(), Error> {
    let Some(root) = std::env::var_os(KILLED_ROOT) else {
        return Ok(());
    };
    let root = std::path::PathBuf::from(root);
    let path = config(&root, true)?;
    let (hook, control) = analyte::runtime::test_hooks::Committed::new();
    let mut loaded = program(&path, &std::env::var(KILLED_URL)?)?;
    loaded.pause_astm_committed_for_test("astm", hook)?;
    let _running = Running::start_program(loaded, "synthetic-api").await?;
    // The HL7 machine's interpreted receipt gives the parent real retained metadata.
    let mut hl7_peer = connect(&path, "hl7").await?;
    assert!(hl7(&mut hl7_peer, ORU).await?.contains("MSA|AA"));
    let mut astm_peer = connect(&path, "astm").await?;
    astm_peer.write_all(b"\x05").await?;
    assert_eq!(astm_peer.read_u8().await?, 6);
    for frame in analyte_framing::astm::encode(
        b"H|\\^&||||||||||P|LIS2-A2|20261115120000\r",
        analyte_framing::astm::FrameNumber::FIRST,
        analyte_framing::astm::Limit::new(64000)?,
    )? {
        astm_peer.write_all(&frame).await?;
    }
    // The frame is journaled and its ACK withheld; the parent now kills this process.
    control.reached().await?;
    std::fs::write(root.join("killed-ready"), b"ready")?;
    tokio::time::sleep(HANG_GUARD).await;
    Err("child was not killed".into())
}

// ADR 0003 rules 1 and 2, LIS01-A2 §6.3 (frame ACK) and HL7 2.5.1 §2.9.3 (commit
// acknowledgment): a process killed after storing data but before acknowledging or
// interpreting it leaves an open ASTM transfer and an uninterpreted receipt. Startup
// must park the transfer for an operator and interpret the receipt before any machine
// connection opens.
// ADR 0003
#[tokio::test]
async fn killed_process_parks_transfer_and_interprets_receipt() -> Result<(), Error> {
    let directory = tempfile::tempdir()?;
    let mut reference = Reference::start(
        &directory.path().join("reference.db"),
        TcpListener::bind("127.0.0.1:0").await?,
        true,
    )
    .await?;
    let mut child = Child(
        std::process::Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "service_recovery::killed_between_store_and_ack_child",
                "--nocapture",
            ])
            .env(KILLED_ROOT, directory.path())
            .env(KILLED_URL, &reference.url)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()?,
    );
    let started = std::time::Instant::now();
    while !directory.path().join("killed-ready").exists() {
        if started.elapsed() > HANG_GUARD || child.0.try_wait()?.is_some() {
            return Err("child did not reach the stored, unacknowledged frame".into());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    child.0.kill()?;
    child.0.wait()?;
    let path = directory.path().join("program.toml");
    let db = database(&path)?;
    assert_eq!(
        db.query_row("SELECT state FROM transfers", [], |r| r.get::<_, String>(0))?,
        "open"
    );
    // A receipt stored before its interpretation, as an HL7 commit ACK (CA) leaves it.
    let (encoded, config): (String, Vec<u8>) = db.query_row(
        "SELECT metadata,config FROM receipts WHERE json_extract(metadata,'$.machine')='hl7' LIMIT 1",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let oru = String::from_utf8(ORU.to_vec())?.replace("00012a", "00099a");
    let raw =
        analyte_framing::hl7::encode(oru.as_bytes(), analyte_framing::hl7::Limit::new(1_048_576)?)?;
    let mut metadata: analyte_model::ReceiptMetadata = serde_json::from_str(&encoded)?;
    metadata.id = analyte_model::ReceiptId::new();
    metadata.raw_digest = analyte_model::Digest(analyte_contract::content_digest(&raw));
    let pending = serde_json::to_string(&metadata.id)?;
    db.execute(
        "INSERT INTO receipts(id,metadata,raw,config) VALUES (?1,?2,?3,?4)",
        rusqlite::params![pending, serde_json::to_string(&metadata)?, raw, config],
    )?;
    drop(db);
    let mut loaded = Program::load(&path)?;
    loaded.set_loopback_destination_for_test("demo", reference.url.clone())?;
    let restarted = Running::start_program(loaded, "synthetic-api").await?;
    let db = database(&path)?;
    assert_eq!(
        db.query_row("SELECT state FROM transfers", [], |r| r.get::<_, String>(0))?,
        "aborted"
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM frames", [], |r| r.get::<_, i64>(0))?,
        1
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM parked WHERE json_extract(evidence,'$.kind')='incomplete_transfer' AND resolution IS NULL",
            [],
            |r| r.get::<_, i64>(0)
        )?,
        1
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM interpretations WHERE receipt=?1",
            [&pending],
            |r| r.get::<_, i64>(0)
        )?,
        1
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM outbox WHERE receipt=?1",
            [&pending],
            |r| r.get::<_, i64>(0)
        )?,
        1
    );
    // The killed child's delivery and the recovered receipt both finish before shutdown.
    release_until_done(&path, &mut reference, 2).await?;
    restarted.stop().await?;
    reference.close().await?;
    Ok(())
}

/// Releases held app answers until `count` deliveries are done, so no attempt stays in flight.
async fn release_until_done(
    path: &Path,
    reference: &mut Reference,
    count: i64,
) -> Result<(), Error> {
    let release = async {
        loop {
            tokio::time::timeout(HANG_GUARD, reference.event()).await??;
        }
    };
    tokio::select! {
        result = wait_state(path, "done", count) => result,
        result = release => result,
    }
}

const PAUSE_MARKER: &str = "ANALYTE_TEST_ASTM_PAUSE_MARKER";

/// Starts the real `analyte run` with synthetic credentials, logging to `log`.
fn start_program_process(
    program: &Path,
    log: &Path,
    marker: Option<&Path>,
) -> Result<Child, Error> {
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_analyte"));
    command
        .arg("--config")
        .arg(program)
        .arg("run")
        .env("ANALYTE_SECRET_DEMO_API", "synthetic-api")
        .env("ANALYTE_SECRET_DEMO_SIGNING", SECRET)
        .env_remove(PAUSE_MARKER)
        .stdout(std::process::Stdio::null())
        .stderr(std::fs::File::create(log)?);
    if let Some(marker) = marker {
        command.env(PAUSE_MARKER, marker);
    }
    Ok(Child(command.spawn()?))
}

/// Connects as the machine once the program listens; listening starts after startup recovery.
async fn connect_process(
    child: &mut Child,
    endpoint: &str,
    log: &Path,
) -> Result<tokio::net::TcpStream, Error> {
    let started = std::time::Instant::now();
    loop {
        if let Ok(stream) = tokio::net::TcpStream::connect(endpoint).await {
            return Ok(stream);
        }
        if let Some(status) = child.0.try_wait()? {
            let log = std::fs::read_to_string(log)?;
            return Err(format!("program exited before listening ({status}): {log}").into());
        }
        if started.elapsed() > HANG_GUARD {
            return Err("program did not listen".into());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

fn frames(path: &Path) -> Result<Vec<Vec<u8>>, Error> {
    Ok(database(path)?
        .prepare("SELECT raw FROM frames ORDER BY sequence")?
        .query_map([], |row| row.get(0))?
        .collect::<Result<_, _>>()?)
}

// ADR 0003 rules 1 and 2; ADR 0010; LIS01-A2 §6.3 (frame ACK). A power cut gives no
// graceful stop and no store drain. The real program, hard-killed after journaling a
// frame and before its ACK, has sent no reply for it; on restart it parks the open
// transfer as incomplete for an operator and keeps the frame.
#[tokio::test]
async fn hard_kill_before_ack_parks_stored_frame() -> Result<(), Error> {
    let directory = tempfile::tempdir()?;
    let root = directory.path();
    let endpoint = std::net::TcpListener::bind("127.0.0.1:0")?
        .local_addr()?
        .to_string();
    let path = analyte_fixtures::write_machine(root, &analyte_fixtures::astm_tcp(), &endpoint)?;
    let marker = root.join("paused");
    let log = root.join("killed.log");
    let mut killed = start_program_process(&path, &log, Some(&marker))?;
    let mut peer = connect_process(&mut killed, &endpoint, &log).await?;
    peer.write_all(b"\x05").await?;
    assert_eq!(tokio::time::timeout(HANG_GUARD, peer.read_u8()).await??, 6);
    let sent = analyte_framing::astm::encode(
        b"H|\\^&||||||||||P|LIS2-A2|20261115120000\r",
        analyte_framing::astm::FrameNumber::FIRST,
        analyte_framing::astm::Limit::new(64000)?,
    )?
    .collect::<Vec<_>>();
    for frame in &sent {
        peer.write_all(frame).await?;
    }
    let started = std::time::Instant::now();
    while !marker.exists() {
        if started.elapsed() > HANG_GUARD || killed.0.try_wait()?.is_some() {
            return Err("program did not pause after storing the frame".into());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    // Read what already arrived before killing: a reset from the killed program can
    // discard unread bytes on Windows. The std socket is non-blocking here.
    let mut peer = peer.into_std()?;
    let mut reply = vec![0; 64];
    let arrived = match std::io::Read::read(&mut peer, &mut reply) {
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => 0,
        read => read?,
    };
    reply.truncate(arrived);
    killed.0.kill()?;
    killed.0.wait()?;
    peer.set_nonblocking(false)?;
    peer.set_read_timeout(Some(HANG_GUARD))?;
    let mut rest = Vec::new();
    // Ends at the close or the reset of the killed program's socket.
    let _ended = std::io::Read::read_to_end(&mut peer, &mut rest);
    reply.extend(rest);
    // The machine sees only the connection end: no ACK and no NAK for its frame.
    assert_eq!(reply, b"");
    drop(peer);
    // Still open: the kill, not the program, ended the connection.
    assert_eq!(
        database(&path)?.query_row("SELECT state FROM transfers", [], |r| r.get::<_, String>(0))?,
        "open"
    );
    assert_eq!(frames(&path)?, sent);

    let log = root.join("restarted.log");
    let mut restarted = start_program_process(&path, &log, None)?;
    let mut peer = connect_process(&mut restarted, &endpoint, &log).await?;
    let db = database(&path)?;
    assert_eq!(
        db.query_row("SELECT state FROM transfers", [], |r| r.get::<_, String>(0))?,
        "aborted"
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM parked WHERE json_extract(evidence,'$.kind')='incomplete_transfer' AND resolution IS NULL",
            [],
            |r| r.get::<_, i64>(0)
        )?,
        1
    );
    // Never treated as a complete message: nothing to interpret or deliver.
    for table in ["receipts", "outbox"] {
        assert_eq!(
            db.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                .get::<_, i64>(0))?,
            0
        );
    }
    drop(db);
    assert_eq!(frames(&path)?, sent);
    let listed = std::process::Command::new(env!("CARGO_BIN_EXE_analyte"))
        .arg("--config")
        .arg(&path)
        .args(["parked", "list"])
        .output()?;
    assert!(
        listed.status.success(),
        "{}",
        String::from_utf8_lossy(&listed.stderr)
    );
    assert_eq!(
        String::from_utf8(listed.stdout)?
            .matches("incomplete_transfer")
            .count(),
        1
    );
    // The restarted program serves the machine again.
    peer.write_all(b"\x05").await?;
    assert_eq!(tokio::time::timeout(HANG_GUARD, peer.read_u8()).await??, 6);
    drop(peer);
    restarted.0.kill()?;
    restarted.0.wait()?;
    Ok(())
}
