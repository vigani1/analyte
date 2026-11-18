//! Tests HTTP delivery retries and stable delivery identities.
use super::*;

#[tokio::test]
async fn adr0008_reference_peer_delivery_conflict_pause_ordering_events_and_adr0004_cache()
-> TestResult {
    let directory = tempfile::tempdir()?;
    let peer = Peer::start(directory.path()).await?;
    let mut store = store(&directory.path().join("store.db")).await?;
    let destinations = [destination(&peer.url)?];
    let destination = destinations.first().ok_or("destination missing")?;
    let secrets = Secrets("key");
    let worker = Worker::for_destination(&store, &destinations[0], &secrets);
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
        Ok::<_, Box<dyn std::error::Error>>(())
    })
    .await??;
    store.shutdown(until()?).await?;
    peer.stop().await?;
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
        destination.limits.request_timeout = Duration::from_secs(30);
        destination.limits.backoff_min = Duration::from_secs(1);
        destination.limits.backoff_max = Duration::from_secs(5);
        let destinations = [destination];
        let secrets = Secrets("key");
        let worker = Worker::for_destination(&store, &destinations[0], &secrets);
        let metadata = receipt()?;
        let result = make_result(metadata.id, "retry-run")?;
        let delivery = encode_delivery(
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
                .request::<_, TestStoreError>(until()?, |s| Ok::<_, TestStoreError>(s.health(
                    &Identifier::new("app").map_err(TestStoreError::Text)?,
                    jiff::Timestamp::now().as_second(),
                    30
                )?))
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
async fn adr0008_shutdown_drains_active_attempt_or_leaves_same_claim_for_recovery() -> TestResult {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    for complete in [true, false] {
        let directory = tempfile::tempdir()?;
        let mut store = store(&directory.path().join("store.db")).await?;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let destinations = [destination(&format!("http://{}", listener.local_addr()?))?];
        let metadata = receipt()?;
        let result = make_result(metadata.id, "drain-run")?;
        let delivery = encode_delivery(
            &result,
            &metadata,
            Identifier::new("app")?,
            jiff::Timestamp::now(),
        )?;
        let id = delivery.id;
        let bytes = delivery.payload.clone();
        enqueue(&store, metadata, result, delivery).await?;
        let secrets = Secrets("key");
        let worker = Worker::for_destination(&store, &destinations[0], &secrets);
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
            assert!(matches!(outcome, Err(DeliveryError::Timeout)));
            let claim = recover_and_claim(&store, destinations[0].id()).await?;
            assert_eq!(claim.id, id);
            assert_eq!(claim.payload, bytes);
        }
        store.shutdown(until()?).await?;
    }
    Ok(())
}

/// Runs startup recovery, then claims the destination's head item as the next worker would.
async fn recover_and_claim(
    store: &Handle,
    destination: &Identifier,
) -> Result<analyte_store::Claim, Box<dyn std::error::Error>> {
    store
        .request::<_, TestStoreError>(until()?, |s| {
            Ok::<_, TestStoreError>(
                s.recover_in_flight(
                    jiff::Timestamp::now().as_second(),
                    &analyte_store::Audit {
                        actor: Identifier::new("startup").map_err(TestStoreError::Text)?,
                        action: Identifier::new("recover").map_err(TestStoreError::Text)?,
                        targets: vec![],
                        time: 0,
                        reason: Reason::new("ADR 0008 interrupted idempotent delivery")
                            .map_err(TestStoreError::Text)?,
                    },
                )?,
            )
        })
        .await?;
    let destination = destination.clone();
    Ok(store
        .request(until()?, move |s| {
            s.claim_destination(jiff::Timestamp::now().as_second(), &destination)
        })
        .await?
        .ok_or("recovered claim missing")?)
}

// Contract v1 HTTP document ceiling and clinical rule 7; synthetic local HTTP only.
#[test]
fn mutation_adr0003_rule4_rule5_encode_delivery_preserves_three_identities_and_exact_values()
-> TestResult {
    let metadata = receipt()?;
    let mut result = make_result(metadata.id, "run-A")?;
    let mut parts = result.parts.as_slice().to_vec();
    let part = parts.first_mut().ok_or("part")?;
    part.values = EvidenceList::new(
        ["<0.1", "1,234", "positive", "ERROR"]
            .into_iter()
            .map(|s| {
                Ok(ResultValue::new(
                    SourceText::new(s)?,
                    ValueInterpretation::Text,
                ))
            })
            .collect::<Result<Vec<_>, TextError>>()?,
    )?;
    result.parts = NonEmptyEvidenceList::new(EvidenceList::new(parts)?)?;
    let delivery = encode_delivery(
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
        encode_delivery(
            &result,
            &metadata,
            Identifier::new("app")?,
            metadata.received_at
        ),
        Err(DeliveryError::RequiresParking)
    ));
    Ok(())
}

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
        let delivery = encode_delivery(
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
        let worker = Worker::for_destination(&store, &destinations[0], &secrets);
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
