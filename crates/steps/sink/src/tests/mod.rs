//! Exercises HTTP delivery, immutable identities and bounded order polling against local synthetic peers.
//! Kept whole: Shared receiver, proxy and store fixtures keep delivery scenarios on the same controlled HTTP harness.
use super::*;
use analyte_fixtures as guard;
use analyte_model::*;
use analyte_store::Handle;
use guard::HANG_GUARD;
use std::{
    path::Path,
    time::{Duration, Instant},
};
use tokio::io::{AsyncBufReadExt as _, AsyncReadExt as _};

use analyte_fixtures::SECRET;
type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Mixed operations used only by synthetic fixture setup.
#[derive(Debug, thiserror::Error)]
enum TestStoreError {
    #[error("alert attempt does not exist")]
    MissingAlertAttempt,
    #[error(transparent)]
    IntakeError(#[from] analyte_store::IntakeError),
    #[error(transparent)]
    ReconciliationError(#[from] analyte_store::ReconciliationError),
    #[error(transparent)]
    DeliveryError(#[from] analyte_store::DeliveryError),
    #[error(transparent)]
    OperatorError(#[from] analyte_store::OperatorError),
    #[error(transparent)]
    OrderError(#[from] analyte_store::OrderError),
    #[error(transparent)]
    WorkerError(#[from] analyte_store::WorkerError),
    #[error(transparent)]
    WorkerStartError(#[from] analyte_store::WorkerStartError),
    #[error(transparent)]
    Text(#[from] TextError),
}

struct Secrets(&'static str);

impl SecretsSource for Secrets {
    fn credentials(&self, _: &Identifier) -> Result<Credentials, DestinationError> {
        Ok(Credentials {
            api_key: self.0.into(),
            signing_secrets: vec![analyte_contract::SigningSecret::parse(SECRET)?],
        })
    }
}

struct RotatingSecrets;

impl SecretsSource for RotatingSecrets {
    fn credentials(&self, _: &Identifier) -> Result<Credentials, DestinationError> {
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

use analyte_fixtures::UnlimitedSpace as Space;

/// Storage deadline for test requests. CI machines with `synchronous=FULL`
/// can be slow; the behavior under test never depends on this budget.
fn until() -> Result<Instant, std::io::Error> {
    deadline(
        Duration::from_secs(30),
        std::io::Error::other("test deadline overflow"),
    )
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

async fn store(path: &Path) -> Result<Handle, Box<dyn std::error::Error>> {
    Ok(Handle::start(
        path.to_owned(),
        {
            let mut limits = analyte_fixtures::store_limits();
            limits.record_bytes = 1_048_576;
            limits.busy_timeout = Duration::from_millis(20);
            limits
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
) -> Result<(), Box<dyn std::error::Error>> {
    store
        .request::<_, TestStoreError>(until()?, move |s| {
            s.receive(&metadata, b"raw", b"config")?;
            let is_correction = result
                .parts
                .as_slice()
                .iter()
                .any(|part| part.source_status.as_str() == "C");
            s.reconcile(
                analyte_store::ReconcileAction::Interpret(metadata.id),
                analyte_store::Interpretation {
                    departures: None,
                    driver: metadata.driver,
                    config_version: metadata.config_version,
                    config: b"config".to_vec(),
                    outcome: analyte_store::Reading::Results(vec![result]),
                },
                analyte_store::ReconcileInput::standard(&[is_correction], &[]),
                |_, _| Ok(vec![delivery]),
            )?;
            Ok::<_, TestStoreError>(())
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
    receiver: analyte_receiver::Receiver,
    stop: tokio::sync::oneshot::Sender<()>,
    task: tokio::task::JoinHandle<std::io::Result<()>>,
    url: String,
}

impl Peer {
    async fn start(directory: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let receiver = analyte_receiver::Receiver::open(
            &directory.join("receiver.db"),
            "key".into(),
            analyte_contract::SigningSecret::parse(SECRET)?,
        )?;
        receiver
            .seed(vec![analyte_contract::decode(include_bytes!(
                "../../../../../docs/contract/vectors/order.json"
            ))?])
            .await?;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let url = format!("http://{}", listener.local_addr()?);
        let router = receiver.router();
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(async {
                    let _ = stopped.await;
                })
                .await
        });
        Ok(Self {
            receiver,
            stop,
            task,
            url,
        })
    }

    async fn stop(mut self) -> TestResult {
        let _ = self.stop.send(());
        tokio::time::timeout(HANG_GUARD, self.task).await???;
        self.receiver.shutdown().await?;
        Ok(())
    }
}

fn destination(url: &str) -> Result<Destination, Box<dyn std::error::Error>> {
    Ok(Destination::for_loopback_test(
        Identifier::new("app").map_err(DeliveryError::OrderId)?,
        url,
        Identifier::new("secrets").map_err(DeliveryError::OrderId)?,
        limits(),
    )?)
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
    let bytes = body(response, DeliveryError::ResponseTooLarge).await?;
    let status: analyte_contract::StatusBody = analyte_contract::decode(&bytes)?;
    Ok(status.status)
}

async fn check_delivery_conflicts(store: &Handle, worker: &Worker<'_>, peer: &Peer) -> TestResult {
    let metadata = receipt()?;
    let original = make_result(metadata.id, "accepted-run")?;
    let delivery = encode_delivery(
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
    let mut conflicting = encode_delivery(
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
    let delivery = encode_delivery(
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
    let delivery = encode_delivery(
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
        let delivery = encode_order_event(
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
        .request::<_, TestStoreError>(until()?, |s| {
            Ok(s.cursor(&Identifier::new("app").map_err(TestStoreError::Text)?)?)
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
        .request::<_, TestStoreError>(until()?, |s| {
            Ok(s.orders(
                &Identifier::new("app").map_err(TestStoreError::Text)?,
                &SampleId::new("000Ab12").map_err(TestStoreError::Text)?,
                jiff::Timestamp::now().as_second(),
                30,
            )?)
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
        .request::<_, TestStoreError>(until()?, |s| {
            Ok(s.cursor(&Identifier::new("app").map_err(TestStoreError::Text)?)?)
        })
        .await?;
    assert_ne!(first, next);
    store
        .request::<_, TestStoreError>(until()?, |s| {
            Ok(s.order_page(&analyte_store::OrderPage {
                destination: Identifier::new("app").map_err(TestStoreError::Text)?,
                cursor: Identifier::new("invalid-cursor").map_err(TestStoreError::Text)?,
                complete: true,
                polled: jiff::Timestamp::now().as_second(),
                orders: vec![],
            })?)
        })
        .await?;
    assert!(matches!(
        poller.poll_once().await,
        Err(PollError::PollStatus(410))
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
    let mut delivery = encode_delivery(
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
            let body = body(response, DeliveryError::ResponseTooLarge).await?;
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
    let delivery = encode_delivery(
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

mod diagnostics;
