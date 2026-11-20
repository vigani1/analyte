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

/// Counts for seconds: far longer than any deadline below, so the read ends in
/// time only when it is stopped. The bound keeps a broken stop from spinning
/// for ever.
const LONG_READ: &str = "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM c WHERE x<50000000) SELECT count(*) FROM c";

/// Enough work for the read guard's progress check to run many times.
const BUSY: &str = "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM c WHERE x<200000) SELECT count(*) FROM c";

type Shared = std::sync::Arc<Handle>;

/// The hang guard from now; never part of the behaviour under test.
fn guarded() -> Result<Instant, MaintenanceError> {
    Instant::now()
        .checked_add(HANG_GUARD)
        .ok_or(MaintenanceError::DeadlineOverflow)
}

async fn started(path: &Path) -> Result<Shared, Box<dyn std::error::Error>> {
    let handle = Handle::start(
        path.to_owned(),
        limits(),
        Box::new(Space(u64::MAX)),
        8,
        guarded()?,
    )
    .await?;
    Ok(std::sync::Arc::new(handle))
}

async fn stop(handle: Shared) -> TestResult {
    let mut owner = std::sync::Arc::try_unwrap(handle).map_err(|_| "a task kept the handle")?;
    owner.shutdown(guarded()?).await?;
    Ok(())
}

/// The worker held inside one request, so that what is queued behind it waits.
struct Held {
    release: std::sync::mpsc::Sender<()>,
    task: tokio::task::JoinHandle<Result<(), StorageError>>,
}

impl Held {
    async fn new(handle: &Shared) -> Result<Self, Box<dyn std::error::Error>> {
        let (entered, waiting) = tokio::sync::oneshot::channel();
        let (release, released) = std::sync::mpsc::channel::<()>();
        let owner = handle.clone();
        let until = guarded()?;
        let task = tokio::spawn(async move {
            owner
                .request::<_, StorageError>(until, move |_| {
                    let _ = entered.send(());
                    let _ = released.recv_timeout(HANG_GUARD);
                    Ok(())
                })
                .await
        });
        waiting.await?;
        Ok(Self { release, task })
    }

    /// Lets the worker go on, once the holding request has ended.
    async fn release(self) -> TestResult {
        self.release.send(())?;
        self.task.await??;
        Ok(())
    }
}

/// Starts [`LONG_READ`] due by `deadline`, and returns once it runs.
async fn long_read(
    handle: &Shared,
    deadline: Instant,
) -> Result<tokio::task::JoinHandle<Result<i64, StorageError>>, Box<dyn std::error::Error>> {
    let reader = handle.clone();
    let (entered, running) = tokio::sync::oneshot::channel();
    let read = tokio::spawn(async move {
        reader
            .read::<i64, StorageError>(deadline, move |store| {
                let _ = entered.send(());
                Ok(store.db.query_row(LONG_READ, [], |r| r.get(0))?)
            })
            .await
    });
    running.await?;
    Ok(read)
}

/// A read that is queued and must never run; true once it did.
fn marked() -> (
    std::sync::Arc<std::sync::atomic::AtomicBool>,
    impl FnOnce(&Store) -> Result<(), StorageError> + Send + 'static,
) {
    let ran = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let marker = ran.clone();
    (ran, move |_: &Store| {
        marker.store(true, std::sync::atomic::Ordering::Release);
        Ok(())
    })
}

fn stopped<T>(result: &Result<T, StorageError>) -> bool {
    matches!(result, Err(StorageError::Worker(WorkerError::ReadStopped)))
}

// ADR 0020; ADR 0003 rules 1 and 7: an admin read still running at its
// deadline is stopped, and a machine's message queued behind it is stored
// promptly instead of waiting for the read to end.
#[tokio::test]
async fn read_past_its_deadline_stops_and_intake_behind_it_is_prompt() -> TestResult {
    let temp = tempfile::tempdir()?;
    let handle = started(&temp.path().join("stuck-read.db")).await?;
    let budget = Duration::from_millis(150);
    let read = long_read(&handle, Instant::now() + budget).await?;
    let metadata = receipt(b"behind a stuck read")?;
    let id = metadata.id;
    let queued = Instant::now();
    handle
        .request(Instant::now() + HANG_GUARD, move |store| {
            store.receive(&metadata, b"behind a stuck read", b"config")
        })
        .await?;
    let waited = queued.elapsed();
    assert!(stopped(&read.await?));
    assert!(
        waited < budget + Duration::from_secs(1),
        "intake waited {waited:?}"
    );
    assert_eq!(
        handle
            .read(Instant::now() + HANG_GUARD, move |store| store.raw(id))
            .await?,
        Some(b"behind a stuck read".to_vec())
    );
    stop(handle).await
}

// ADR 0020: a read whose deadline passed while it waited in the queue is
// skipped and never runs.
#[tokio::test]
async fn expired_queued_read_never_runs() -> TestResult {
    let temp = tempfile::tempdir()?;
    let handle = started(&temp.path().join("expired-read.db")).await?;
    let held = Held::new(&handle).await?;
    let (ran, work) = marked();
    let result = handle
        .read(Instant::now() + Duration::from_millis(50), work)
        .await;
    assert!(stopped(&result));
    held.release().await?;
    // A request behind it proves the queue has drained past the read.
    handle
        .request::<_, StorageError>(Instant::now() + HANG_GUARD, |_| Ok(()))
        .await?;
    assert!(!ran.load(std::sync::atomic::Ordering::Acquire));
    stop(handle).await
}

// ADR 0020: a page that gives up drops its read. Still queued, the read is
// skipped; already running, it is stopped and the request behind it runs.
#[tokio::test]
async fn dropped_callers_read_is_skipped_or_stopped() -> TestResult {
    let temp = tempfile::tempdir()?;
    let handle = started(&temp.path().join("dropped-read.db")).await?;
    let held = Held::new(&handle).await?;
    let (ran, work) = marked();
    let queued = handle.read(Instant::now() + HANG_GUARD, work);
    assert!(
        tokio::time::timeout(Duration::from_millis(20), queued)
            .await
            .is_err()
    );
    held.release().await?;
    handle
        .request::<_, StorageError>(Instant::now() + HANG_GUARD, |_| Ok(()))
        .await?;
    assert!(!ran.load(std::sync::atomic::Ordering::Acquire));
    let read = long_read(&handle, Instant::now() + HANG_GUARD).await?;
    read.abort();
    let dropped = Instant::now();
    handle
        .request::<_, StorageError>(Instant::now() + HANG_GUARD, |_| Ok(()))
        .await?;
    let waited = dropped.elapsed();
    assert!(waited < Duration::from_secs(1), "waited {waited:?}");
    assert!(read.await.is_err_and(|error| error.is_cancelled()));
    stop(handle).await
}

// ADR 0020; ADR 0003 rule 1: a write is never interrupted, even right after a
// stopped read, and even when its caller's deadline passes and the caller goes
// while its transaction is open: it commits whole.
#[tokio::test]
async fn write_is_never_interrupted_after_its_caller_goes() -> TestResult {
    let temp = tempfile::tempdir()?;
    let handle = started(&temp.path().join("write-whole.db")).await?;
    let read = long_read(&handle, Instant::now() + Duration::from_millis(30)).await?;
    assert!(stopped(&read.await?));
    let writer = handle.clone();
    let (entered, inside) = tokio::sync::oneshot::channel();
    let (release, released) = std::sync::mpsc::channel::<()>();
    let write = tokio::spawn(async move {
        writer
            .request::<i64, StorageError>(
                Instant::now() + Duration::from_millis(30),
                move |store| {
                    let tx = store.db.transaction()?;
                    tx.execute("INSERT INTO machine_activity VALUES('first',1)", [])?;
                    let _ = entered.send(());
                    let _ = released.recv_timeout(HANG_GUARD);
                    let counted = tx.query_row(BUSY, [], |r| r.get(0))?;
                    tx.execute("INSERT INTO machine_activity VALUES('second',2)", [])?;
                    tx.commit()?;
                    Ok(counted)
                },
            )
            .await
    });
    inside.await?;
    assert!(matches!(
        write.await?,
        Err(StorageError::Worker(WorkerError::ResponseTimeout(_)))
    ));
    release.send(())?;
    let rows = handle
        .read::<Vec<String>, StorageError>(Instant::now() + HANG_GUARD, |store| {
            let mut query = store
                .db
                .prepare("SELECT machine FROM machine_activity ORDER BY machine")?;
            let rows = query.query_map([], |r| r.get(0))?;
            Ok(rows.collect::<Result<_, _>>()?)
        })
        .await?;
    assert_eq!(rows, ["first", "second"]);
    stop(handle).await
}

// ADR 0020: after a stopped read the connection reads and writes as before,
// with no transaction left open and nothing left refusing writes.
#[tokio::test]
async fn connection_works_normally_after_a_stopped_read() -> TestResult {
    let temp = tempfile::tempdir()?;
    let handle = started(&temp.path().join("after-stop.db")).await?;
    let read = long_read(&handle, Instant::now() + Duration::from_millis(30)).await?;
    assert!(stopped(&read.await?));
    let until = Instant::now() + HANG_GUARD;
    let (open, query_only) = handle
        .request::<_, StorageError>(until, |store| {
            let open = !store.db.is_autocommit();
            let query_only: bool = store
                .db
                .pragma_query_value(None, "query_only", |r| r.get(0))?;
            Ok((open, query_only))
        })
        .await?;
    assert!(!open && !query_only);
    let counted: i64 = handle
        .read::<_, StorageError>(until, |store| {
            Ok(store.db.query_row(BUSY, [], |r| r.get(0))?)
        })
        .await?;
    assert_eq!(counted, 200_000);
    let metadata = receipt(b"after a stop")?;
    let id = metadata.id;
    handle
        .request(until, move |store| {
            store.receive(&metadata, b"after a stop", b"config")
        })
        .await?;
    assert_eq!(
        handle.read(until, move |store| store.raw(id)).await?,
        Some(b"after a stop".to_vec())
    );
    stop(handle).await
}

// ADR 0020: a read cannot write. Its work gets a shared store, which offers
// no method that writes, and SQLite refuses a write it is asked for directly.
#[tokio::test]
async fn read_cannot_write() -> TestResult {
    let temp = tempfile::tempdir()?;
    let handle = started(&temp.path().join("read-only.db")).await?;
    let until = Instant::now() + HANG_GUARD;
    let refused = handle
        .read::<usize, StorageError>(until, |store| {
            Ok(store
                .db
                .execute("INSERT INTO machine_activity VALUES('read',1)", [])?)
        })
        .await;
    assert!(matches!(
        refused,
        Err(StorageError::Sqlite(rusqlite::Error::SqliteFailure(code, _)))
            if code.code == rusqlite::ErrorCode::ReadOnly
    ));
    let rows: i64 = handle
        .request::<_, StorageError>(until, |store| {
            Ok(store
                .db
                .query_row("SELECT count(*) FROM machine_activity", [], |r| r.get(0))?)
        })
        .await?;
    assert_eq!(rows, 0);
    stop(handle).await
}

// ADR 0020: a transaction a write left open belongs to that write; a read
// that can be stopped is refused rather than run inside it, and the
// transaction is untouched.
#[tokio::test]
async fn read_is_refused_inside_an_open_transaction() -> TestResult {
    let temp = tempfile::tempdir()?;
    let handle = started(&temp.path().join("open-transaction.db")).await?;
    let until = Instant::now() + HANG_GUARD;
    handle
        .request::<_, StorageError>(until, |store| {
            store
                .db
                .execute_batch("BEGIN; INSERT INTO machine_activity VALUES('open',1);")?;
            Ok(())
        })
        .await?;
    let refused = handle
        .read::<i64, StorageError>(until, |store| {
            Ok(store.db.query_row(BUSY, [], |r| r.get(0))?)
        })
        .await;
    assert!(matches!(
        refused,
        Err(StorageError::Worker(WorkerError::ReadInsideTransaction))
    ));
    let (was_open, rows) = handle
        .request::<_, StorageError>(until, |store| {
            let was_open = !store.db.is_autocommit();
            store.db.execute_batch("COMMIT")?;
            let rows: i64 =
                store
                    .db
                    .query_row("SELECT count(*) FROM machine_activity", [], |r| r.get(0))?;
            Ok((was_open, rows))
        })
        .await?;
    assert!(was_open);
    assert_eq!(rows, 1);
    stop(handle).await
}
