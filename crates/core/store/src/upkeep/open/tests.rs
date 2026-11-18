//! Tests database opening and stored settings.
use super::*;
use crate::SCHEMA_VERSION;
use crate::startup::Progress;
use crate::test_support::{Space, TestResult, limits};
use crate::upkeep::migration::STARTING_VERSION;
use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

fn synthetic_store(path: &Path) -> Result<Connection, Error> {
    drop(Store::open(path, limits(), Box::new(Space(u64::MAX)))?);
    let db = Connection::open(path)?;
    db.execute("INSERT INTO transfers VALUES ('synthetic','open',NULL)", [])?;
    Ok(db)
}

fn injected_clock(startup: &mut Startup, target: StartupStep, base: Instant) {
    let progress = startup.progress();
    let ticks = AtomicU64::new(0);
    startup.clock = Some(Arc::new(move || {
        let elapsed = if *progress.borrow() == target {
            ticks.fetch_add(1, Ordering::Relaxed)
        } else {
            0
        };
        base.checked_add(Duration::from_millis(elapsed))
            .expect("bounded synthetic clock")
    }));
}

// ADR 0003 rule 7
#[test]
fn integrity_timeout_is_named_larger_budget_opens() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("integrity.db");
    drop(Store::open(&path, limits(), Box::new(Space(u64::MAX)))?);
    for (budget, succeeds) in [
        (Duration::from_millis(5), false),
        (Duration::from_secs(1000), true),
    ] {
        let base = Instant::now();
        let mut startup = Startup::new(base.checked_add(budget).ok_or("deadline")?);
        injected_clock(&mut startup, StartupStep::Integrity, base);
        let result = Store::open_with_startup(&path, limits(), Box::new(Space(u64::MAX)), &startup);
        if succeeds {
            drop(result?);
        } else {
            assert!(matches!(
                result,
                Err(Error::StartupTimeout {
                    step: StartupStep::Integrity
                })
            ));
        }
    }
    Ok(())
}

// ADR 0003 rule 7
#[test]
fn schema_creation_timeout_leaves_empty_file() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("create.db");
    let creating = StartupStep::Migration {
        from: 0,
        to: STARTING_VERSION,
    };
    let base = Instant::now();
    let mut startup = Startup::new(
        base.checked_add(Duration::from_millis(500))
            .ok_or("deadline")?,
    );
    injected_clock(&mut startup, creating, base);
    assert!(matches!(
        Store::open_with_startup(&path, limits(), Box::new(Space(u64::MAX)), &startup),
        Err(Error::StartupTimeout { step }) if step == creating
    ));
    let db = Connection::open(&path)?;
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))?,
        0
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM sqlite_schema", [], |r| r
            .get::<_, i64>(0))?,
        0
    );
    drop(db);
    let store = Store::open(&path, limits(), Box::new(Space(u64::MAX)))?;
    assert_eq!(
        store
            .db
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))?,
        SCHEMA_VERSION
    );
    Ok(())
}

// Plan §11
#[test]
fn empty_file_gets_schema_other_versions_refused() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("store.db");
    let store = Store::open(&path, limits(), Box::new(Space(u64::MAX)))?;
    for (pragma, expected) in [
        ("user_version", SCHEMA_VERSION.to_string()),
        ("synchronous", "2".into()),
        ("journal_mode", "wal".into()),
        ("foreign_keys", "1".into()),
        ("auto_vacuum", "2".into()),
    ] {
        let value: String = store.db.query_row(
            &format!("SELECT CAST({pragma} AS TEXT) FROM pragma_{pragma}"),
            [],
            |r| r.get(0),
        )?;
        assert_eq!(value, expected, "{pragma}");
    }
    drop(store);
    for (version, refused) in [
        (SCHEMA_VERSION + 1, "newer"),
        (STARTING_VERSION - 1, "older"),
        (1, "older"),
        (-1, "corrupt"),
    ] {
        Connection::open(&path)?.pragma_update(None, "user_version", version)?;
        let result = Store::open(&path, limits(), Box::new(Space(u64::MAX)));
        match refused {
            "newer" => assert!(matches!(result, Err(Error::SchemaTooNew(v)) if v == version)),
            "older" => assert!(matches!(result, Err(Error::SchemaTooOld(v)) if v == version)),
            _ => assert!(matches!(result, Err(Error::Corruption))),
        }
        let db = Connection::open(&path)?;
        assert_eq!(
            db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))?,
            version,
            "a refused store is left unchanged"
        );
    }
    Ok(())
}

#[test]
#[ignore = "manual benchmark: opens a synthetic store larger than 2 GiB"]
fn startup_timing_two_gib_synthetic_store() -> TestResult {
    let _ = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_target(false)
        .try_init();
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("synthetic-large.db");
    let mut db = synthetic_store(&path)?;
    let started = Instant::now();
    let tx = db.transaction()?;
    for _ in 0..2048 {
        tx.execute(
            "INSERT INTO frames(transfer,number,raw) VALUES ('synthetic',1,zeroblob(1048576))",
            [],
        )?;
    }
    tx.commit()?;
    let bytes = std::fs::metadata(&path)?.len();
    assert!(bytes >= 2_147_483_648);
    println!(
        "synthetic_bytes={bytes} generation_seconds={:.3}",
        started.elapsed().as_secs_f64()
    );
    for run in 1..=3 {
        let started = Instant::now();
        Store::integrity(&db)?;
        println!(
            "integrity_run={run} seconds={:.3}",
            started.elapsed().as_secs_f64()
        );
    }
    drop(db);
    for run in 1..=3 {
        let started = Instant::now();
        drop(Store::open(
            &path,
            {
                let mut policy = limits();
                policy.database_bytes = 16_000_000_000;
                policy
            },
            Box::new(Space(u64::MAX)),
        )?);
        println!(
            "current_schema_startup_run={run} seconds={:.3}",
            started.elapsed().as_secs_f64()
        );
    }
    Ok(())
}

// ADR 0003 rule 7
#[test]
fn worker_reports_opening_deadline_not_generic_timeout() -> TestResult {
    let temp = tempfile::tempdir()?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    let result = runtime.block_on(crate::Handle::start(
        temp.path().join("expired.db"),
        limits(),
        Box::new(Space(u64::MAX)),
        1,
        Instant::now(),
    ));
    assert!(matches!(
        result,
        Err(crate::WorkerStartError::Open(
            crate::MaintenanceError::StartupTimeout {
                step: StartupStep::Opening
            }
        ))
    ));
    Ok(())
}

// ADR 0003 rule 7
#[test]
fn completed_startup_does_not_expire_later_operations() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("completed.db");
    let base = Instant::now();
    let expired = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let clock = expired.clone();
    let mut startup = Startup::new(
        base.checked_add(Duration::from_secs(60))
            .ok_or("deadline")?,
    );
    startup.clock = Some(Arc::new(move || {
        if clock.load(Ordering::Acquire) {
            base.checked_add(Duration::from_secs(120))
                .expect("bounded synthetic clock")
        } else {
            base
        }
    }));
    let store = Store::open_with_startup(&path, limits(), Box::new(Space(u64::MAX)), &startup)?;
    expired.store(true, Ordering::Release);
    assert_eq!(
        store
            .db
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))?,
        SCHEMA_VERSION
    );
    Ok(())
}

// ADR 0003; mutation-testing regression
#[test]
fn startup_interrupts_sqlite_once_then_allows_cleanup() -> TestResult {
    let base = Instant::now();
    let mut startup = Startup::new(base + crate::test_support::HANG_GUARD);
    startup.clock = Some(Arc::new(move || base));
    let mut db = Connection::open_in_memory()?;
    startup.install(&mut db)?;
    startup.cancel();
    let first = db.query_row("SELECT 1", [], |r| r.get::<_, i64>(0));
    assert_eq!(
        first
            .err()
            .ok_or("cancelled query succeeded")?
            .sqlite_error_code(),
        Some(rusqlite::ErrorCode::OperationInterrupted)
    );
    assert_eq!(db.query_row("SELECT 2", [], |r| r.get::<_, i64>(0))?, 2);
    Ok(())
}

// Plan §11; mutation-testing regression
#[test]
fn startup_progress_shows_names_and_completed_steps() -> TestResult {
    use crate::startup::StartupStep;
    for (step, expected) in [
        (StartupStep::Opening, "opening"),
        (StartupStep::Integrity, "integrity check"),
        (
            StartupStep::Migration { from: 7, to: 8 },
            "migration 7 to 8",
        ),
        (StartupStep::Finalizing, "finalizing"),
    ] {
        assert_eq!(step.to_string(), expected);
    }

    let events = Arc::new(std::sync::Mutex::new(Vec::new()));
    let observed = events.clone();
    let elapsed = Arc::new(AtomicU64::new(0));
    let clock = elapsed.clone();
    let base = Instant::now();
    let mut startup = Startup::new(base + crate::test_support::HANG_GUARD);
    startup.clock = Some(Arc::new(move || {
        base.checked_add(Duration::from_millis(clock.load(Ordering::SeqCst)))
            .expect("test clock stays within 26 milliseconds")
    }));
    startup.observer = Some(Arc::new(move |event| {
        observed
            .lock()
            .expect("progress collector is not poisoned")
            .push(event);
    }));
    for (step, finished_at) in [
        (StartupStep::Opening, 7),
        (StartupStep::Integrity, 10),
        (StartupStep::Migration { from: 7, to: 8 }, 21),
        (StartupStep::Finalizing, 26),
    ] {
        startup.run(step, || {
            elapsed.store(finished_at, Ordering::SeqCst);
            Ok(())
        })?;
    }
    assert_eq!(
        *events.lock().map_err(|_| "progress collector poisoned")?,
        vec![
            Progress::Started(StartupStep::Opening),
            Progress::Finished {
                step: StartupStep::Opening,
                elapsed_ms: 7,
                succeeded: true
            },
            Progress::Started(StartupStep::Migration { from: 7, to: 8 }),
            Progress::Finished {
                step: StartupStep::Migration { from: 7, to: 8 },
                elapsed_ms: 11,
                succeeded: true
            },
            Progress::Started(StartupStep::Finalizing),
            Progress::Finished {
                step: StartupStep::Finalizing,
                elapsed_ms: 5,
                succeeded: true
            },
        ],
        "every visible startup step must report its start and timed completion; integrity is intentionally silent"
    );
    Ok(())
}

// ADR 0003; mutation-testing regression
#[test]
fn foreign_key_corruption_refuses_without_rewriting_raw() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("broken-custody.db");
    drop(Store::open(&path, limits(), Box::new(Space(u64::MAX)))?);
    let db = Connection::open(&path)?;
    db.execute_batch("PRAGMA foreign_keys=OFF")?;
    db.execute(
        "INSERT INTO frames(transfer,number,raw) VALUES ('missing-transfer',0,x'010203')",
        [],
    )?;
    drop(db);
    assert!(matches!(
        Store::open(&path, limits(), Box::new(Space(u64::MAX))),
        Err(Error::Corruption)
    ));
    let db = Connection::open(&path)?;
    assert_eq!(
        db.query_row("SELECT raw FROM frames", [], |r| r.get::<_, Vec<u8>>(0))?,
        vec![1, 2, 3]
    );
    Ok(())
}
