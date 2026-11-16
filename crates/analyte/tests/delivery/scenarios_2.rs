//! Tests HTTP delivery retries and stable delivery identities.
use super::*;
use crate::support::step;

#[tokio::test]
async fn plan11_adr0008_restored_outbox_retries_same_id_bytes_as_duplicate() -> Result<(), Error> {
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
        let backup = directory.path().join("backup.db");
        let target = backup.clone();
        store
            .request(
                std::time::Instant::now() + Duration::from_secs(5),
                move |store| {
                    store.backup(&target, std::time::Instant::now() + Duration::from_secs(5))
                },
            )
            .await?;
        let secrets = secrets(&program, "synthetic-api")?;
        let destinations = [analyte_sink::Destination::for_loopback_test(
            analyte_model::Identifier::new("demo")?,
            &url,
            analyte_model::Identifier::new("demo")?,
            analyte_sink::Limits {
                request_timeout: Duration::from_secs(30),
                store_timeout: Duration::from_secs(2),
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
        let worker = analyte_sink::Worker::new(&store, &destinations, &secrets);
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
                    .checked_add(Duration::from_secs(5))
                    .ok_or("deadline")?,
            )
            .await?;
        service::restore(
            program,
            backup,
            analyte_store::Audit {
                actor: analyte_model::Identifier::new("operator")?,
                action: analyte_model::Identifier::new("store-restore")?,
                targets: vec![],
                time: 0,
                reason: analyte_model::Reason::new("synthetic recovery")?,
            },
        )
        .await?;
        let restarted = Running::start(&path, &url, "synthetic-api").await?;
        let duplicate = reference.event().await?;
        assert!(duplicate.outcome.contains("duplicate"));
        assert_eq!(duplicate.id, original.id);
        assert_eq!(duplicate.bytes, original.bytes);
        wait_state(&path, "done", 1).await?;
        assert_eq!(receiver_count(&receiver_path)?, 1);
        restarted.stop().await?;
        reference.close().await?;
    }
    Ok(())
}

// Plan section 11: the running evaluator must not gate receive ACKs, app delivery,
// or the service's existing bounded shutdown path.
#[tokio::test]
async fn plan11_alert_evaluator_keeps_receive_delivery_and_shutdown_bounded() -> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let fixture = config(temp.path(), false)?;
    let original = std::fs::read_to_string(&fixture)?;
    std::fs::write(
        &fixture,
        format!(
            "alert_interval_seconds=1\n{original}\n[[alerts]]\nkind='machine_silent'\nthreshold=1\nmachine='hl7'\nresponder='lab duty officer'\n[[alerts]]\nkind='restore_paused'\nresponder='lab duty officer'\n"
        ),
    )?;
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let mut reference =
        Reference::start(&temp.path().join("reference.db"), listener, false).await?;
    let running = Running::start(&fixture, &reference.url, "synthetic-api").await?;
    let mut peer = connect(&fixture, "hl7").await?;
    tokio::time::timeout(HANG_GUARD, async {
        loop {
            let count: i64 =
                database(&fixture)?
                    .query_row("SELECT count(*) FROM alert_state", [], |r| r.get(0))?;
            if count == 2 {
                break;
            }
            tokio::task::yield_now().await;
        }
        Ok::<_, Error>(())
    })
    .await??;
    let ack = tokio::time::timeout(HANG_GUARD, hl7(&mut peer, ORU)).await??;
    assert!(ack.contains("MSA|AA|message1"));
    let event = tokio::time::timeout(HANG_GUARD, reference.event()).await??;
    assert_content(&event, "hl7", "00012a", "12.30", "final", 0)?;
    drop(peer);
    tokio::time::timeout(HANG_GUARD, running.stop()).await??;
    let state: bool = database(&fixture)?.query_row(
        "SELECT active FROM alert_state WHERE key=?1",
        [serde_json::to_string(
            &analyte_store::AlertKey::RestorePaused,
        )?],
        |r| r.get(0),
    )?;
    assert!(!state);
    reference.close().await?;
    Ok(())
}

// Plan section 11 and ADR 0008: an explicit CLI resend reaches the real reference
// receiver as a duplicate, with the identical ID and wire body, and one audit.
#[tokio::test]
async fn plan11_adr0008_operator_resend_reference_receiver_duplicate_same_id_and_bytes()
-> Result<(), Error> {
    let directory = tempfile::tempdir()?;
    let path = config(directory.path(), false)?;
    let receiver_path = directory.path().join("reference.db");
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let mut reference = Reference::start(&receiver_path, listener, false).await?;
    let service = step(
        "start resend service",
        Running::start(&path, &reference.url, "synthetic-api"),
    )
    .await?;
    let mut peer = step("connect resend peer", connect(&path, "hl7")).await?;
    assert!(
        step("receive result ACK", hl7(&mut peer, ORU))
            .await?
            .contains("MSA|AA")
    );
    let original = step("receive original delivery", reference.event()).await?;
    assert!(original.outcome.contains("accepted"));
    step("commit original done", wait_state(&path, "done", 1)).await?;
    refuse_incomplete_resends(&path, &original).await?;
    let output = step(
        "resend existing delivery CLI",
        tokio::process::Command::new(env!("CARGO_BIN_EXE_analyte"))
            .kill_on_drop(true)
            .arg("--config")
            .arg(&*path)
            .args([
                "--actor",
                "synthetic-operator",
                "deliveries",
                "resend",
                &original.id,
                "--reason",
                "app data recovery requested",
            ])
            .output(),
    )
    .await?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let duplicate = step("receive duplicate delivery", reference.event()).await?;
    assert!(duplicate.outcome.contains("duplicate"));
    assert_eq!(duplicate.id, original.id);
    assert_eq!(duplicate.bytes, original.bytes);
    step("commit duplicate done", wait_state(&path, "done", 1)).await?;
    assert_eq!(receiver_count(&receiver_path)?, 1);
    let db = database(&path)?;
    let evidence: String = db.query_row(
        "SELECT evidence FROM audit WHERE json_extract(evidence,'$.action')='delivery-resend'",
        [],
        |r| r.get(0),
    )?;
    let audit: analyte_store::Audit = serde_json::from_str(&evidence)?;
    assert_eq!(audit.actor.as_str(), "synthetic-operator");
    assert!(
        audit
            .targets
            .iter()
            .any(|t| t.as_str() == format!("delivery:{}", original.id))
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM audit WHERE json_extract(evidence,'$.action')='delivery-resend'",
            [],
            |r| r.get::<_, i64>(0)
        )?,
        1
    );
    // No operator resend writes to the analyzer connection.
    assert!(
        // Deliberate quiet window: no reply may arrive.
        tokio::time::timeout(Duration::from_millis(50), peer.read_u8())
            .await
            .is_err()
    );
    drop(db);
    step("stop resend service", service.stop()).await?;
    step("close resend receiver", reference.close()).await?;
    Ok(())
}

async fn refuse_incomplete_resends(path: &Fixture, original: &Captured) -> Result<(), Error> {
    for (name, suffix) in [
        (
            "reject resend without actor CLI",
            vec!["--reason", "app recovery"],
        ),
        (
            "reject resend without reason CLI",
            vec!["--actor", "synthetic-operator"],
        ),
    ] {
        let refused = step(
            name,
            tokio::process::Command::new(env!("CARGO_BIN_EXE_analyte"))
                .kill_on_drop(true)
                .arg("--config")
                .arg(path.as_ref())
                .args(["deliveries", "resend", &original.id])
                .args(suffix)
                .output(),
        )
        .await?;
        assert!(!refused.status.success());
    }
    let unknown = uuid::Uuid::now_v7().to_string();
    let refused = step(
        "reject unknown resend ID CLI",
        tokio::process::Command::new(env!("CARGO_BIN_EXE_analyte"))
            .kill_on_drop(true)
            .arg("--config")
            .arg(path.as_ref())
            .args([
                "--actor",
                "synthetic-operator",
                "deliveries",
                "resend",
                &unknown,
                "--reason",
                "app recovery",
            ])
            .output(),
    )
    .await?;
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("UnknownDelivery"));
    Ok(())
}
