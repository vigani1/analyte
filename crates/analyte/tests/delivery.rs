//! Phase 1 synthetic program-to-reference-receiver tests (ADR 0008, plan 13).
#[path = "orders/cases.rs"]
mod order_cases;
mod support;
use analyte::{Error, config::Program, service};
use axum::{
    body::{Body, to_bytes},
    extract::{Request, State},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use support::*;
use tokio::{
    net::TcpListener,
    sync::{Notify, mpsc, oneshot},
    task::JoinHandle,
};

struct Captured {
    id: String,
    bytes: Vec<u8>,
    outcome: String,
}

struct Capture {
    events: mpsc::Sender<Captured>,
    hold_once: AtomicBool,
    release: Notify,
}

async fn capture_response(
    State(state): State<Arc<Capture>>,
    request: Request,
    next: Next,
) -> Response {
    if request.uri().path() != "/results" {
        return next.run(request).await;
    }
    let (parts, body) = request.into_parts();
    let id = parts
        .headers
        .get("webhook-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned();
    let Ok(bytes) = to_bytes(body, analyte_contract::MAX_DOCUMENT_BYTES).await else {
        return axum::http::StatusCode::PAYLOAD_TOO_LARGE.into_response();
    };
    let response = next
        .run(Request::from_parts(parts, Body::from(bytes.clone())))
        .await;
    let (parts, body) = response.into_parts();
    let Ok(body) = to_bytes(body, 65536).await else {
        return axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let outcome = String::from_utf8_lossy(&body).into_owned();
    if state
        .events
        .try_send(Captured {
            id,
            bytes: bytes.to_vec(),
            outcome,
        })
        .is_err()
    {
        return axum::http::StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    if state.hold_once.swap(false, Ordering::AcqRel) {
        let _ = tokio::time::timeout(Duration::from_secs(10), state.release.notified()).await;
    }
    Response::from_parts(parts, Body::from(body))
}

struct Reference {
    receiver: analyte_receiver::Receiver,
    events: mpsc::Receiver<Captured>,
    capture: Arc<Capture>,
    stop: oneshot::Sender<()>,
    task: JoinHandle<std::io::Result<()>>,
    url: String,
}

impl Reference {
    async fn start(path: &Path, listener: TcpListener, hold: bool) -> Result<Self, Error> {
        let receiver = analyte_receiver::Receiver::open(
            path,
            "synthetic-api".into(),
            analyte_contract::SigningSecret::parse(SECRET)?,
        )?;
        let (events, received) = mpsc::channel(16);
        let capture = Arc::new(Capture {
            events,
            hold_once: AtomicBool::new(hold),
            release: Notify::new(),
        });
        let router = receiver
            .router()
            .layer(axum::middleware::from_fn_with_state(
                capture.clone(),
                capture_response,
            ));
        let url = format!("http://{}", listener.local_addr()?);
        let (stop, stopped) = oneshot::channel();
        let task = tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(async {
                    let _ = stopped.await;
                })
                .await
        });
        Ok(Self {
            receiver,
            events: received,
            capture,
            stop,
            task,
            url,
        })
    }

    async fn event(&mut self) -> Result<Captured, Error> {
        Ok(
            tokio::time::timeout(Duration::from_secs(8), self.events.recv())
                .await?
                .ok_or("reference event missing")?,
        )
    }

    async fn close(self) -> Result<(), Error> {
        self.capture.release.notify_waiters();
        self.stop.send(()).map_err(|()| "reference stopped")?;
        tokio::time::timeout(Duration::from_secs(3), self.task).await???;
        self.receiver
            .shutdown()
            .map_err(|_| "reference worker panicked")?;
        Ok(())
    }
}

fn receiver_count(path: &Path) -> Result<i64, Error> {
    Ok(rusqlite::Connection::open(path)?
        .query_row("SELECT count(*) FROM deliveries", [], |r| r.get(0))?)
}

fn assert_content(
    event: &Captured,
    machine: &str,
    sample: &str,
    value: &str,
    status: &str,
    revision: u64,
) -> Result<(), Error> {
    assert!(event.outcome.contains("accepted"));
    let body: serde_json::Value = serde_json::from_slice(&event.bytes)?;
    assert_eq!(
        body.pointer("/delivery_id")
            .and_then(serde_json::Value::as_str),
        Some(event.id.as_str())
    );
    assert_eq!(
        body.pointer("/source/analyzer")
            .and_then(serde_json::Value::as_str),
        Some(machine)
    );
    assert_eq!(
        body.pointer("/results/0/sample/id")
            .and_then(serde_json::Value::as_str),
        Some(sample)
    );
    assert_eq!(
        body.pointer("/results/0/parts/0/values/0/original")
            .and_then(serde_json::Value::as_str),
        Some(value)
    );
    assert_eq!(
        body.pointer("/results/0/parts/0/units/original")
            .and_then(serde_json::Value::as_str),
        Some("mg/L")
    );
    assert_eq!(
        body.pointer("/results/0/parts/0/status")
            .and_then(serde_json::Value::as_str),
        Some(status)
    );
    assert_eq!(
        body.pointer("/results/0/identity/resolved/revision")
            .and_then(serde_json::Value::as_u64),
        Some(revision)
    );
    assert_eq!(
        body.pointer("/results/0/identity/resolved/observation/order/value")
            .and_then(serde_json::Value::as_str),
        Some(sample)
    );
    assert!(
        body.pointer("/receipt/raw_sha256")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|digest| digest.len() == 64)
    );
    assert_eq!(
        body.pointer("/results/0/parts/0/source_status")
            .and_then(serde_json::Value::as_str),
        Some(if status == "corrected" { "C" } else { "F" })
    );
    assert_eq!(
        body.pointer("/results/0/identity/resolved/observation/order/namespace")
            .and_then(serde_json::Value::as_str),
        Some("synthetic-orders")
    );
    assert_eq!(
        body.pointer("/results/0/receipt"),
        body.pointer("/receipt/id")
    );
    // The actual receiver verified bearer authentication, the signature, matching ID,
    // and the complete contract before recording this accepted response.
    Ok(())
}

#[tokio::test]
async fn plan13_adr0008_astm_and_hl7_deliver_once_retransmission_and_correction()
-> Result<(), Error> {
    let directory = tempfile::tempdir()?;
    let path = config(directory.path(), true)?;
    let receiver_path = directory.path().join("reference.db");
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let mut reference = Reference::start(&receiver_path, listener, false).await?;
    let service = Running::start(&path, &reference.url, "synthetic-api")?;
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
    astm(&mut astm_peer, ASTM).await?;
    let correction = String::from_utf8(ORU.to_vec())?
        .replace("12.30", "<0.1")
        .replace("|||||F", "|||||C")
        .replace("message1", "message2");
    assert!(
        hl7(&mut hl7_peer, correction.as_bytes())
            .await?
            .contains("MSA|AA|message2")
    );
    let corrected = reference.event().await?;
    assert_content(&corrected, "hl7", "00012a", "<0.1", "corrected", 1)?;
    wait_state(&path, "done", 3).await?;
    for mode in ["T", "D"] {
        let message = String::from_utf8(ORU.to_vec())?
            .replace("|P|2.5.1", &format!("|{mode}|2.5.1"))
            .replace("00012a", &format!("synthetic-{mode}"));
        assert!(
            hl7(&mut hl7_peer, message.as_bytes())
                .await?
                .contains("MSA|AA")
        );
    }
    let conflict = String::from_utf8(ORU.to_vec())?.replace("12.30", "99.9");
    assert!(
        hl7(&mut hl7_peer, conflict.as_bytes())
            .await?
            .contains("MSA|AE")
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

#[tokio::test]
async fn adr0008_receiver_outage_retries_immutable_delivery_when_it_returns() -> Result<(), Error> {
    let directory = tempfile::tempdir()?;
    let path = config(directory.path(), false)?;
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let service = Running::start(&path, &format!("http://{address}"), "synthetic-api")?;
    let mut peer = connect(&path, "hl7").await?;
    assert!(hl7(&mut peer, ORU).await?.contains("MSA|AA"));
    let (id, bytes): (String, Vec<u8>) =
        database(&path)?.query_row("SELECT id,payload FROM outbox", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?;
    wait_state(&path, "pending", 1).await?;
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

#[tokio::test]
async fn adr0008_wrong_api_key_pauses_destination_and_status_reports_it() -> Result<(), Error> {
    let directory = tempfile::tempdir()?;
    let path = config(directory.path(), false)?;
    let receiver_path = directory.path().join("reference.db");
    let reference = Reference::start(
        &receiver_path,
        TcpListener::bind("127.0.0.1:0").await?,
        false,
    )
    .await?;
    let service = Running::start(&path, &reference.url, "synthetic-wrong-key")?;
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

#[tokio::test]
async fn adr0008_interrupted_accepted_delivery_recovers_on_program_restart_as_duplicate()
-> Result<(), Error> {
    let directory = tempfile::tempdir()?;
    let path = config(directory.path(), false)?;
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let url = format!("http://{address}");
    let service = Running::start(&path, &url, "synthetic-api")?;
    let mut peer = connect(&path, "hl7").await?;
    assert!(hl7(&mut peer, ORU).await?.contains("MSA|AA"));
    service.stop().await?;
    let receiver_path = directory.path().join("reference.db");
    let mut reference = Reference::start(&receiver_path, listener, true).await?;
    let program = program(&path, &url)?;
    let mut store = service::open_store(&program, Box::new(Space)).await?;
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
    let original = tokio::time::timeout(Duration::from_secs(6), async {
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
    let restarted = Running::start(&path, &url, "synthetic-api")?;
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
    Ok(())
}

#[tokio::test]
async fn adr0008_shutdown_during_http_wait_is_bounded_and_restart_sends_same_delivery()
-> Result<(), Error> {
    let directory = tempfile::tempdir()?;
    let path = config(directory.path(), false)?;
    let receiver_path = directory.path().join("reference.db");
    let mut reference = Reference::start(
        &receiver_path,
        TcpListener::bind("127.0.0.1:0").await?,
        true,
    )
    .await?;
    let service = Running::start(&path, &reference.url, "synthetic-api")?;
    let mut peer = connect(&path, "hl7").await?;
    assert!(hl7(&mut peer, ORU).await?.contains("MSA|AA"));
    let original = reference.event().await?;
    assert!(original.outcome.contains("accepted"));
    // The receiver has committed, but its answer is deliberately withheld. The
    // request deadline expires inside the shutdown drain and preserves a retry.
    tokio::time::timeout(Duration::from_secs(5), service.stop()).await??;
    let restarted = Running::start(&path, &reference.url, "synthetic-api")?;
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

#[test]
fn adr0008_host_rotation_loads_each_named_key_and_missing_rotation_key_is_specific()
-> Result<(), Error> {
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
    let error = analyte::secrets::EnvironmentSecrets::load_with(&program, |name| {
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
