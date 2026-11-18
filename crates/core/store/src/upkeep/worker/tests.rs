//! Tests bounded storage worker requests and shutdown.
use crate::test_support::*;
use crate::*;

// Safety rule 7 (ADR 0003)
#[test]
fn worker_roundtrip_expiry_and_shutdown_join() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("store.db");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    let id = runtime.block_on(async {
        let deadline = Instant::now()
            .checked_add(HANG_GUARD)
            .ok_or(MaintenanceError::DeadlineOverflow)?;
        let mut handle = Handle::start(
            path.clone(),
            limits(),
            Box::new(Space(u64::MAX)),
            1,
            deadline,
        )
        .await?;
        let metadata = receipt(b"raw")?;
        let id = metadata.id;
        handle
            .request(deadline, move |store| {
                store.receive(&metadata, b"raw", b"config")
            })
            .await?;
        assert_eq!(
            handle.request(deadline, move |store| store.raw(id)).await?,
            Some(b"raw".to_vec())
        );
        let expired = Instant::now();
        let error = handle.request(expired, move |store| store.raw(id)).await;
        assert!(matches!(
            error,
            Err(IntakeError::Worker(WorkerError::AdmissionTimeout {
                cause: None
            }))
        ));
        handle.shutdown(deadline).await?;
        assert!(matches!(
            handle.request(deadline, move |store| store.raw(id)).await,
            Err(IntakeError::Worker(WorkerError::Closed))
        ));
        Ok::<_, Box<dyn std::error::Error>>(id)
    })?;
    assert_eq!(open(&path)?.raw(id)?, Some(b"raw".to_vec()));
    Ok(())
}

// Plan §11
#[test]
fn worker_open_reports_newer_schema() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("newer.db");
    Connection::open(&path)?.execute_batch("PRAGMA user_version=99")?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    let deadline = Instant::now()
        .checked_add(HANG_GUARD)
        .ok_or(MaintenanceError::DeadlineOverflow)?;
    let result = runtime.block_on(Handle::start(
        path,
        limits(),
        Box::new(Space(u64::MAX)),
        1,
        deadline,
    ));
    assert!(matches!(
        result,
        Err(WorkerStartError::Open(MaintenanceError::SchemaTooNew(99)))
    ));
    Ok(())
}

// Safety rule 7 (ADR 0003); mutation-testing regression
#[test]
fn worker_capacity_bounds_are_inclusive() -> TestResult {
    let temp = tempfile::tempdir()?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    runtime.block_on(async {
        for capacity in [0, 1024, 1025] {
            let deadline = Instant::now() + HANG_GUARD;
            let result = Handle::start(
                temp.path().join(format!("capacity-{capacity}.db")),
                limits(),
                Box::new(Space(u64::MAX)),
                capacity,
                deadline,
            )
            .await;
            if capacity == 1024 {
                let mut handle = result?;
                handle.shutdown(deadline).await?;
            } else {
                assert!(matches!(result, Err(WorkerStartError::QueueCapacity { capacity: rejected }) if rejected == capacity));
            }
        }
        Ok::<_, Box<dyn std::error::Error>>(())
    })?;
    Ok(())
}

// Safety rule 7 (ADR 0003); mutation-testing regression
#[test]
fn worker_drop_joins_store_owner() -> TestResult {
    struct DropSpace(std::sync::mpsc::Sender<()>);
    impl FreeSpace for DropSpace {
        fn available(&self, _: &Path) -> std::io::Result<u64> {
            Ok(u64::MAX)
        }
    }
    impl Drop for DropSpace {
        fn drop(&mut self) {
            let _ = self.0.send(());
        }
    }
    let temp = tempfile::tempdir()?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    for attempt in 0..16 {
        let (closed, observed) = std::sync::mpsc::channel();
        let handle = runtime.block_on(Handle::start(
            temp.path().join(format!("drop-{attempt}.db")),
            limits(),
            Box::new(DropSpace(closed)),
            1,
            Instant::now() + HANG_GUARD,
        ))?;
        drop(handle);
        // Synchronous owners retain the joined-before-return guarantee.
        assert_eq!(observed.try_recv(), Ok(()));
    }
    Ok(())
}

mod mutation_tests {

    use super::*;
    use crate::test_support::{HANG_GUARD, Space, TestResult, limits};

    // Safety rule 7 (ADR 0003); mutation-testing regression
    #[test]
    fn finished_owner_shutdown_needs_no_more_time() -> TestResult {
        let temp = tempfile::tempdir()?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()?;
        runtime.block_on(async {
            let mut handle = Handle::start(
                temp.path().join("store.db"),
                limits(),
                Box::new(Space(u64::MAX)),
                1,
                Instant::now() + HANG_GUARD,
            )
            .await?;
            handle.sender.take();
            let guard = Instant::now() + HANG_GUARD;
            while !handle
                .thread
                .as_ref()
                .ok_or(WorkerError::Closed)?
                .is_finished()
            {
                assert!(Instant::now() < guard, "closed worker did not finish");
                tokio::task::yield_now().await;
            }
            handle.shutdown(Instant::now()).await?;
            assert!(handle.thread.is_none());
            Ok::<_, Box<dyn std::error::Error>>(())
        })?;
        Ok(())
    }
}

// ADR 0003 rule 7
#[test]
fn request_at_deadline_is_refused() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("deadline.db"))?;
    let deadline = Instant::now();
    for (now, expected) in [
        (deadline - Duration::from_nanos(1), true),
        (deadline, false),
        (deadline + Duration::from_nanos(1), false),
    ] {
        let (sent, received) = std::sync::mpsc::channel();
        let request = super::Request {
            deadline,
            operation: Box::new(move |_| {
                let _ = sent.send(());
            }),
        };
        request.run(&mut store, now);
        assert_eq!(received.try_recv().is_ok(), expected);
    }
    Ok(())
}

// ADR 0003 rule 7
#[test]
fn startup_guard_cancels_before_owner_join() -> TestResult {
    let mut startup = startup::Startup::new(Instant::now() + HANG_GUARD);
    let fixed_now = Instant::now();
    startup.clock = Some(std::sync::Arc::new(move || fixed_now));
    let (released, release) = std::sync::mpsc::channel();
    let opening = startup.clone();
    let thread = std::thread::spawn(move || {
        let observed = release.recv_timeout(HANG_GUARD);
        assert!(observed.is_ok());
        assert!(opening.expired());
    });
    let (_finished, completion) = tokio::sync::oneshot::channel();
    let handle = Handle {
        sender: None,
        thread: Some(thread),
        finished: completion,
    };
    assert!(!startup.expired());
    {
        let _cancel = super::CancelStartup(startup.clone());
        // Leaving the scope is the same guard destruction used by future cancellation.
    }
    // Require cancellation before the owner is allowed to complete, not after a join.
    let cancelled_before_release = startup.expired();
    released.send(())?;
    assert!(cancelled_before_release);
    drop(handle);
    Ok(())
}

// Safety rule 7 (ADR 0003)
#[test]
fn shutdown_timeout_keeps_owner_and_can_resume() -> TestResult {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    let (release, released) = std::sync::mpsc::channel();
    let (finished, completion) = tokio::sync::oneshot::channel();
    let thread = std::thread::spawn(move || {
        let _finished = super::Finished(Some(finished));
        released
            .recv_timeout(HANG_GUARD)
            .expect("test releases worker");
    });
    let mut handle = Handle {
        sender: None,
        thread: Some(thread),
        finished: completion,
    };
    runtime.block_on(async {
        assert!(matches!(
            handle.shutdown(Instant::now()).await,
            Err(WorkerError::ShutdownTimeout(_))
        ));
        assert!(handle.thread.is_some());
        release.send(()).map_err(|_| WorkerError::Closed)?;
        handle.shutdown(Instant::now() + HANG_GUARD).await?;
        assert!(handle.thread.is_none());
        Ok::<_, Box<dyn std::error::Error>>(())
    })?;
    Ok(())
}

// Safety rule 7 (ADR 0003)
#[test]
fn async_drop_reports_missing_close_without_waiting() -> TestResult {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    let (release, released) = std::sync::mpsc::channel();
    let (finished, completion) = tokio::sync::oneshot::channel();
    let thread = std::thread::spawn(move || {
        let _finished = super::Finished(Some(finished));
        released
            .recv_timeout(HANG_GUARD)
            .expect("test releases owner after Drop");
    });
    let (sender, _receiver) = tokio::sync::mpsc::channel(1);
    let handle = Handle {
        sender: Some(sender),
        thread: Some(thread),
        finished: completion,
    };
    runtime.block_on(async {
        let error = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(handle)));
        assert!(
            error.is_err(),
            "test build must diagnose skipped explicit close"
        );
        // A joining Drop cannot reach this release; the worker is still blocked.
        release.send(())?;
        Ok::<_, Box<dyn std::error::Error>>(())
    })?;
    Ok(())
}

// Safety rule 7 (ADR 0003)
#[tokio::test]
async fn bad_queue_capacity_refused_before_opening() -> TestResult {
    let temp = tempfile::tempdir()?;
    for capacity in [0, 1025, usize::MAX] {
        let path = temp.path().join("must-not-open.db");
        let result = Handle::start(
            path.clone(),
            limits(),
            Box::new(Space(u64::MAX)),
            capacity,
            Instant::now(),
        )
        .await;
        assert!(
            matches!(result, Err(WorkerStartError::QueueCapacity { capacity: rejected }) if rejected == capacity)
        );
        assert!(!path.exists());
    }
    Ok(())
}

// ADR 0003 rule 1
#[tokio::test]
async fn response_timeout_keeps_cause_and_may_commit() -> TestResult {
    use std::sync::Arc;
    let temp = tempfile::tempdir()?;
    let mut until = Instant::now() + HANG_GUARD;
    let handle = Arc::new(
        Handle::start(
            temp.path().join("uncertain.db"),
            limits(),
            Box::new(Space(u64::MAX)),
            1,
            until,
        )
        .await?,
    );
    let metadata = receipt(b"uncertain raw")?;
    let id = metadata.id;
    let (entered, waiting) = tokio::sync::oneshot::channel();
    let (release, released) = std::sync::mpsc::channel();
    let owner = handle.clone();
    let request = tokio::spawn(async move {
        owner
            .request::<_, IntakeError>(Instant::now() + Duration::from_secs(1), move |store| {
                let _ = entered.send(());
                released
                    .recv_timeout(HANG_GUARD)
                    .map_err(|cause| StorageError::Io(std::io::Error::other(cause)))?;
                store.receive(&metadata, b"uncertain raw", b"config")
            })
            .await
    });
    waiting.await?;
    let result = request.await?;
    assert!(matches!(
        result,
        Err(IntakeError::Worker(WorkerError::ResponseTimeout(_)))
    ));
    release.send(())?;
    until = Instant::now() + HANG_GUARD;
    assert_eq!(
        handle.request(until, move |store| store.raw(id)).await?,
        Some(b"uncertain raw".to_vec())
    );
    let busy = handle
        .request::<(), DeliveryError>(until, |_| {
            Err(rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(5),
                Some("synthetic lock".into()),
            )
            .into())
        })
        .await;
    assert!(
        matches!(busy, Err(DeliveryError::Storage(StorageError::Busy(rusqlite::Error::SqliteFailure(code, Some(message))))) if code.extended_code == 5 && message == "synthetic lock")
    );
    let mut owner = Arc::try_unwrap(handle).map_err(|_| "request retained its store owner")?;
    owner.shutdown(until).await?;
    Ok(())
}
