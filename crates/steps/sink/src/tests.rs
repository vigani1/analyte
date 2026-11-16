//! Sink behavior tests belong beside delivery and polling to guard the HTTP boundary.
#[path = "../../../analyte/tests/support/guard.rs"]
mod guard;
use super::*;
use analyte_model::*;
use analyte_store::{FreeSpace, Handle};
use guard::HANG_GUARD;
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
                decoded_text: None,
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
            decoded_text: None,
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
                include_str!("../../../../docs/contract/vectors/order.json")
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
        use tokio::io::{AsyncBufReadExt as _, AsyncReadExt as _};
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
    check_delivery_conflicts(&store, &worker, &peer).await?;

    check_order_events(&store, &destinations).await?;

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

    check_cache_changes(directory.path(), &store, destination, &poller).await?;

    check_pause_and_resume(&store, &destinations, destination, &worker).await?;

    // Closing the owner signal interrupts idle and active loops within a bound.
    let (stop, stopped) = watch::channel(false);
    let work = worker.run(stopped.clone());
    let poll = poller.run(stopped);
    let cancel = async {
        // join! polls both owners before this branch closes their signal.
        drop(stop);
    };
    tokio::time::timeout(HANG_GUARD, async {
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
        "../../../../docs/contract/vectors/event.json"
    ))?;
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
    let receiver = analyte_receiver::Receiver::open(
        &directory.join("receiver.db"),
        "key".into(),
        analyte_contract::SigningSecret::parse(SECRET)?,
    )?;
    let mut order: analyte_contract::OrderFields = serde_json::from_slice(include_bytes!(
        "../../../../docs/contract/vectors/order.json"
    ))?;
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
        destination.limits.request_timeout = Duration::from_secs(30);
        destination.limits.backoff_min = Duration::from_secs(1);
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
        let clock = std::sync::atomic::AtomicI64::new(before);
        let now = || clock.load(std::sync::atomic::Ordering::Relaxed);
        let DeliveryOutcome::RetryAt(next) = worker.deliver_once_with_clock(&now).await? else {
            return Err("not retried".into());
        };
        assert_eq!(
            next,
            before
                .checked_add(if lose_response { 1 } else { 2 })
                .ok_or("retry timestamp overflow")?
        );
        if lose_response {
            enqueue_retry_correction(&store).await?;
        }
        assert_eq!(
            worker.deliver_once_with_clock(&now).await?,
            DeliveryOutcome::Idle
        );
        clock.store(
            next.checked_sub(1).ok_or("retry timestamp underflow")?,
            std::sync::atomic::Ordering::Relaxed,
        );
        assert_eq!(
            worker.deliver_once_with_clock(&now).await?,
            DeliveryOutcome::Idle
        );
        clock.store(next, std::sync::atomic::Ordering::Relaxed);
        assert_eq!(
            worker.deliver_once_with_clock(&now).await?,
            DeliveryOutcome::Done(if lose_response {
                analyte_contract::DeliveryStatus::Duplicate
            } else {
                analyte_contract::DeliveryStatus::Accepted
            })
        );
        let first = tokio::time::timeout(HANG_GUARD, proxy.requests.recv())
            .await?
            .ok_or("first request missing")?;
        let second = tokio::time::timeout(HANG_GUARD, proxy.requests.recv())
            .await?
            .ok_or("retry missing")?;
        assert_eq!(first, second);
        assert_eq!(first, (uuid::Uuid::from(id).to_string(), bytes));
        if lose_response {
            assert_eq!(
                worker.deliver_once_with_clock(&now).await?,
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
        let (finished, finish) = tokio::sync::oneshot::channel();
        let cancel = async {
            let (mut stream, _) = listener.accept().await?;
            let mut byte = [0; 1];
            stream.read_exact(&mut byte).await?;
            stop.send(true).map_err(std::io::Error::other)?;
            // Keep the socket alive until the worker actually finishes cancellation.
            let _ = finish.await;
            drop(stream);
            Ok::<_, std::io::Error>(())
        };
        tokio::time::timeout(HANG_GUARD, async {
            let run = async {
                let result = run.await;
                let _ = finished.send(());
                result
            };
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
        let (finished, finish) = tokio::sync::oneshot::channel();
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
                // Keep the request unresolved until the drain has returned.
                let _ = finish.await;
                drop(stream);
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
        let (outcome, peer) = tokio::time::timeout(HANG_GUARD, async {
            let run = async {
                let result = worker.run_draining(stopped, grace).await;
                let _ = finished.send(());
                result
            };
            tokio::join!(run, peer)
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

// Contract v1 HTTP document ceiling and clinical rule 7; synthetic local HTTP only.
#[tokio::test]
async fn mutation_contract_v1_rule7_response_body_ceiling_is_inclusive() -> TestResult {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    for length in [
        analyte_contract::MAX_DOCUMENT_BYTES,
        analyte_contract::MAX_DOCUMENT_BYTES + 1,
    ] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let server = async {
            let (mut stream, _) = listener.accept().await?;
            let mut headers = Vec::new();
            while !headers.ends_with(b"\r\n\r\n") {
                if headers.len() > 8192 {
                    return Err(std::io::Error::other("test headers too large"));
                }
                headers.push(stream.read_u8().await?);
            }
            stream
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n"
                    )
                    .as_bytes(),
                )
                .await?;
            stream.write_all(&vec![b'x'; length]).await?;
            Ok::<_, std::io::Error>(())
        };
        let client = async {
            let response = reqwest::Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(10))
                .build()?
                .get(format!("http://{address}"))
                .send()
                .await?;
            let result = body(response).await;
            if length == analyte_contract::MAX_DOCUMENT_BYTES {
                assert_eq!(result?.len(), length);
            } else {
                assert!(matches!(result, Err(Error::ResponseTooLarge)));
            }
            Ok::<_, Box<dyn std::error::Error>>(())
        };
        let (server, client) =
            tokio::time::timeout(HANG_GUARD, async { tokio::join!(server, client) }).await?;
        server?;
        client?;
    }
    Ok(())
}

#[test]
fn mutation_rule7_contract_v1_destination_limits_validate_each_independent_boundary() -> TestResult
{
    let app = Identifier::new("app")?;
    let secret = Identifier::new("secret")?;
    let construct = |limits| {
        Destination::new(
            app.clone(),
            "https://example.invalid/",
            secret.clone(),
            limits,
        )
    };
    for (field, values) in [
        (0, vec![Duration::ZERO, Duration::from_secs(301)]),
        (1, vec![Duration::ZERO, Duration::from_secs(301)]),
        (2, vec![Duration::from_millis(999)]),
        (3, vec![Duration::ZERO, Duration::from_secs(86401)]),
        (4, vec![Duration::from_millis(999)]),
        (5, vec![Duration::ZERO, Duration::from_secs(86401)]),
    ] {
        for value in values {
            let mut l = limits();
            match field {
                0 => l.request_timeout = value,
                1 => l.store_timeout = value,
                2 => l.backoff_min = value,
                3 => l.backoff_max = value,
                4 => l.freshness = value,
                5 => l.poll_interval = value,
                _ => unreachable!("fixed test fields"),
            };
            assert!(matches!(construct(l), Err(Error::Configuration)));
        }
    }
    for size in [0, 101] {
        let mut l = limits();
        l.page_size = size;
        assert!(matches!(construct(l), Err(Error::Configuration)));
    }
    let mut l = limits();
    l.max_pages = 0;
    assert!(matches!(construct(l), Err(Error::Configuration)));
    let mut l = limits();
    l.request_timeout = Duration::from_secs(300);
    l.store_timeout = Duration::from_secs(300);
    l.backoff_max = Duration::from_secs(86400);
    l.poll_interval = Duration::from_secs(86400);
    l.page_size = 100;
    assert_eq!(construct(l)?.id().as_str(), "app");
    let mut equal = limits();
    equal.backoff_max = equal.backoff_min;
    assert!(construct(equal).is_ok());
    Ok(())
}

#[test]
fn mutation_adr0003_rule4_rule5_build_delivery_preserves_three_identities_and_exact_values()
-> TestResult {
    let metadata = receipt()?;
    let mut result = make_result(metadata.id, "run-A")?;
    let mut parts = result.parts.as_slice().to_vec();
    let part = parts.first_mut().ok_or("part")?;
    part.values = EvidenceList::new(
        ["<0.1", "1,234", "positive", "ERROR"]
            .into_iter()
            .map(|s| {
                Ok(ResultValue {
                    original: SourceText::new(s)?,
                    decoded_text: None,
                    parsed: ValueInterpretation::Text,
                })
            })
            .collect::<Result<Vec<_>, TextError>>()?,
    )?;
    result.parts = NonEmptyEvidenceList::new(EvidenceList::new(parts)?)?;
    let delivery = build_delivery(
        &result,
        &metadata,
        Identifier::new("app")?,
        metadata.received_at,
    )?;
    let wire: serde_json::Value = serde_json::from_slice(&delivery.payload)?;
    assert_eq!(
        wire["delivery_id"],
        uuid::Uuid::from(delivery.id).to_string()
    );
    assert_eq!(
        wire["receipt"]["id"],
        uuid::Uuid::from(metadata.id).to_string()
    );
    assert_eq!(wire["results"][0]["receipt"], wire["receipt"]["id"]);
    assert_eq!(
        wire["results"][0]["identity"]["resolved"]["observation"]["run"],
        "run-A"
    );
    assert_eq!(wire["results"][0]["identity"]["resolved"]["revision"], 0);
    for (value, expected) in wire["results"][0]["parts"][0]["values"]
        .as_array()
        .ok_or("values")?
        .iter()
        .zip(["<0.1", "1,234", "positive", "ERROR"])
    {
        assert_eq!(value["original"], expected);
    }
    assert_eq!(wire["results"][0]["sample"]["id"], "000123");
    result.identity = IdentityStatus::Unresolved(Reason::new("run identity unknown")?);
    assert!(matches!(
        build_delivery(
            &result,
            &metadata,
            Identifier::new("app")?,
            metadata.received_at
        ),
        Err(Error::RequiresParking)
    ));
    Ok(())
}

#[test]
fn mutation_contract_v1_rule7_credentials_reject_each_invalid_component() -> TestResult {
    struct Supplied {
        key: String,
        count: usize,
    }
    impl SecretsSource for Supplied {
        fn credentials(&self, _: &Identifier) -> Result<Credentials, Error> {
            Ok(Credentials {
                api_key: self.key.clone(),
                signing_secrets: vec![analyte_contract::SigningSecret::parse(SECRET)?; self.count],
            })
        }
    }
    let d = destination("http://127.0.0.1:1234")?;
    for (key, count) in [
        (String::new(), 1),
        ("x".repeat(4097), 1),
        ("é".into(), 1),
        ("a\nb".into(), 1),
        ("key".into(), 0),
        ("key".into(), 9),
    ] {
        assert!(matches!(
            d.credentials(&Supplied { key, count }),
            Err(Error::Secrets)
        ));
    }
    for count in [1, 8] {
        let c = d.credentials(&Supplied {
            key: "x".repeat(4096),
            count,
        })?;
        assert_eq!(c.api_key.len(), 4096);
        assert_eq!(c.signing_secrets.len(), count);
    }
    Ok(())
}

// ADR 0003 retention; contract v1 delivery responses preserve the app's reason.
#[tokio::test]
async fn mutation_contract_v1_conflict_detail_requires_conflict_status_and_version_one()
-> TestResult {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    for (status, version, expected) in [
        (409, 1, "specific receiver explanation"),
        (409, 2, "app reported delivery identity conflict"),
        (422, 1, "specific receiver explanation"),
    ] {
        let directory = tempfile::tempdir()?;
        let mut store = store(&directory.path().join("store.db")).await?;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let destinations = [destination(&format!("http://{}", listener.local_addr()?))?];
        let metadata = receipt()?;
        let result = make_result(metadata.id, "detail-run")?;
        let delivery = build_delivery(
            &result,
            &metadata,
            Identifier::new("app")?,
            metadata.received_at,
        )?;
        let length = delivery.payload.len();
        enqueue(&store, metadata, result, delivery).await?;
        let (stop, mut stopped) = tokio::sync::oneshot::channel();
        let server = async {
            let accepted = tokio::select! {biased; _=&mut stopped=>return Ok(()), accepted=listener.accept()=>accepted};
            let (mut stream, _) = accepted?;
            let mut headers = Vec::new();
            while !headers.ends_with(b"\r\n\r\n") {
                if headers.len() > 8192 {
                    return Err(std::io::Error::other("test headers too large"));
                }
                headers.push(stream.read_u8().await?);
            }
            stream.read_exact(&mut vec![0; length]).await?;
            let body = format!(
                r#"{{"contract_version":{version},"error":"rejected","detail":"specific receiver explanation"}}"#
            );
            stream.write_all(format!("HTTP/1.1 {status} Response\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await?;
            Ok::<_, std::io::Error>(())
        };
        let secrets = Secrets("key");
        let worker = Worker::new(&store, &destinations, &secrets);
        let (server, outcome) = tokio::time::timeout(HANG_GUARD, async {
            tokio::join!(server, async {
                let outcome = worker.deliver_once().await;
                let _ = stop.send(());
                outcome
            })
        })
        .await?;
        server?;
        assert_eq!(outcome?, DeliveryOutcome::Parked);
        let parked = store.request(until()?, |s| s.parked(0)).await?;
        assert!(
            parked.iter().any(|p| p.evidence.contains(expected)),
            "missing expected app reason"
        );
        store.shutdown(until()?).await?;
    }
    Ok(())
}

#[tokio::test]
async fn mutation_rule7_draining_closed_or_set_stop_does_not_claim() -> TestResult {
    let directory = tempfile::tempdir()?;
    let mut store = store(&directory.path().join("store.db")).await?;
    store.shutdown(until()?).await?;
    let secrets = Secrets("key");
    let worker = Worker::new(&store, &[], &secrets);
    for closed in [false, true] {
        let (sender, receiver) = watch::channel(!closed);
        if closed {
            drop(sender);
        }
        worker
            .run_draining(receiver, Duration::from_secs(1))
            .await?;
    }
    Ok(())
}

#[test]
fn mutation_rule7_destination_url_byte_ceiling_is_inclusive() -> TestResult {
    for n in [4096, 4097] {
        let prefix = "https://example.invalid/";
        let url = format!("{prefix}{}", "x".repeat(n - prefix.len()));
        assert_eq!(
            Destination::new(
                Identifier::new("app")?,
                &url,
                Identifier::new("key")?,
                limits()
            )
            .is_ok(),
            n == 4096
        );
    }
    // The original fixture combined username and password, masking either check.
    for url in [
        "https://user@example.invalid/",
        "https://:password@example.invalid/",
    ] {
        assert!(matches!(
            Destination::new(
                Identifier::new("app")?,
                url,
                Identifier::new("key")?,
                limits()
            ),
            Err(Error::Configuration)
        ));
    }
    Ok(())
}

// Keep paused Tokio time fixed while the dedicated storage thread completes I/O.
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

#[tokio::test]
async fn mutation_rule7_idle_workers_wait_before_claiming_again() -> TestResult {
    use std::{future::Future as _, task::Poll};
    for draining in [false, true] {
        let directory = tempfile::tempdir()?;
        let mut store = store(&directory.path().join("store.db")).await?;
        let metadata = receipt()?;
        let result = make_result(metadata.id, "idle-run")?;
        let delivery = build_delivery(
            &result,
            &metadata,
            Identifier::new("app")?,
            metadata.received_at,
        )?;
        let secrets = Secrets("key");
        let worker = Worker::new(&store, &[], &secrets);
        let (_sender, receiver) = watch::channel(false);
        {
            let run = async {
                if draining {
                    worker.run_draining(receiver, Duration::from_secs(1)).await
                } else {
                    worker.run(receiver).await
                }
            };
            tokio::pin!(run);
            assert!(
                std::future::poll_fn(|cx| Poll::Ready(run.as_mut().poll(cx)))
                    .await
                    .is_pending()
            );
            store.request(until()?, |_| Ok(())).await?;
            tokio::time::pause();
            assert!(
                std::future::poll_fn(|cx| Poll::Ready(run.as_mut().poll(cx)))
                    .await
                    .is_pending()
            );
            fixed_time(enqueue(&store, metadata, result, delivery)).await?;
            assert!(
                std::future::poll_fn(|cx| Poll::Ready(run.as_mut().poll(cx)))
                    .await
                    .is_pending()
            );
            fixed_time(store.request(until()?, |_| Ok(()))).await?;
            let claim = fixed_time(
                store.request(until()?, |s| s.claim(jiff::Timestamp::now().as_second())),
            )
            .await?;
            assert!(claim.is_some(), "idle worker claimed before its 100ms wait");
            tokio::time::resume();
        }
        store.shutdown(until()?).await?;
    }
    Ok(())
}

// ADR 0004 / contract v1 GET orders: incomplete pages cannot establish trust,
// but a final page may retain its cursor. Page-size and chain limits are separate.
#[tokio::test]
async fn mutation_adr0004_contract_v1_pagination_progress_and_page_bounds() -> TestResult {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    for case in 0..4 {
        let directory = tempfile::tempdir()?;
        let mut store = store(&directory.path().join("store.db")).await?;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let mut destination = destination(&format!("http://{}", listener.local_addr()?))?;
        destination.limits.max_pages = 2;
        let secrets = Secrets("key");
        let poller = Poller::new(&store, &destination, &secrets);
        let (stop, mut stopped) = tokio::sync::oneshot::channel();
        let server = async {
            for index in 0..3 {
                let accept = tokio::select! {biased; _=&mut stopped=>return Ok(()), accepted=listener.accept()=>accepted};
                let (mut stream, _) = accept?;
                let mut headers = Vec::new();
                while !headers.ends_with(b"\r\n\r\n") {
                    if headers.len() > 8192 {
                        return Err(std::io::Error::other("test headers too large"));
                    }
                    headers.push(stream.read_u8().await?);
                }
                let cursor = if case == 2 {
                    format!("cursor-{index}")
                } else {
                    "cursor".into()
                };
                let complete = (case == 1 && index == 1) || case == 3;
                let orders = if case == 3 {
                    serde_json::json!(vec![
                        serde_json::from_slice::<serde_json::Value>(
                            include_bytes!("../../../../docs/contract/vectors/order.json")
                        )?;
                        2
                    ])
                } else {
                    serde_json::json!([])
                };
                let body=serde_json::json!({"contract_version":1,"orders":orders,"next_cursor":cursor,"complete":complete}).to_string();
                stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await?;
            }
            Ok::<_, std::io::Error>(())
        };
        let poll = async {
            let result = poller.poll_once().await;
            let _ = stop.send(());
            result
        };
        let (server, result) =
            tokio::time::timeout(HANG_GUARD, async { tokio::join!(server, poll) }).await?;
        server?;
        if case == 1 {
            result?;
        } else {
            assert!(matches!(result, Err(Error::FeedProgress)));
        }
        let health = destination
            .health(&store, jiff::Timestamp::now().as_second())
            .await?;
        assert_eq!(health.cache_trusted, case == 1);
        store.shutdown(until()?).await?;
    }
    Ok(())
}

#[tokio::test]
async fn mutation_rule7_owner_loops_and_cancellation_wait_for_their_signal() -> TestResult {
    use std::{future::Future as _, task::Poll};
    let (sender, mut receiver) = watch::channel(false);
    {
        let wait = cancelled(&mut receiver);
        tokio::pin!(wait);
        assert!(
            std::future::poll_fn(|cx| Poll::Ready(wait.as_mut().poll(cx)))
                .await
                .is_pending()
        );
        sender.send(true)?;
        assert!(
            std::future::poll_fn(|cx| Poll::Ready(wait.as_mut().poll(cx)))
                .await
                .is_ready()
        );
    }
    let directory = tempfile::tempdir()?;
    let mut store = store(&directory.path().join("store.db")).await?;
    let destination = destination("http://127.0.0.1:1")?;
    let secrets = Secrets("key");
    {
        let poller = Poller::new(&store, &destination, &secrets);
        let (sender, receiver) = watch::channel(false);
        let run = poller.run(receiver);
        tokio::pin!(run);
        assert!(
            std::future::poll_fn(|cx| Poll::Ready(run.as_mut().poll(cx)))
                .await
                .is_pending()
        );
        sender.send(true)?;
        assert!(matches!(
            std::future::poll_fn(|cx| Poll::Ready(run.as_mut().poll(cx))).await,
            Poll::Ready(Ok(()))
        ));
    }
    store.shutdown(until()?).await?;
    Ok(())
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
