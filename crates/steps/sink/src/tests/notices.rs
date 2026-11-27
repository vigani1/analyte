//! Tests that Needs review notices go through the same outbox as results, and that a lab app answering 404 or 501 for them stops getting them without pausing its results.
use super::*;

/// The published opened-notice vector under a fresh delivery ID.
fn vector_notice() -> Result<analyte_contract::ReviewNotice, Box<dyn std::error::Error>> {
    let mut value: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../../../../../docs/contract/vectors/review-notice.json"
    ))?;
    value["delivery_id"] = serde_json::Value::String(uuid::Uuid::now_v7().to_string());
    Ok(analyte_contract::decode(&serde_json::to_vec(&value)?)?)
}

/// Opts `app` in, parks one item about a fresh receipt and queues its notice.
async fn queue_notice(store: &Handle) -> Result<DeliveryId, Box<dyn std::error::Error>> {
    let notice = vector_notice()?;
    let metadata = receipt()?;
    let item = store
        .request::<_, TestStoreError>(until()?, move |s| {
            s.configure_review_notices(&[Identifier::new("app")?])?;
            s.receive(&metadata, b"raw", b"config")?;
            Ok::<_, TestStoreError>(s.park(&analyte_store::Parked {
                kind: Identifier::new(analyte_store::ParkedKind::InterpretationFailure.as_str())?,
                reason: Reason::new("unreadable")?,
                targets: vec![Identifier::new(&uuid::Uuid::from(metadata.id).to_string())?],
            })?)
        })
        .await?;
    let delivery = encode_review_notice(
        &notice,
        Identifier::new("app")?,
        item,
        analyte_store::ReviewPhase::Opened,
    )?;
    let id = delivery.id;
    store
        .request::<_, TestStoreError>(until()?, move |s| {
            s.enqueue_review_notices(&Identifier::new("app")?, &[delivery], item)?;
            Ok::<_, TestStoreError>(())
        })
        .await?;
    Ok(id)
}

// ADR 0038, contract v1 "POST /review-notices": a notice is signed and
// stored by the reference receiver like any delivery.
#[tokio::test]
async fn the_reference_receiver_stores_a_notice() -> TestResult {
    let directory = tempfile::tempdir()?;
    let peer = Peer::start(directory.path()).await?;
    let mut store = store(&directory.path().join("store.db")).await?;
    let destination = destination(&peer.url)?;
    let secrets = Secrets("key");
    let worker = Worker::for_destination(&store, &destination, &secrets);
    queue_notice(&store).await?;
    assert_eq!(
        worker.deliver_once().await?,
        DeliveryOutcome::Done(analyte_contract::DeliveryStatus::Accepted)
    );
    assert_eq!(worker.deliver_once().await?, DeliveryOutcome::Idle);
    store.shutdown(until()?).await?;
    peer.stop().await?;
    Ok(())
}

/// A lab app that takes results but answers `status` for /review-notices.
async fn without_notices(
    status: u16,
) -> Result<(String, tokio::task::JoinHandle<std::io::Result<()>>), std::io::Error> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let url = format!("http://{}", listener.local_addr()?);
    let code = axum::http::StatusCode::from_u16(status).map_err(std::io::Error::other)?;
    let router = axum::Router::new()
        .route(
            "/review-notices",
            axum::routing::post(move || async move { code }),
        )
        .route(
            "/results",
            axum::routing::post(|| async {
                (
                    [("content-type", "application/json")],
                    r#"{"contract_version":1,"status":"accepted"}"#,
                )
            }),
        );
    let task = tokio::spawn(async move { axum::serve(listener, router).await });
    Ok((url, task))
}

// ADR 0038: 404 or 501 for /review-notices means the lab app takes no
// notices. The notice is declined, the lab app is not paused, nothing is
// parked, and the result queued after it is still delivered.
#[tokio::test]
async fn a_lab_app_without_notices_still_gets_its_results() -> TestResult {
    for status in [404, 501] {
        let directory = tempfile::tempdir()?;
        let (url, server) = without_notices(status).await?;
        let mut store = store(&directory.path().join("store.db")).await?;
        let destination = destination(&url)?;
        let secrets = Secrets("key");
        let worker = Worker::for_destination(&store, &destination, &secrets);
        let notice = queue_notice(&store).await?;
        let metadata = receipt()?;
        let result = make_result(metadata.id, "run-1")?;
        let delivery = encode_delivery(
            &result,
            &metadata,
            &model()?,
            Identifier::new("app")?,
            jiff::Timestamp::now(),
        )?;
        let result_id = delivery.id;
        enqueue(&store, metadata, result, delivery).await?;
        let parked = store.request(until()?, |s| s.parked_counts()).await?;
        assert_eq!(
            worker.deliver_once().await?,
            DeliveryOutcome::Declined,
            "{status}"
        );
        assert_eq!(
            worker.deliver_once().await?,
            DeliveryOutcome::Done(analyte_contract::DeliveryStatus::Accepted),
            "{status}"
        );
        let (states, status_now, parked_now) = store
            .request::<_, TestStoreError>(until()?, move |s| {
                Ok((
                    (
                        s.delivery(notice)?.map(|d| d.state),
                        s.delivery(result_id)?.map(|d| d.state),
                    ),
                    s.review_notice_status(&Identifier::new("app")?)?,
                    s.parked_counts()?,
                ))
            })
            .await?;
        assert_eq!(
            states,
            (
                Some(analyte_store::DeliveryState::Declined),
                Some(analyte_store::DeliveryState::Done)
            )
        );
        assert!(status_now.and_then(|s| s.declined).is_some(), "{status}");
        assert_eq!(parked_now, parked);
        store.shutdown(until()?).await?;
        server.abort();
    }
    Ok(())
}
