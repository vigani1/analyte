//! Tests rule7 active delivery and order poll cancel without waiting for http deadline; mutation rule7 draining closed or set stop does not claim.
use super::*;

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
