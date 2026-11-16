//! Plan §11, ADR 0003 crash recovery and AGENTS concurrency: real forced process exit.
use super::*;
use crate::support::HANG_GUARD;
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
    let orders = include_str!("../../../config/examples/orders.toml")
        // This test exercises crash recovery, not query response expiry.
        .replace("deadline_ms = 1000", "deadline_ms = 30000")
        .replace("send_ahead = true", "send_ahead = false")
        .replace(
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

#[tokio::test]
async fn plan11_adr0003_agents_forced_exit_preserves_receipts_delivery_and_uncertain_orders()
-> Result<(), Error> {
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
                    use std::io::Read as _;
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
    let wire = analyte_framing::mllp::encode(ORU, analyte_framing::mllp::Limit::new(1_048_576)?)?;
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
    use std::io::Read as _;
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
