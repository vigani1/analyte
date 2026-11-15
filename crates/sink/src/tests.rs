use super::*;
use analyte_model::*;
use analyte_store::{FreeSpace, Handle};
use std::{
    path::Path,
    time::{Duration, Instant},
};

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
            values: EvidenceList::new(vec![ResultValue {
                original: SourceText::new("<0.1")?,
                parsed: ValueInterpretation::Text,
            }])?,
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
        part.values = EvidenceList::new(vec![ResultValue {
            original: SourceText::new("<0.2")?,
            parsed: ValueInterpretation::Text,
        }])?;
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

#[test]
fn adr0008_build_refuses_nonproduction_and_parked_evidence() -> TestResult {
    let receipt = receipt()?;
    let mut result = make_result(receipt.id, "build-validation-run")?;
    for mode in [
        ProcessingMode::Training,
        ProcessingMode::Debug,
        ProcessingMode::QualityControl,
    ] {
        result.processing = mode;
        assert!(matches!(
            build_delivery(
                &result,
                &receipt,
                Identifier::new("app")?,
                jiff::Timestamp::now()
            ),
            Err(Error::ProcessingMode)
        ));
    }
    result.processing = ProcessingMode::Production;
    result.sample = None;
    assert!(matches!(
        build_delivery(
            &result,
            &receipt,
            Identifier::new("app")?,
            jiff::Timestamp::now()
        ),
        Err(Error::RequiresParking)
    ));
    Ok(())
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
        let output = tokio::time::timeout(Duration::from_secs(120), build.output()).await??;
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
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        drop(listener);
        let seed = directory.join("seed.json");
        std::fs::write(
            &seed,
            format!(
                "[{}]",
                include_str!("../../../docs/contract/vectors/order.json")
            ),
        )?;
        let child = tokio::process::Command::new(executable)
            .args(["--listen", &address.to_string(), "--database"])
            .arg(directory.join("receiver.db"))
            .arg("--seed")
            .arg(seed)
            .env("ANALYTE_API_KEY", "key")
            .env("ANALYTE_SIGNING_SECRET", SECRET)
            .kill_on_drop(true)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()?;
        let mut peer = Self {
            child,
            url: format!("http://{address}"),
        };
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if peer.child.try_wait()?.is_some() {
                    return Err(std::io::Error::other("receiver exited before readiness"));
                }
                if tokio::net::TcpStream::connect(address).await.is_ok() {
                    return Ok(());
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await??;
        Ok(peer)
    }

    async fn stop(mut self) -> TestResult {
        self.child.kill().await?;
        tokio::time::timeout(Duration::from_secs(5), self.child.wait()).await??;
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
        .timeout(Duration::from_secs(2))
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

#[tokio::test]
async fn adr0008_reference_peer_delivery_conflict_pause_ordering_events_and_adr0004_cache()
-> TestResult {
    let directory = tempfile::tempdir()?;
    let peer = Peer::start(directory.path()).await?;
    let mut store = store(&directory.path().join("store.db")).await?;
    let destinations = [destination(&peer.url)?];
    let destination = destinations.first().ok_or("destination missing")?;
    let secrets = Secrets("key");
    let worker = Worker::new(&store, &destinations, &secrets);
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
    enqueue(&store, metadata, original, delivery).await?;
    assert_eq!(
        worker.deliver_once().await?,
        DeliveryOutcome::Done(analyte_contract::DeliveryStatus::Accepted)
    );
    assert_eq!(
        post(&peer, &bytes, id).await?,
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
        post(&peer, &conflicting.payload, conflict_id).await?,
        analyte_contract::DeliveryStatus::Accepted
    );
    conflicting.payload.push(b' ');
    enqueue(&store, metadata, original, conflicting).await?;
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
    enqueue(&store, metadata, correction, delivery).await?;
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
    enqueue(&store, metadata, other, delivery).await?;
    assert_eq!(
        worker.deliver_once().await?,
        DeliveryOutcome::Done(analyte_contract::DeliveryStatus::Accepted)
    );

    let rotating = RotatingSecrets;
    let event_worker = Worker::new(&store, &destinations, &rotating);
    let mut event: analyte_contract::EventFields =
        serde_json::from_slice(include_bytes!("../../../docs/contract/vectors/event.json"))?;
    for state in [
        analyte_contract::evidence::OrderState::Sent,
        analyte_contract::evidence::OrderState::Accepted,
        analyte_contract::evidence::OrderState::Rejected,
        analyte_contract::evidence::OrderState::Done,
        analyte_contract::evidence::OrderState::CancelConfirmed,
        analyte_contract::evidence::OrderState::Uncertain,
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

    let poller = Poller::new(&store, destination, &secrets);
    poller.poll_once().await?;
    let health = destination
        .health(&store, jiff::Timestamp::now().as_second())
        .await?;
    assert!(health.cache_trusted && health.snapshot_complete);
    assert!(health.last_successful_poll.is_some());
    assert!(matches!(
        destination
            .orders(
                &store,
                &SampleId::new("000Ab12")?,
                jiff::Timestamp::now().as_second()
            )
            .await?,
        analyte_store::Lookup::Orders(_)
    ));
    assert!(matches!(
        destination
            .orders(
                &store,
                &SampleId::new("000Ab12")?,
                jiff::Timestamp::now().as_second().saturating_add(30)
            )
            .await?,
        analyte_store::Lookup::NotTrusted
    ));

    let first = store
        .request(until()?, |s| {
            s.cursor(&Identifier::new("app").map_err(|_| analyte_store::Error::InvalidInput)?)
        })
        .await?;
    let receiver = analyte_receiver::Receiver::open(
        &directory.path().join("receiver.db"),
        "key".into(),
        analyte_contract::SigningSecret::parse(SECRET)?,
    )?;
    let mut order: analyte_contract::OrderFields =
        serde_json::from_slice(include_bytes!("../../../docs/contract/vectors/order.json"))?;
    order.version = std::num::NonZeroU64::new(2).ok_or("version")?;
    order.state = analyte_contract::evidence::OrderState::CancelRequested;
    receiver
        .seed(vec![analyte_contract::Order::try_from(order)?])
        .await?;
    receiver
        .shutdown()
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
        analyte_contract::evidence::OrderState::CancelRequested
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
            .health(&store, jiff::Timestamp::now().as_second())
            .await?
            .cache_trusted
    );
    poller.poll_once().await?;
    assert!(
        destination
            .health(&store, jiff::Timestamp::now().as_second())
            .await?
            .cache_trusted
    );

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
    enqueue(&store, metadata, original, delivery).await?;
    assert_eq!(
        Worker::new(&store, &destinations, &wrong)
            .deliver_once()
            .await?,
        DeliveryOutcome::Paused
    );
    assert!(
        destination
            .health(&store, jiff::Timestamp::now().as_second())
            .await?
            .paused
    );
    assert_eq!(worker.deliver_once().await?, DeliveryOutcome::Idle);

    let release = audit()?;
    store
        .request(until()?, move |s| s.release_delivery(pause_id, &release))
        .await?;
    assert!(
        !destination
            .health(&store, jiff::Timestamp::now().as_second())
            .await?
            .paused
    );
    assert_eq!(
        worker.deliver_once().await?,
        DeliveryOutcome::Done(analyte_contract::DeliveryStatus::Accepted)
    );

    // Closing the owner signal interrupts idle and active loops within a bound.
    let (stop, stopped) = watch::channel(false);
    let work = worker.run(stopped.clone());
    let poll = poller.run(stopped);
    let cancel = async {
        tokio::time::sleep(Duration::from_millis(20)).await;
        drop(stop);
    };
    tokio::time::timeout(Duration::from_secs(1), async {
        let (w, p, ()) = tokio::join!(work, poll, cancel);
        w?;
        p?;
        Ok::<_, Error>(())
    })
    .await??;
    store.shutdown(until()?).await?;
    peer.stop().await?;
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
            match tokio::time::timeout(Duration::from_secs(2), &mut task).await {
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
        .timeout(Duration::from_secs(2))
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
            let read =
                tokio::time::timeout(Duration::from_secs(2), stream.read(&mut buffer)).await??;
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
            let read =
                tokio::time::timeout(Duration::from_secs(2), stream.read(&mut buffer)).await??;
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
                // Upstream has durably committed; hold the answer past the client deadline.
                // Longer than the 1 s client deadline, shorter than the 5 s backoff,
                // so this sequential relay is free again before the retry arrives.
                tokio::time::sleep(Duration::from_secs(3)).await;
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

async fn retry_due(next: i64) -> Result<(), Box<dyn std::error::Error>> {
    let wait = next
        .saturating_sub(jiff::Timestamp::now().as_second())
        .saturating_add(1);
    tokio::time::sleep(Duration::from_secs(u64::try_from(wait)?)).await;
    Ok(())
}

#[tokio::test]
async fn adr0008_lost_response_retry_reuses_id_and_bytes_and_503_honours_retry_after() -> TestResult
{
    let directory = tempfile::tempdir()?;
    let peer = Peer::start(directory.path()).await?;
    for lose_response in [true, false] {
        let mut proxy = Proxy::start(peer.url.clone(), lose_response).await?;
        let path = directory
            .path()
            .join(if lose_response { "lost.db" } else { "retry.db" });
        let mut store = store(&path).await?;
        let mut destination = destination(&proxy.url)?;
        // Loose enough for a slow runner's real round trip through the proxy.
        destination.limits.request_timeout = Duration::from_secs(1);
        // Timestamps have whole-second resolution; a 1 s backoff can already be due
        // on a slow runner before the "not yet due" check below.
        destination.limits.backoff_min = Duration::from_secs(5);
        destination.limits.backoff_max = Duration::from_secs(5);
        let destinations = [destination];
        let secrets = Secrets("key");
        let worker = Worker::new(&store, &destinations, &secrets);
        let metadata = receipt()?;
        let result = make_result(metadata.id, "retry-run")?;
        let delivery = build_delivery(
            &result,
            &metadata,
            Identifier::new("app")?,
            jiff::Timestamp::now(),
        )?;
        let id = delivery.id;
        let bytes = delivery.payload.clone();
        enqueue(&store, metadata, result, delivery).await?;
        let before = jiff::Timestamp::now().as_second();
        let DeliveryOutcome::RetryAt(next) = worker.deliver_once().await? else {
            return Err("not retried".into());
        };
        if !lose_response {
            assert!(next >= before.saturating_add(2));
        }
        if lose_response {
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
            enqueue(&store, metadata, correction, delivery).await?;
        }
        assert_eq!(worker.deliver_once().await?, DeliveryOutcome::Idle);
        retry_due(next).await?;
        assert_eq!(
            worker.deliver_once().await?,
            DeliveryOutcome::Done(if lose_response {
                analyte_contract::DeliveryStatus::Duplicate
            } else {
                analyte_contract::DeliveryStatus::Accepted
            })
        );
        let first = tokio::time::timeout(Duration::from_secs(1), proxy.requests.recv())
            .await?
            .ok_or("first request missing")?;
        let second = tokio::time::timeout(Duration::from_secs(1), proxy.requests.recv())
            .await?
            .ok_or("retry missing")?;
        assert_eq!(first, second);
        assert_eq!(first, (uuid::Uuid::from(id).to_string(), bytes));
        if lose_response {
            assert_eq!(
                worker.deliver_once().await?,
                DeliveryOutcome::Done(analyte_contract::DeliveryStatus::Accepted)
            );
        }
        assert_eq!(
            store
                .request(until()?, |s| s.health(
                    &Identifier::new("app").map_err(|_| analyte_store::Error::InvalidInput)?,
                    jiff::Timestamp::now().as_second(),
                    30
                ))
                .await?
                .pending_count,
            0
        );
        store.shutdown(until()?).await?;
        proxy.stop().await?;
    }
    peer.stop().await?;
    Ok(())
}

#[tokio::test]
async fn rule7_active_delivery_and_order_poll_cancel_without_waiting_for_http_deadline()
-> TestResult {
    use tokio::io::AsyncReadExt as _;
    let directory = tempfile::tempdir()?;
    let mut store = store(&directory.path().join("store.db")).await?;
    let secrets = Secrets("key");
    for polling in [false, true] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let mut destination = destination(&format!("http://{}", listener.local_addr()?))?;
        destination.limits.request_timeout = Duration::from_secs(30);
        let destinations = [destination];
        let destination = destinations.first().ok_or("destination missing")?;
        if !polling {
            let metadata = receipt()?;
            let result = make_result(metadata.id, "cancel-run")?;
            let delivery = build_delivery(
                &result,
                &metadata,
                Identifier::new("app")?,
                jiff::Timestamp::now(),
            )?;
            enqueue(&store, metadata, result, delivery).await?;
        }
        let (stop, stopped) = watch::channel(false);
        let worker = Worker::new(&store, &destinations, &secrets);
        let poller = Poller::new(&store, destination, &secrets);
        let run = async {
            if polling {
                poller.run(stopped).await
            } else {
                worker.run(stopped).await
            }
        };
        let cancel = async {
            let (mut stream, _) = listener.accept().await?;
            let mut byte = [0; 1];
            stream.read_exact(&mut byte).await?;
            stop.send(true).map_err(std::io::Error::other)?;
            // Keep the socket alive while cancellation must finish.
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok::<_, std::io::Error>(())
        };
        tokio::time::timeout(Duration::from_secs(1), async {
            let (run, cancel) = tokio::join!(run, cancel);
            run?;
            cancel?;
            Ok::<_, Box<dyn std::error::Error>>(())
        })
        .await??;
    }
    // Cancellation after claim preserves uncertain evidence for audited reconciliation.
    let health = store
        .request(until()?, |s| {
            s.health(
                &Identifier::new("app").map_err(|_| analyte_store::Error::InvalidInput)?,
                jiff::Timestamp::now().as_second(),
                30,
            )
        })
        .await?;
    assert_eq!(health.pending_count, 1);
    assert!(
        store
            .request(until()?, |s| s.claim(jiff::Timestamp::now().as_second()))
            .await?
            .is_none()
    );
    store.shutdown(until()?).await?;
    Ok(())
}

#[tokio::test]
async fn adr0008_shutdown_drains_active_attempt_or_leaves_same_claim_for_recovery() -> TestResult {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    for complete in [true, false] {
        let directory = tempfile::tempdir()?;
        let mut store = store(&directory.path().join("store.db")).await?;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let destinations = [destination(&format!("http://{}", listener.local_addr()?))?];
        let metadata = receipt()?;
        let result = make_result(metadata.id, "drain-run")?;
        let delivery = build_delivery(
            &result,
            &metadata,
            Identifier::new("app")?,
            jiff::Timestamp::now(),
        )?;
        let id = delivery.id;
        let bytes = delivery.payload.clone();
        enqueue(&store, metadata, result, delivery).await?;
        let secrets = Secrets("key");
        let worker = Worker::new(&store, &destinations, &secrets);
        let (stop, stopped) = watch::channel(false);
        let peer = async {
            let (mut stream, _) = listener.accept().await?;
            let mut headers = Vec::new();
            while !headers.ends_with(b"\r\n\r\n") {
                if headers.len() >= 8192 {
                    return Err(std::io::Error::other("oversized test headers"));
                }
                headers.push(stream.read_u8().await?);
            }
            let mut body = vec![0; bytes.len()];
            stream.read_exact(&mut body).await?;
            assert_eq!(body, bytes);
            stop.send_replace(true);
            if complete {
                let body = r#"{"contract_version":1,"status":"accepted"}"#;
                stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await?;
            } else {
                // Hold the request far past the grace period so the drain must give up.
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
            Ok::<_, std::io::Error>(())
        };
        // A completing attempt gets a grace period a slow CI runner can meet; an
        // abandoned one gets a short grace that the peer outlasts by far.
        let grace = if complete {
            Duration::from_secs(5)
        } else {
            Duration::from_millis(75)
        };
        let (outcome, peer) = tokio::time::timeout(Duration::from_secs(10), async {
            tokio::join!(worker.run_draining(stopped, grace), peer)
        })
        .await?;
        peer?;
        if complete {
            outcome?;
            assert_eq!(
                destinations
                    .first()
                    .ok_or("destination missing")?
                    .health(&store, jiff::Timestamp::now().as_second())
                    .await?
                    .pending_count,
                0
            );
        } else {
            assert!(matches!(outcome, Err(Error::Timeout)));
            store
                .request(until()?, |s| {
                    s.recover_in_flight(
                        jiff::Timestamp::now().as_second(),
                        &analyte_store::Audit {
                            actor: Identifier::new("startup")
                                .map_err(|_| analyte_store::Error::InvalidInput)?,
                            action: Identifier::new("recover")
                                .map_err(|_| analyte_store::Error::InvalidInput)?,
                            targets: vec![],
                            time: 0,
                            reason: Reason::new("ADR 0008 interrupted idempotent delivery")
                                .map_err(|_| analyte_store::Error::InvalidInput)?,
                        },
                    )
                })
                .await?;
            let claim = store
                .request(until()?, |s| s.claim(jiff::Timestamp::now().as_second()))
                .await?
                .ok_or("recovered claim missing")?;
            assert_eq!(claim.id, id);
            assert_eq!(claim.payload, bytes);
        }
        store.shutdown(until()?).await?;
    }
    Ok(())
}
