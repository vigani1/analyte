//! Tests bounded storage worker requests and shutdown.
use crate::test_support::*;
use crate::*;

#[test]
fn rule7_worker_roundtrip_expired_requests_and_shutdown_joins() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("store.db");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    runtime.block_on(async {
        let deadline = Instant::now()
            .checked_add(HANG_GUARD)
            .ok_or(Error::Timeout)?;
        let mut handle = Handle::start(
            path.clone(),
            limits(),
            Box::new(Space(u64::MAX)),
            1,
            deadline,
        )
        .await?;
        let metadata = receipt(b"raw").map_err(|_| Error::InvalidInput)?;
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
        assert!(matches!(error, Err(Error::Timeout) | Err(Error::Closed)));
        handle.shutdown(deadline).await?;
        assert!(matches!(
            handle.request(deadline, move |store| store.raw(id)).await,
            Err(Error::Closed)
        ));
        Ok::<_, Error>(())
    })?;
    assert_eq!(open(&path)?.raw(receipt(b"unused")?.id)?, None);
    Ok(())
}

#[test]
fn plan11_worker_open_reports_newer_schema_specifically() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("newer.db");
    Connection::open(&path)?.execute_batch("PRAGMA user_version=99")?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    let deadline = Instant::now()
        .checked_add(HANG_GUARD)
        .ok_or(Error::Timeout)?;
    let result = runtime.block_on(Handle::start(
        path,
        limits(),
        Box::new(Space(u64::MAX)),
        1,
        deadline,
    ));
    assert!(matches!(result, Err(Error::SchemaTooNew(99))));
    Ok(())
}

#[test]
fn mutation_rule7_worker_capacity_bounds_are_inclusive() -> TestResult {
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
                assert!(matches!(result, Err(Error::InvalidInput)));
            }
        }
        Ok::<_, Error>(())
    })?;
    Ok(())
}

#[test]
fn mutation_rule7_worker_drop_joins_the_store_owner_before_returning() -> TestResult {
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
    runtime.block_on(async {
        for attempt in 0..16 {
            let (closed, observed) = std::sync::mpsc::channel();
            let handle = Handle::start(
                temp.path().join(format!("drop-{attempt}.db")),
                limits(),
                Box::new(DropSpace(closed)),
                1,
                Instant::now() + HANG_GUARD,
            )
            .await?;
            drop(handle);
            // Drop promises the owner has already finished, not eventual cleanup.
            assert_eq!(observed.try_recv(), Ok(()));
        }
        Ok::<_, Error>(())
    })?;
    Ok(())
}

mod mutation_tests {

    use super::*;
    use crate::test_support::{HANG_GUARD, Space, TestResult, limits};

    #[test]
    fn mutation_rule7_worker_finished_owner_shutdown_does_not_require_more_time() -> TestResult {
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
            while !handle.thread.as_ref().ok_or(Error::Closed)?.is_finished() {
                assert!(Instant::now() < guard, "closed worker did not finish");
                tokio::task::yield_now().await;
            }
            handle.shutdown(Instant::now()).await?;
            assert!(handle.thread.is_none());
            Ok::<_, Error>(())
        })?;
        Ok(())
    }
}

#[test]
fn adr0003_rule7_worker_deadline_equality_refuses_execution() -> TestResult {
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

#[test]
fn adr0003_rule7_worker_startup_guard_cancels_before_owner_join() -> TestResult {
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
    let handle = Handle {
        sender: None,
        thread: Some(thread),
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
