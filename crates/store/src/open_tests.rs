//! ADR 0003 rule 7: bounded startup, with atomic, resumable migrations.
use super::*;
use crate::tests::{Space, TestResult, limits};
use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};

fn version_seven(path: &Path) -> Result<Connection, Error> {
    let mut db = Connection::open(path)?;
    db.execute_batch(include_str!("schema.sql"))?;
    Store::migrate_v2(&mut db)?;
    Store::migrate_v3(&mut db, 65_536)?;
    db.execute_batch(include_str!("migration_v4.sql"))?;
    db.execute_batch(include_str!("migration_v5.sql"))?;
    db.execute_batch(include_str!("migration_v6.sql"))?;
    Store::migrate_v7(&mut db, 65_536)?;
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

#[test]
fn adr0003_rule7_startup_timeout_names_integrity_and_larger_budget_succeeds() -> TestResult {
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

#[test]
fn adr0003_rule7_timeout_during_migration_rolls_back_version_schema_and_raw_frames() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("migration.db");
    let mut db = version_seven(&path)?;
    let tx = db.transaction()?;
    for _ in 0..1000 {
        tx.execute(
            "INSERT INTO frames(transfer,number,raw) VALUES ('synthetic',1,x'01020304')",
            [],
        )?;
    }
    tx.commit()?;
    let schema: String = db.query_row(
        "SELECT sql FROM sqlite_schema WHERE name='frames'",
        [],
        |r| r.get(0),
    )?;
    drop(db);
    let base = Instant::now();
    let mut startup = Startup::new(
        base.checked_add(Duration::from_millis(500))
            .ok_or("deadline")?,
    );
    injected_clock(
        &mut startup,
        StartupStep::Migration { from: 7, to: 8 },
        base,
    );
    assert!(matches!(
        Store::open_with_startup(&path, limits(), Box::new(Space(u64::MAX)), &startup),
        Err(Error::StartupTimeout {
            step: StartupStep::Migration { from: 7, to: 8 }
        })
    ));
    let db = Connection::open(&path)?;
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))?,
        7
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM frames WHERE raw=x'01020304'",
            [],
            |r| r.get::<_, i64>(0)
        )?,
        1000
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM sqlite_schema WHERE name='frames_v8'",
            [],
            |r| r.get::<_, i64>(0)
        )?,
        0
    );
    assert_eq!(
        db.query_row(
            "SELECT sql FROM sqlite_schema WHERE name='frames'",
            [],
            |r| r.get::<_, String>(0)
        )?,
        schema
    );
    assert_eq!(
        db.query_row(
            "SELECT seq FROM sqlite_sequence WHERE name='frames'",
            [],
            |r| r.get::<_, i64>(0)
        )?,
        1000
    );
    Store::integrity(&db)?;
    drop(db);
    let store = Store::open(&path, limits(), Box::new(Space(u64::MAX)))?;
    assert_eq!(
        store
            .db
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))?,
        SCHEMA_VERSION
    );
    assert_eq!(
        store.db.query_row(
            "SELECT count(*) FROM frames WHERE raw=x'01020304'",
            [],
            |r| r.get::<_, i64>(0)
        )?,
        1000
    );
    Ok(())
}

#[test]
#[ignore = "manual benchmark: creates and migrates a synthetic store larger than 2 GiB"]
fn startup_timing_two_gib_synthetic_store() -> TestResult {
    let _ = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_target(false)
        .try_init();
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("synthetic-large.db");
    let mut db = version_seven(&path)?;
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
    let started = Instant::now();
    let store = Store::open(
        &path,
        Limits {
            database_bytes: 16_000_000_000,
            ..limits()
        },
        Box::new(Space(u64::MAX)),
    )?;
    println!(
        "startup_including_v7_to_v9_seconds={:.3}",
        started.elapsed().as_secs_f64()
    );
    assert_eq!(
        store
            .db
            .query_row("SELECT sum(length(raw)) FROM frames", [], |r| r
                .get::<_, i64>(0))?,
        2_147_483_648
    );
    drop(store);
    for run in 1..=3 {
        let started = Instant::now();
        drop(Store::open(
            &path,
            Limits {
                database_bytes: 16_000_000_000,
                ..limits()
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

#[test]
fn adr0003_rule7_worker_reports_opening_deadline_instead_of_generic_timeout() -> TestResult {
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
        Err(Error::StartupTimeout {
            step: StartupStep::Opening
        })
    ));
    Ok(())
}

#[test]
fn adr0003_rule7_completed_startup_does_not_expire_later_store_operations() -> TestResult {
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
