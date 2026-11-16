//! Tests HTTP delivery retries and stable delivery identities.
#[path = "../orders/autolyser.rs"]
mod autolyser_cases;
#[path = "../service/capture.rs"]
mod capture_cases;
#[path = "../service/gazelle.rs"]
mod gazelle_cases;
#[path = "../orders/hl7.rs"]
mod hl7_order_cases;
#[path = "../orders/hl7_send.rs"]
mod hl7_send_cases;
#[path = "../law/queries/mod.rs"]
mod law_queries;
#[path = "../law/results/mod.rs"]
mod law_results;
#[path = "../service/machines.rs"]
mod machine_definitions;
#[path = "../orders/cases.rs"]
mod order_cases;
#[path = "../orders/timeout.rs"]
mod order_timeout;
#[cfg(feature = "test-hooks")]
#[path = "../service/recovery.rs"]
mod service_recovery;
#[path = "../support/mod.rs"]
mod support;
use support::HANG_GUARD;
#[path = "../orders/vendor_support.rs"]
mod vendor_support;
#[path = "../orders/vidas.rs"]
mod vidas_cases;
use analyte::{Error, installation::config::Program, runtime::service};
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
    io::AsyncReadExt as _,
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
    unavailable: AtomicBool,
    release: Notify,
}

async fn capture_response(
    State(state): State<Arc<Capture>>,
    request: Request,
    next: Next,
) -> Response {
    if state.unavailable.load(Ordering::Acquire) {
        return axum::http::StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
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
        let _ = tokio::time::timeout(HANG_GUARD, state.release.notified()).await;
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
        let (events, captured_events) = mpsc::channel(16);
        let capture = Arc::new(Capture {
            events,
            hold_once: AtomicBool::new(hold),
            unavailable: AtomicBool::new(false),
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
            events: captured_events,
            capture,
            stop,
            task,
            url,
        })
    }

    async fn event(&mut self) -> Result<Captured, Error> {
        Ok(tokio::time::timeout(HANG_GUARD, self.events.recv())
            .await?
            .ok_or("reference event missing")?)
    }

    async fn close(self) -> Result<(), Error> {
        self.capture.release.notify_one();
        self.stop.send(()).map_err(|()| "reference stopped")?;
        tokio::time::timeout(HANG_GUARD, self.task).await???;
        self.receiver
            .shutdown()
            .map_err(|_| "reference worker panicked")?;
        Ok(())
    }
}

// A bound, non-listening socket refuses connections while retaining its port.
// Recovery starts listening on that same socket after the old service has stopped.
enum AppOutage {
    Refused(tokio::net::TcpSocket),
    Http503(Reference),
}
impl AppOutage {
    async fn start(path: &Path, refused: bool) -> Result<(Self, String), Error> {
        let socket = tokio::net::TcpSocket::new_v4()?;
        socket.bind("127.0.0.1:0".parse()?)?;
        let address = socket.local_addr()?;
        // ADR 0008: another reference receiver must not claim this endpoint during the outage.
        assert_eq!(
            TcpListener::bind(address)
                .await
                .err()
                .ok_or("outage endpoint was not reserved")?
                .kind(),
            std::io::ErrorKind::AddrInUse
        );
        let outage = if refused {
            let error = tokio::time::timeout(HANG_GUARD, tokio::net::TcpStream::connect(address))
                .await?
                .err()
                .ok_or("bound non-listening socket accepted a connection")?;
            assert_eq!(error.kind(), std::io::ErrorKind::ConnectionRefused);
            Self::Refused(socket)
        } else {
            let reference = Reference::start(path, socket.listen(16)?, true).await?;
            reference.capture.unavailable.store(true, Ordering::Release);
            Self::Http503(reference)
        };
        Ok((outage, format!("http://{address}")))
    }
    async fn recover(self, path: &Path) -> Result<Reference, Error> {
        match self {
            Self::Refused(socket) => Reference::start(path, socket.listen(16)?, true).await,
            Self::Http503(reference) => {
                reference
                    .capture
                    .unavailable
                    .store(false, Ordering::Release);
                Ok(reference)
            }
        }
    }
}

async fn wait_outage_attempt(path: &Path) -> Result<(), Error> {
    tokio::time::timeout(HANG_GUARD, async {
        loop {
            let retries: i64 = database(path)?.query_row(
                "SELECT count(*) FROM attempts WHERE json_type(outcome,'$.RetryAt')='integer'",
                [],
                |r| r.get(0),
            )?;
            if retries > 0 {
                return Ok::<_, Error>(());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?
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

fn recording(
    name: &str,
) -> Result<
    (
        analyte_transport::recording::Header,
        Vec<analyte_transport::recording::Event>,
    ),
    Error,
> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../testing/sim/recordings")
        .join(format!("{name}.rec.jsonl"));
    Ok(analyte_transport::recording::read(
        std::fs::File::open(path)?,
        analyte_transport::recording::Limits::default(),
    )?)
}

fn recording_deadline(path: &Path) -> Result<Duration, Error> {
    let loaded = Program::load(path)?;
    let settings = loaded
        .machines()
        .iter()
        .find(|m| m.id().as_str() == "astm")
        .ok_or("machine absent")?
        .snapshot()
        .effective();
    Ok(Duration::from_millis(u64::from(
        settings.stack().timeouts().send_timeout_ms(),
    )))
}

async fn recording_eof(peer: &mut tokio::net::TcpStream) -> Result<(), Error> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    peer.shutdown().await?;
    let mut byte = [0];
    assert_eq!(
        tokio::time::timeout(HANG_GUARD, peer.read(&mut byte)).await??,
        0,
        "unexpected extra host byte"
    );
    Ok(())
}

fn recording_frames(path: &Path) -> Result<Vec<Vec<u8>>, Error> {
    let db = database(path)?;
    let mut statement = db.prepare("SELECT raw FROM frames ORDER BY sequence")?;
    Ok(statement
        .query_map([], |r| r.get(0))?
        .collect::<Result<Vec<_>, _>>()?)
}

async fn failure_recording_case(name: &str, completes: bool) -> Result<(), Error> {
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
    let (header, events) = recording(name)?;
    let deadline = recording_deadline(&path)?;
    let mut peer = connect(&path, "astm").await?;
    if name == "astm-split-merged" {
        use tokio::io::AsyncReadExt;
        analyte_sim::replay(
            &mut peer,
            events.get(..3).ok_or("split prefix absent")?,
            &[],
            deadline,
        )
        .await?;
        let mut byte = [0];
        assert!(
            // Deliberate quiet window: no reply may arrive.
            tokio::time::timeout(Duration::from_millis(50), peer.read(&mut byte))
                .await
                .is_err(),
            "partial frame must not be acknowledged"
        );
        assert!(recording_frames(&path)?.is_empty());
        analyte_sim::replay(
            &mut peer,
            events.get(3..).ok_or("merged suffix absent")?,
            &[],
            deadline,
        )
        .await?;
    } else if name == "astm-lost-ack" {
        // Observe and discard the first host ACK, then act as a machine that never received it.
        analyte_sim::replay(
            &mut peer,
            events.get(..4).ok_or("first send absent")?,
            &[],
            deadline,
        )
        .await?;
        let (_, original) = recording("astm")?;
        assert_eq!(
            recording_frames(&path)?,
            vec![original.get(2).ok_or("frame absent")?.bytes.clone()]
        );
        analyte_sim::replay(
            &mut peer,
            events.get(4..).ok_or("repeat absent")?,
            &[],
            deadline,
        )
        .await?;
    } else {
        analyte_sim::replay(&mut peer, &events, &header.exceptions, deadline).await?;
    }
    // EOF proves no extra ACK followed the expected NAK, incomplete frame, or EOT.
    recording_eof(&mut peer).await?;
    if completes {
        let delivered = reference.event().await?;
        assert_content(&delivered, "astm", "00012A", "12.30", "final", 0)?;
        wait_state(&path, "done", 1).await?;
    }
    service.stop().await?;
    assert_eq!(receiver_count(&receiver_path)?, i64::from(completes));
    assert_failure_recording_retained(&path, completes, &events)?;
    reference.close().await?;
    Ok(())
}

fn assert_completed_recording_retained(
    path: &Path,
    db: &rusqlite::Connection,
) -> Result<(), Error> {
    let (_, original) = recording("astm")?;
    let frames: Vec<_> = original
        .iter()
        .filter(|event| event.bytes.first() == Some(&2))
        .map(|event| event.bytes.clone())
        .collect();
    let expected = frames.concat();
    assert_eq!(
        recording_frames(path)?,
        frames,
        "repeated frame must not be journaled twice in one transfer"
    );
    let raw: Vec<u8> = db.query_row("SELECT raw FROM receipts WHERE raw=?1", [&expected], |r| {
        r.get(0)
    })?;
    assert_eq!(raw, expected);
    Ok(())
}

mod scenarios_1;

mod scenarios_2;

fn assert_failure_recording_retained(
    path: &Path,
    completes: bool,
    events: &[analyte_transport::recording::Event],
) -> Result<(), Error> {
    let db = database(path)?;
    let interpretations: i64 = db.query_row("SELECT count(*) FROM outbox", [], |r| r.get(0))?;
    assert_eq!(interpretations, i64::from(completes));
    if completes {
        assert_completed_recording_retained(path, &db)?;
    } else {
        assert!(
            recording_frames(path)?.is_empty(),
            "invalid or incomplete frames cannot become accepted journal frames"
        );
        let expected = &events.get(2).ok_or("bad input absent")?.bytes;
        let raw: Vec<u8> =
            db.query_row("SELECT raw FROM receipts WHERE raw=?1", [&expected], |r| {
                r.get(0)
            })?;
        assert_eq!(&raw, expected, "rejected wire bytes must be retained");
        let parked: i64 = db.query_row("SELECT count(*) FROM parked", [], |r| r.get(0))?;
        assert!(parked > 0);
    }
    Ok(())
}
