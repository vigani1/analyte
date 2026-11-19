//! Tests delivery worker cancellation and retry ownership.
use super::*;

// Safety rule 7 (ADR 0003)
#[tokio::test]
async fn active_work_cancels_before_http_deadline() -> TestResult {
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
            let delivery = encode_delivery(
                &result,
                &metadata,
                Identifier::new("app")?,
                jiff::Timestamp::now(),
            )?;
            enqueue(&store, metadata, result, delivery).await?;
        }
        let (stop, stopped) = watch::channel(false);
        let worker = Worker::for_destination(&store, &destinations[0], &secrets);
        let poller = Poller::new(&store, destination, &secrets);
        let run = async {
            if polling {
                Ok::<_, Box<dyn std::error::Error>>(poller.run(stopped).await?)
            } else {
                Ok::<_, Box<dyn std::error::Error>>(worker.run(stopped).await?)
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
        .request::<_, TestStoreError>(until()?, |s| {
            Ok::<_, TestStoreError>(s.health(
                &Identifier::new("app").map_err(TestStoreError::Text)?,
                jiff::Timestamp::now().as_second(),
                30,
            )?)
        })
        .await?;
    assert_eq!(health.pending_count, 1);
    let app = Identifier::new("app")?;
    assert!(
        store
            .request::<_, TestStoreError>(until()?, move |s| Ok::<_, TestStoreError>(
                s.claim_destination(jiff::Timestamp::now().as_second(), &app)?
            ))
            .await?
            .is_none()
    );
    store.shutdown(until()?).await?;
    Ok(())
}

// Safety rule 7 (ADR 0003); mutation-testing regression
#[tokio::test]
async fn stopped_worker_does_not_claim() -> TestResult {
    let directory = tempfile::tempdir()?;
    let mut store = store(&directory.path().join("store.db")).await?;
    store.shutdown(until()?).await?;
    let secrets = Secrets("key");
    let destination = destination("http://127.0.0.1:1")?;
    let worker = Worker::for_destination(&store, &destination, &secrets);
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

// Safety rule 7 (ADR 0003); mutation-testing regression
#[tokio::test]
async fn idle_workers_wait_before_claiming_again() -> TestResult {
    use std::{future::Future as _, task::Poll};
    for draining in [false, true] {
        let directory = tempfile::tempdir()?;
        let mut store = store(&directory.path().join("store.db")).await?;
        let metadata = receipt()?;
        let result = make_result(metadata.id, "idle-run")?;
        let delivery = encode_delivery(
            &result,
            &metadata,
            Identifier::new("app")?,
            metadata.received_at,
        )?;
        let secrets = Secrets("key");
        let destination = destination("http://127.0.0.1:1")?;
        let worker = Worker::for_destination(&store, &destination, &secrets);
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
            store
                .request(until()?, |_| Ok::<_, TestStoreError>(()))
                .await?;
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
            fixed_time(store.request(until()?, |_| Ok::<_, TestStoreError>(()))).await?;
            let app = destination.id().clone();
            let claim = fixed_time(store.request(until()?, move |s| {
                s.claim_destination(jiff::Timestamp::now().as_second(), &app)
            }))
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
// Safety rule 7 (ADR 0003); mutation-testing regression
#[tokio::test]
async fn owner_loops_wait_for_their_signal() -> TestResult {
    use std::{future::Future as _, task::Poll};
    let (sender, mut receiver) = watch::channel(false);
    {
        let wait = receiver.wait_for(|&stopped| stopped);
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

// ADR 0008: a stalled app must not hold another destination's immutable delivery.
// ADR 0008
#[tokio::test]
async fn slow_destination_never_delays_another_worker() -> TestResult {
    use tokio::io::AsyncWriteExt as _;
    let directory = tempfile::tempdir()?;
    let mut store = store(&directory.path().join("isolated.db")).await?;
    let slow_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let fast_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let mut slow = destination(&format!("http://{}", slow_listener.local_addr()?))?;
    slow.id = Identifier::new("slow")?;
    slow.limits.request_timeout = HANG_GUARD;
    let mut fast = destination(&format!("http://{}", fast_listener.local_addr()?))?;
    fast.id = Identifier::new("fast")?;
    let mut ids = Vec::new();
    for destination in [&slow, &fast] {
        let metadata = receipt()?;
        let result = make_result(metadata.id, destination.id.as_str())?;
        let delivery = encode_delivery(
            &result,
            &metadata,
            destination.id.clone(),
            jiff::Timestamp::now(),
        )?;
        ids.push(delivery.id);
        enqueue(&store, metadata, result, delivery).await?;
    }
    let secrets = Secrets("key");
    let slow_worker = Worker::for_destination(&store, &slow, &secrets);
    let fast_worker = Worker::for_destination(&store, &fast, &secrets);
    let mut slow_attempt = Box::pin(slow_worker.deliver_once());
    let fast_flow = async {
        let (mut stalled, _) = slow_listener.accept().await?;
        let mut byte = [0; 1];
        stalled.read_exact(&mut byte).await?;
        let (finished, finish) = tokio::sync::oneshot::channel();
        let respond = async {
            let (mut socket, _) = fast_listener.accept().await?;
            socket.read_exact(&mut byte).await?;
            socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 42\r\nConnection: close\r\n\r\n{\"contract_version\":1,\"status\":\"accepted\"}").await?;
            let _ = finish.await;
            drop(socket);
            Ok::<_, std::io::Error>(())
        };
        let deliver = async {
            let result = fast_worker.deliver_once().await;
            let _ = finished.send(());
            result
        };
        let (outcome, response) = tokio::join!(deliver, respond);
        response?;
        assert_eq!(
            outcome?,
            DeliveryOutcome::Done(analyte_contract::DeliveryStatus::Accepted)
        );
        // Fast completion is observed while the slow socket still has no reply.
        drop(stalled);
        Ok::<_, Box<dyn std::error::Error>>(())
    };
    tokio::time::timeout(HANG_GUARD, async {
        tokio::select! {
            result = &mut slow_attempt => panic!("stalled destination completed first: {result:?}"),
            result = fast_flow => result,
        }
    })
    .await??;
    drop(slow_attempt);
    let rows = store.request(until()?, |store| store.deliveries(0)).await?;
    assert!(
        rows.iter().any(|row| Some(&row.id) == ids.first()
            && row.state == analyte_store::DeliveryState::InFlight)
    );
    assert!(
        rows.iter()
            .any(|row| Some(&row.id) == ids.get(1)
                && row.state == analyte_store::DeliveryState::Done)
    );
    store.shutdown(until()?).await?;
    Ok(())
}

// Owner request 2026-11-19 (results waiting after a removal); AGENTS.md safety
// rule 2: a delivery-only worker for a destination no machine uses ends at once
// when nothing is waiting, but an item held for an operator (here paused) keeps
// it running, so a later release is still sent; shutdown still stops it.
#[tokio::test]
async fn delivery_only_worker_ends_when_nothing_waits() -> TestResult {
    let directory = tempfile::tempdir()?;
    let store = store(&directory.path().join("store.db")).await?;
    let secrets = Secrets("key");
    let destination = destination("http://127.0.0.1:1")?;
    let worker = Worker::for_destination(&store, &destination, &secrets);
    let (stop, stopped) = watch::channel(false);
    tokio::time::timeout(
        HANG_GUARD,
        worker.run_until_delivered(stopped.clone(), Duration::from_secs(1)),
    )
    .await??;
    let metadata = receipt()?;
    let result = make_result(metadata.id, "held")?;
    let delivery = encode_delivery(
        &result,
        &metadata,
        Identifier::new("app")?,
        jiff::Timestamp::now(),
    )?;
    let id = delivery.id;
    enqueue(&store, metadata, result, delivery).await?;
    store
        .request::<_, TestStoreError>(until()?, move |s| {
            let app = Identifier::new("app").map_err(TestStoreError::Text)?;
            let claim = s
                .claim_destination(jiff::Timestamp::now().as_second(), &app)?
                .ok_or(TestStoreError::Text(TextError::Empty))?;
            assert_eq!(claim.id, id);
            s.attempted(
                id,
                &analyte_store::Attempt::Paused(
                    Reason::new("synthetic pause").map_err(TestStoreError::Text)?,
                ),
                jiff::Timestamp::now().as_second(),
            )?;
            Ok::<_, TestStoreError>(())
        })
        .await?;
    let run = worker.run_until_delivered(stopped, Duration::from_secs(1));
    tokio::pin!(run);
    assert!(
        tokio::time::timeout(Duration::from_millis(300), &mut run)
            .await
            .is_err(),
        "a paused item keeps the worker running"
    );
    stop.send(true)?;
    tokio::time::timeout(HANG_GUARD, run).await??;
    Ok(())
}
