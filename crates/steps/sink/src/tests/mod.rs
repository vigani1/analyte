//! Tests src behavior with synthetic fixtures.
//! Kept whole: Shared receiver, proxy and store fixtures keep delivery scenarios on the same controlled HTTP harness.
#[path = "../../../../analyte/tests/support/guard.rs"]
mod guard;
use super::*;
use analyte_model::*;
use analyte_store::{FreeSpace, Handle};
use guard::HANG_GUARD;
use std::{
    path::Path,
    time::{Duration, Instant},
};
use tokio::io::{AsyncBufReadExt as _, AsyncReadExt as _};

const SECRET: &str = "whsec_AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=";
type TestResult = Result<(), Box<dyn std::error::Error>>;

struct Secrets(&'static str);

impl SecretsSource for Secrets {
    fn credentials(&self, _: &Identifier) -> Result<Credentials, Error> {
        Ok(Credentials {
            api_key: self.0.into(),
            signing_secrets: vec![analyte_contract::SigningSecret::parse(SECRET)?],
        })
    }
}

struct RotatingSecrets;

impl SecretsSource for RotatingSecrets {
    fn credentials(&self, _: &Identifier) -> Result<Credentials, Error> {
        Ok(Credentials {
            api_key: "key".into(),
            signing_secrets: vec![
                analyte_contract::SigningSecret::parse(
                    "whsec_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
                )?,
                analyte_contract::SigningSecret::parse(SECRET)?,
            ],
        })
    }
}

struct Space;

impl FreeSpace for Space {
    fn available(&self, _: &Path) -> std::io::Result<u64> {
        Ok(u64::MAX)
    }
}

/// Storage deadline for test requests. CI machines with `synchronous=FULL`
/// can be slow; the behavior under test never depends on this budget.
fn until() -> Result<Instant, Error> {
    deadline(Duration::from_secs(30))
}

fn limits() -> Limits {
    Limits {
        request_timeout: Duration::from_secs(2),
        store_timeout: Duration::from_secs(30),
        backoff_min: Duration::from_secs(1),
        backoff_max: Duration::from_secs(3),
        freshness: Duration::from_secs(30),
        poll_interval: Duration::from_millis(10),
        page_size: 1,
        max_pages: 100,
    }
}

async fn store(path: &Path) -> Result<Handle, Error> {
    Ok(Handle::start(
        path.to_owned(),
        analyte_store::Limits {
            reserve_bytes: 1024,
            database_bytes: 100_000_000,
            record_bytes: 1_048_576,
            page_rows: 100,
            busy_timeout: Duration::from_millis(20),
        },
        Box::new(Space),
        16,
        until()?,
    )
    .await?)
}

fn name(value: &str) -> Result<NamespacedId, TextError> {
    Ok(NamespacedId {
        namespace: Identifier::new("synthetic")?,
        value: Identifier::new(value)?,
    })
}

fn receipt() -> Result<ReceiptMetadata, Box<dyn std::error::Error>> {
    Ok(ReceiptMetadata {
        id: ReceiptId::try_from(uuid::Uuid::now_v7())?,
        raw_digest: Digest(analyte_contract::content_digest(b"raw")),
        connection: Identifier::new("bench")?,
        machine: Identifier::new("machine")?,
        driver: name("driver")?,
        config_version: Identifier::new("1")?,
        config_digest: Digest(analyte_contract::content_digest(b"config")),
        received_at: jiff::Timestamp::now(),
    })
}

fn make_result(receipt: ReceiptId, run: &str) -> Result<ResultContent, Box<dyn std::error::Error>> {
    Ok(ResultContent {
        receipt,
        sample: Some(SampleIdentity {
            id: SampleId::new("000123")?,
            source: SampleSource::ScannedByMachine,
            position: None,
        }),
        processing: ProcessingMode::Production,
        identity: IdentityStatus::Resolved(ResultIdentity {
            observation: ObservationIdentity {
                code: name("observation")?,
                sub_id: SourceText::new("")?,
                run: SourceText::new(run)?,
                order: name("order")?,
            },
            revision: Revision(0),
        }),
        parts: NonEmptyEvidenceList::new(EvidenceList::new(vec![ResultPart {
            test: name("test")?,
            values: EvidenceList::new(vec![ResultValue::new(
                SourceText::new("<0.1")?,
                ValueInterpretation::Text,
            )])?,
            units: None,
            dilution: None,
            flags: EvidenceList::new(vec![])?,
            reference_range: None,
            comments: EvidenceList::new(vec![])?,
            qc_class: QcClass::Patient,
            source_status: SourceText::new("F")?,
            status: ResultStatus::Final,
            source_path: EvidenceList::new(vec![])?,
            times: EvidenceList::new(vec![])?,
            received_at: jiff::Timestamp::now(),
        }])?)?,
    })
}

fn corrected_content(result: &mut ResultContent) -> Result<(), Box<dyn std::error::Error>> {
    let mut parts = result.parts.as_slice().to_vec();
    for part in &mut parts {
        part.source_status = SourceText::new("C")?;
        part.status = ResultStatus::Corrected;
        part.values = EvidenceList::new(vec![ResultValue::new(
            SourceText::new("<0.2")?,
            ValueInterpretation::Text,
        )])?;
    }
    result.parts = NonEmptyEvidenceList::new(EvidenceList::new(parts)?)?;
    Ok(())
}

async fn enqueue(
    store: &Handle,
    metadata: ReceiptMetadata,
    result: ResultContent,
    delivery: analyte_store::Delivery,
) -> Result<(), Error> {
    store
        .request(until()?, move |s| {
            s.receive(&metadata, b"raw", b"config")?;
            let is_correction = result
                .parts
                .as_slice()
                .iter()
                .any(|part| part.source_status.as_str() == "C");
            s.interpret_reconciled(
                metadata.id,
                analyte_store::Interpretation {
                    driver: metadata.driver,
                    config_version: metadata.config_version,
                    config: b"config".to_vec(),
                    outcome: analyte_store::Reading::Results(vec![result]),
                },
                &[is_correction],
                &[],
                |_, _| Ok(vec![delivery]),
            )?;
            Ok(())
        })
        .await?;
    Ok(())
}

fn audit() -> Result<analyte_store::Audit, TextError> {
    Ok(analyte_store::Audit {
        actor: Identifier::new("test-operator")?,
        action: Identifier::new("release")?,
        targets: vec![],
        time: jiff::Timestamp::now().as_second(),
        reason: Reason::new("synthetic reconciliation")?,
    })
}

struct Peer {
    child: tokio::process::Child,
    url: String,
}

impl Peer {
    async fn start(directory: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        // Build the real reference server instead of depending on its HTTP framework.
        let mut build = tokio::process::Command::new(env!("CARGO"));
        build
            .args(["build", "-p", "analyte-receiver", "--message-format=json"])
            .kill_on_drop(true)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null());
        let output = tokio::time::timeout(HANG_GUARD, build.output()).await??;
        if !output.status.success() {
            return Err("reference receiver build failed".into());
        }
        let executable = String::from_utf8(output.stdout)?
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .find_map(|value| {
                if value.get("target")?.get("name")?.as_str()? != "analyte-receiver" {
                    return None;
                }
                value.get("executable")?.as_str().map(str::to_owned)
            })
            .ok_or("receiver artifact missing")?;
        let seed = directory.join("seed.json");
        std::fs::write(
            &seed,
            format!(
                "[{}]",
                include_str!("../../../../../docs/contract/vectors/order.json")
            ),
        )?;
        let mut child = tokio::process::Command::new(executable)
            .args(["--listen", "127.0.0.1:0", "--database"])
            .arg(directory.join("receiver.db"))
            .arg("--seed")
            .arg(seed)
            .env("ANALYTE_API_KEY", "key")
            .env("ANALYTE_SIGNING_SECRET", SECRET)
            .kill_on_drop(true)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()?;
        // This address comes from our child after it has seeded and bound its own
        // socket. Another process cannot satisfy readiness on a released port.

        let stdout = child
            .stdout
            .take()
            .ok_or("receiver startup output missing")?;
        let mut output = tokio::io::BufReader::new(stdout.take(128));
        let mut line = String::new();
        if tokio::time::timeout(HANG_GUARD, output.read_line(&mut line)).await?? == 0 {
            return Err("receiver exited before reporting its bound address".into());
        }
        let address: std::net::SocketAddr = line.trim().parse()?;
        if !address.ip().is_loopback() || address.port() == 0 {
            return Err("receiver did not report a bound loopback address".into());
        }
        Ok(Self {
            child,
            url: format!("http://{address}"),
        })
    }

    async fn stop(mut self) -> TestResult {
        self.child.kill().await?;
        tokio::time::timeout(HANG_GUARD, self.child.wait()).await??;
        Ok(())
    }
}

fn destination(url: &str) -> Result<Destination, Error> {
    Destination::for_loopback_test(
        Identifier::new("app").map_err(|_| Error::Configuration)?,
        url,
        Identifier::new("secrets").map_err(|_| Error::Configuration)?,
        limits(),
    )
}

async fn post(
    peer: &Peer,
    payload: &[u8],
    id: DeliveryId,
) -> Result<analyte_contract::DeliveryStatus, Box<dyn std::error::Error>> {
    let timestamp = jiff::Timestamp::now().as_second().to_string();
    let id = uuid::Uuid::from(id).to_string();
    let response = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .no_proxy()
        .build()?
        .post(format!("{}/results", peer.url))
        .bearer_auth("key")
        .header("webhook-id", &id)
        .header("webhook-timestamp", &timestamp)
        .header(
            "webhook-signature",
            analyte_contract::SigningSecret::parse(SECRET)?.sign(&id, &timestamp, payload)?,
        )
        .body(payload.to_vec())
        .send()
        .await?;
    let bytes = body(response).await?;
    let status: analyte_contract::StatusBody = analyte_contract::decode(&bytes)?;
    Ok(status.status)
}

async fn check_delivery_conflicts(store: &Handle, worker: &Worker<'_>, peer: &Peer) -> TestResult {
    let metadata = receipt()?;
    let original = make_result(metadata.id, "accepted-run")?;
    let delivery = build_delivery(
        &original,
        &metadata,
        Identifier::new("app")?,
        jiff::Timestamp::now(),
    )?;
    let bytes = delivery.payload.clone();
    let id = delivery.id;
    enqueue(store, metadata, original, delivery).await?;
    assert_eq!(
        worker.deliver_once().await?,
        DeliveryOutcome::Done(analyte_contract::DeliveryStatus::Accepted)
    );
    assert_eq!(
        post(peer, &bytes, id).await?,
        analyte_contract::DeliveryStatus::Duplicate
    );

    // Commit an original to the app before enqueueing different bytes with its ID.
    let metadata = receipt()?;
    let original = make_result(metadata.id, "conflict-run")?;
    let mut conflicting = build_delivery(
        &original,
        &metadata,
        Identifier::new("app")?,
        jiff::Timestamp::now(),
    )?;
    let conflict_id = conflicting.id;
    assert_eq!(
        post(peer, &conflicting.payload, conflict_id).await?,
        analyte_contract::DeliveryStatus::Accepted
    );
    conflicting.payload.push(b' ');
    enqueue(store, metadata, original, conflicting).await?;
    assert_eq!(worker.deliver_once().await?, DeliveryOutcome::Parked);
    let parked = store.request(until()?, |s| s.parked(0)).await?;
    assert!(parked.iter().any(|p| p.evidence.contains("conflict")));
    assert!(parked.iter().any(|p| p.evidence.contains("\"conflict\"")));

    let metadata = receipt()?;
    let mut correction = make_result(metadata.id, "conflict-run")?;
    if let IdentityStatus::Resolved(identity) = &mut correction.identity {
        identity.revision = Revision(1);
    }
    corrected_content(&mut correction)?;
    let delivery = build_delivery(
        &correction,
        &metadata,
        Identifier::new("app")?,
        jiff::Timestamp::now(),
    )?;
    enqueue(store, metadata, correction, delivery).await?;
    assert_eq!(worker.deliver_once().await?, DeliveryOutcome::Idle);
    let mut metadata = receipt()?;
    metadata.machine = Identifier::new("other-machine")?;
    let other = make_result(metadata.id, "conflict-run")?;
    let delivery = build_delivery(
        &other,
        &metadata,
        Identifier::new("app")?,
        jiff::Timestamp::now(),
    )?;
    enqueue(store, metadata, other, delivery).await?;
    assert_eq!(
        worker.deliver_once().await?,
        DeliveryOutcome::Done(analyte_contract::DeliveryStatus::Accepted)
    );

    Ok(())
}

async fn check_order_events(store: &Handle, destinations: &[Destination]) -> TestResult {
    let rotating = RotatingSecrets;
    let event_worker = Worker::new(store, destinations, &rotating);
    let mut event: analyte_contract::EventFields = serde_json::from_slice(include_bytes!(
        "../../../../../docs/contract/vectors/event.json"
    ))?;
    for state in [
        analyte_contract::document::OrderState::Sent,
        analyte_contract::document::OrderState::Accepted,
        analyte_contract::document::OrderState::Rejected,
        analyte_contract::document::OrderState::Done,
        analyte_contract::document::OrderState::CancelConfirmed,
        analyte_contract::document::OrderState::Uncertain,
    ] {
        event.delivery_id = analyte_contract::Text::try_from(uuid::Uuid::now_v7().to_string())?;
        event.state = state;
        let delivery = build_order_event(
            &analyte_contract::OrderEvent::try_from(event.clone())?,
            Identifier::new("app")?,
        )?;
        store
            .request(until()?, move |s| {
                s.enqueue_order_event(&delivery, jiff::Timestamp::now().as_second())
            })
            .await?;
        assert_eq!(
            event_worker.deliver_once().await?,
            DeliveryOutcome::Done(analyte_contract::DeliveryStatus::Accepted)
        );
    }

    Ok(())
}

async fn check_cache_changes(
    directory: &Path,
    store: &Handle,
    destination: &Destination,
    poller: &Poller<'_>,
) -> TestResult {
    let first = store
        .request(until()?, |s| {
            s.cursor(&Identifier::new("app").map_err(|_| analyte_store::Error::InvalidInput)?)
        })
        .await?;
    let mut receiver = analyte_receiver::Receiver::open(
        &directory.join("receiver.db"),
        "key".into(),
        analyte_contract::SigningSecret::parse(SECRET)?,
    )?;
    let mut order: analyte_contract::OrderFields = serde_json::from_slice(include_bytes!(
        "../../../../../docs/contract/vectors/order.json"
    ))?;
    order.version = std::num::NonZeroU64::new(2).ok_or("version")?;
    order.state = analyte_contract::document::OrderState::CancelRequested;
    receiver
        .seed(vec![analyte_contract::Order::try_from(order)?])
        .await?;
    receiver
        .shutdown()
        .await
        .map_err(|_| "receiver seed thread failed")?;
    poller.poll_once().await?;
    let cached = store
        .request(until()?, |s| {
            s.orders(
                &Identifier::new("app").map_err(|_| analyte_store::Error::InvalidInput)?,
                &SampleId::new("000Ab12").map_err(|_| analyte_store::Error::InvalidInput)?,
                jiff::Timestamp::now().as_second(),
                30,
            )
        })
        .await?;
    let analyte_store::Lookup::Orders(orders) = cached else {
        return Err("orders not trusted".into());
    };
    let order: analyte_contract::Order =
        serde_json::from_str(&orders.first().ok_or("order missing")?.document)?;
    assert_eq!(order.fields().version.get(), 2);
    assert_eq!(
        order.fields().state,
        analyte_contract::document::OrderState::CancelRequested
    );
    let next = store
        .request(until()?, |s| {
            s.cursor(&Identifier::new("app").map_err(|_| analyte_store::Error::InvalidInput)?)
        })
        .await?;
    assert_ne!(first, next);
    store
        .request(until()?, |s| {
            s.order_page(&analyte_store::OrderPage {
                destination: Identifier::new("app")
                    .map_err(|_| analyte_store::Error::InvalidInput)?,
                cursor: Identifier::new("invalid-cursor")
                    .map_err(|_| analyte_store::Error::InvalidInput)?,
                complete: true,
                polled: jiff::Timestamp::now().as_second(),
                orders: vec![],
            })
        })
        .await?;
    assert!(matches!(
        poller.poll_once().await,
        Err(Error::PollStatus(410))
    ));
    assert!(
        !destination
            .health(store, jiff::Timestamp::now().as_second())
            .await?
            .cache_trusted
    );
    poller.poll_once().await?;
    assert!(
        destination
            .health(store, jiff::Timestamp::now().as_second())
            .await?
            .cache_trusted
    );

    Ok(())
}

async fn check_pause_and_resume(
    store: &Handle,
    destinations: &[Destination],
    destination: &Destination,
    worker: &Worker<'_>,
) -> TestResult {
    let wrong = Secrets("wrong");
    let metadata = receipt()?;
    let original = make_result(metadata.id, "conflict-run")?;
    // A new observation bypasses the intentionally parked observation above.
    let mut delivery = build_delivery(
        &original,
        &metadata,
        Identifier::new("app")?,
        jiff::Timestamp::now(),
    )?;
    delivery.ordering_key.machine = Identifier::new("pause-machine")?;
    let pause_id = delivery.id;
    let mut metadata = metadata;
    metadata.machine = Identifier::new("pause-machine")?;
    enqueue(store, metadata, original, delivery).await?;
    assert_eq!(
        Worker::new(store, destinations, &wrong)
            .deliver_once()
            .await?,
        DeliveryOutcome::Paused
    );
    assert!(
        destination
            .health(store, jiff::Timestamp::now().as_second())
            .await?
            .paused
    );
    assert_eq!(worker.deliver_once().await?, DeliveryOutcome::Idle);

    let mut release = audit()?;
    release.action = Identifier::new("delivery-resend")?;
    release.targets = vec![Identifier::new(&format!(
        "delivery:{}",
        uuid::Uuid::from(pause_id)
    ))?];
    store
        .request(until()?, move |s| {
            s.resend_delivery(pause_id, false, &release)
        })
        .await?;
    assert!(
        !destination
            .health(store, jiff::Timestamp::now().as_second())
            .await?
            .paused
    );
    assert_eq!(
        worker.deliver_once().await?,
        DeliveryOutcome::Done(analyte_contract::DeliveryStatus::Accepted)
    );

    Ok(())
}

/// A bounded synthetic HTTP fault relay; only the real reference receiver is upstream.
struct Proxy {
    url: String,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    task: Option<tokio::task::JoinHandle<Result<(), String>>>,
    requests: tokio::sync::mpsc::Receiver<(String, Vec<u8>)>,
}

impl Proxy {
    async fn start(
        upstream: String,
        lose_response: bool,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let url = format!("http://{}", listener.local_addr()?);
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let (requests, received) = tokio::sync::mpsc::channel(8);
        let task = tokio::spawn(async move {
            tokio::select! {
                _=stopped=>Ok(()),
                result=relay(listener,upstream,lose_response,requests)=>result.map_err(|e|e.to_string()),
            }
        });
        Ok(Self {
            url,
            stop: Some(stop),
            task: Some(task),
            requests: received,
        })
    }

    async fn stop(mut self) -> TestResult {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(mut task) = self.task.take() {
            match tokio::time::timeout(HANG_GUARD, &mut task).await {
                Ok(result) => result?.map_err(std::io::Error::other)?,
                Err(error) => {
                    task.abort();
                    let _ = task.await;
                    return Err(error.into());
                }
            }
        }
        Ok(())
    }
}

impl Drop for Proxy {
    fn drop(&mut self) {
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}

async fn relay(
    listener: tokio::net::TcpListener,
    upstream: String,
    lose_response: bool,
    requests: tokio::sync::mpsc::Sender<(String, Vec<u8>)>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .no_proxy()
        .build()?;
    let mut first = true;
    loop {
        let (mut stream, _) = listener.accept().await?;
        let mut bytes = Vec::new();
        let boundary = loop {
            if let Some(index) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                break index.checked_add(4).ok_or("header overflow")?;
            }
            let mut buffer = [0; 1024];
            let read = tokio::time::timeout(HANG_GUARD, stream.read(&mut buffer)).await??;
            if read == 0 || bytes.len() > 16_384 {
                return Err("invalid synthetic HTTP headers".into());
            }
            bytes.extend_from_slice(buffer.get(..read).ok_or("read bounds")?);
        };
        let headers =
            std::str::from_utf8(bytes.get(..boundary).ok_or("header bounds")?)?.to_owned();
        let length = headers
            .lines()
            .filter_map(|line| line.split_once(':'))
            .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
            .ok_or("length missing")?
            .1
            .trim()
            .parse::<usize>()?;
        if length > analyte_contract::MAX_DOCUMENT_BYTES {
            return Err("request too large".into());
        }
        let expected = boundary.checked_add(length).ok_or("length overflow")?;
        while bytes.len() < expected {
            let mut buffer = [0; 4096];
            let read = tokio::time::timeout(HANG_GUARD, stream.read(&mut buffer)).await??;
            if read == 0 {
                return Err("request incomplete".into());
            }
            bytes.extend_from_slice(buffer.get(..read).ok_or("read bounds")?);
            if bytes.len() > expected {
                return Err("request excess".into());
            }
        }
        let payload = bytes.get(boundary..expected).ok_or("body bounds")?.to_vec();
        let id = headers
            .lines()
            .filter_map(|line| line.split_once(':'))
            .find(|(name, _)| name.eq_ignore_ascii_case("webhook-id"))
            .ok_or("id missing")?
            .1
            .trim()
            .to_owned();
        requests.send((id, payload.clone())).await?;
        if first && !lose_response {
            stream.write_all(b"HTTP/1.1 503 Service Unavailable\r\nRetry-After: 2\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await?;
        } else {
            let mut request = client.post(format!("{upstream}/results")).body(payload);
            for (name, value) in headers.lines().filter_map(|line| line.split_once(':')) {
                if name.eq_ignore_ascii_case("authorization")
                    || name.to_ascii_lowercase().starts_with("webhook-")
                {
                    request = request.header(name, value.trim());
                }
            }
            let response = request.send().await?;
            let status = response.status().as_u16();
            let body = body(response).await?;
            if first && lose_response {
                // The upstream committed. Close without returning its response:
                // a lost answer, independent of the client's wall-clock deadline.
                drop(stream);
            } else {
                let header = format!(
                    "HTTP/1.1 {status} Response\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                stream.write_all(header.as_bytes()).await?;
                stream.write_all(&body).await?;
            }
        }
        first = false;
    }
}

async fn fixed_time<T>(future: impl Future<Output = T>) -> T {
    let started = Instant::now();
    tokio::pin!(future);
    loop {
        assert!(
            started.elapsed() < HANG_GUARD,
            "storage did not complete within test guard"
        );
        tokio::select! {biased; result=&mut future=>return result, ()=tokio::task::yield_now()=>{}}
    }
}

async fn enqueue_retry_correction(store: &Handle) -> TestResult {
    let metadata = receipt()?;
    let mut correction = make_result(metadata.id, "retry-run")?;
    if let IdentityStatus::Resolved(identity) = &mut correction.identity {
        identity.revision = Revision(1);
    }
    corrected_content(&mut correction)?;
    let delivery = build_delivery(
        &correction,
        &metadata,
        Identifier::new("app")?,
        jiff::Timestamp::now(),
    )?;
    enqueue(store, metadata, correction, delivery).await?;
    Ok(())
}

mod workers;

mod bounds;

mod delivery;

mod alert_webhooks;
