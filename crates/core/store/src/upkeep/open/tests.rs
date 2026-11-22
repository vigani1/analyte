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

/// A version-15 store, as the release before schema 16 wrote it, holding
/// parked items of each kind the step treats differently.
fn store_at_fifteen(path: &Path) -> Result<(), Error> {
    let db = Connection::open(path)?;
    db.execute_batch(include_str!("../schema.sql"))?;
    db.execute_batch(concat!(
        "INSERT INTO outbox(id,kind,order_id,destination,ordering_key,payload,digest,contract,state,next_attempt) VALUES ",
        "('\"d-rejected\"','order_event','o1','main-lab','o1',x'00',x'00',1,'parked',0),",
        "('\"d-conflict\"','order_event','o2','main-lab','o2',x'00',x'00',1,'conflict',0),",
        "('\"d-silent\"','order_event','o3','main-lab','o3',x'00',x'00',1,'parked',0);",
        "INSERT INTO attempts(delivery,outcome,time) VALUES ",
        "('\"d-rejected\"','\"Done\"',100),",
        "('\"d-rejected\"','{\"Parked\":\"HTTP 422\"}',200),",
        "('\"d-rejected\"','{\"RetryAt\":350}',300),",
        "('\"d-rejected\"','{\"Parked\":\"HTTP 422\"}',400),",
        "('\"d-conflict\"','{\"Conflict\":\"HTTP 409\"}',500),",
        "('\"d-silent\"','{\"Paused\":\"keys\"}',600);",
        "INSERT INTO parked(evidence,resolution) VALUES ",
        "('{\"kind\":\"parked\",\"reason\":\"HTTP 422\",\"targets\":[\"d-rejected\"],\"context\":null}','{\"time\":250}'),",
        "('{\"kind\":\"parked\",\"reason\":\"HTTP 422\",\"targets\":[\"d-rejected\"],\"context\":null}',NULL),",
        "('{\"kind\":\"conflict\",\"reason\":\"HTTP 409\",\"targets\":[\"d-conflict\"],\"context\":null}',NULL),",
        "('{\"kind\":\"parked\",\"reason\":\"HTTP 422\",\"targets\":[\"d-silent\"],\"context\":null}',NULL),",
        "('{\"kind\":\"interpretation_failure\",\"reason\":\"no frames\",\"targets\":[\"r-1\"],\"context\":null}',NULL);",
    ))?;
    Ok(())
}

// Owner decision 2026-11-20 (PR c): a parked item has a time. The step from 15
// keeps every row, fills only the time written in the same transaction as the
// parking (a rejected or conflicting delivery's attempt) and leaves the rest NULL.
#[test]
fn schema_sixteen_keeps_parked_items_and_fills_only_recorded_times() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("fifteen.db");
    store_at_fifteen(&path)?;
    let before: Vec<(i64, String, Option<String>)> = {
        let db = Connection::open(&path)?;
        assert_eq!(
            db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))?,
            STARTING_VERSION
        );
        let mut rows = db.prepare("SELECT id,evidence,resolution FROM parked ORDER BY id")?;
        rows.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<Result<_, _>>()?
    };
    let opened = jiff::Timestamp::now().as_second();
    let mut store = Store::open(&path, limits(), Box::new(Space(u64::MAX)))?;
    assert_eq!(
        store
            .db
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))?,
        SCHEMA_VERSION
    );
    let after: Vec<(i64, String, Option<String>, Option<i64>)> = {
        let mut rows = store
            .db
            .prepare("SELECT id,evidence,resolution,parked_at FROM parked ORDER BY id")?;
        rows.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
            .collect::<Result<_, _>>()?
    };
    assert_eq!(
        after
            .iter()
            .map(|(id, evidence, resolution, _)| (*id, evidence.clone(), resolution.clone()))
            .collect::<Vec<_>>(),
        before,
        "every item is kept as it was"
    );
    assert_eq!(
        after.iter().map(|row| row.3).collect::<Vec<_>>(),
        [Some(200), Some(400), Some(500), None, None],
        "the n-th rejection's attempt, the conflict's attempt, then NULL where no time was written"
    );
    assert_eq!(
        store.db.query_row(
            "SELECT value FROM alert_counts WHERE kind='parked'",
            [],
            |r| r.get::<_, i64>(0)
        )?,
        4,
        "the open count is untouched"
    );
    let item = store.park(&crate::Parked {
        kind: analyte_model::Identifier::new("mapping_finding")?,
        reason: analyte_model::Reason::new("synthetic")?,
        targets: vec![analyte_model::Identifier::new("r-2")?],
    })?;
    let parked_at: i64 =
        store
            .db
            .query_row("SELECT parked_at FROM parked WHERE id=?1", [item], |r| {
                r.get(0)
            })?;
    assert!(
        (opened..=jiff::Timestamp::now().as_second()).contains(&parked_at),
        "a new item gets the time it was parked: {parked_at}"
    );
    assert!(
        store
            .db
            .execute("UPDATE parked SET parked_at=1 WHERE id=?1", [item])
            .is_err(),
        "a parking time never changes"
    );
    Ok(())
}

/// A version-16 store, as the release before schema 17 wrote it: messages from
/// two machines, deliveries to two lab apps accepted once, twice (the second
/// time with an earlier clock), never or not yet, parked items with a JSON, a
/// plain-text and no resolution, audit entries and an order snapshot.
fn store_at_sixteen(path: &Path) -> Result<(), Error> {
    let db = Connection::open(path)?;
    db.execute_batch(include_str!("../schema.sql"))?;
    db.execute_batch(include_str!("../parked_at.sql"))?;
    db.execute_batch(concat!(
        "INSERT INTO receipts VALUES ",
        "('\"r-1\"','{\"machine\":\"m-1\",\"received_at\":\"2026-01-01T00:00:00Z\"}',x'00',x'00'),",
        "('\"r-2\"','{\"machine\":\"m-2\",\"received_at\":\"2026-01-01T00:00:00.5Z\"}',x'00',x'00'),",
        "('\"r-3\"','{\"machine\":\"m-1\",\"received_at\":\"2026-01-02T00:00:00Z\"}',x'00',x'00');",
        "INSERT INTO interpretations VALUES ('\"r-1\"',1,'{}'),('\"r-2\"',1,'{}'),('\"r-3\"',1,'{}');",
        "INSERT INTO outbox(id,kind,receipt,version,destination,ordering_key,payload,digest,contract,state,next_attempt,enqueued_at) VALUES ",
        "('\"d-twice\"','result','\"r-1\"',1,'main-lab','k1',x'00',x'00',1,'done',0,100),",
        "('\"d-waiting\"','result','\"r-2\"',1,'main-lab','k2',x'00',x'00',1,'pending',900,110),",
        "('\"d-once\"','result','\"r-3\"',1,'outpatient','k3',x'00',x'00',1,'done',0,120);",
        "INSERT INTO outbox(id,kind,order_id,destination,ordering_key,payload,digest,contract,state,next_attempt,enqueued_at) VALUES ",
        "('\"d-never\"','order_event','o-1','outpatient','o-1',x'00',x'00',1,'pending',0,130);",
        "INSERT INTO attempts(delivery,outcome,time) VALUES ",
        "('\"d-twice\"','\"Done\"',200),",
        "('\"d-waiting\"','{\"RetryAt\":900}',210),",
        "('\"d-twice\"','{\"RetryAt\":320}',300),",
        "('\"d-twice\"','\"Done\"',150),",
        "('\"d-once\"','\"Done\"',400);",
        "INSERT INTO parked(evidence,resolution) VALUES ",
        "('{\"kind\":\"mapping_finding\",\"reason\":\"unknown test\",\"targets\":[\"r-1\"],\"context\":null}','{\"action\":\"parked-resolve\",\"targets\":[\"1\"],\"time\":500}'),",
        "('{\"kind\":\"mapping_finding\",\"reason\":\"unknown test\",\"targets\":[\"r-2\"],\"context\":null}','resolved by hand'),",
        "('{\"kind\":\"mapping_finding\",\"reason\":\"unknown test\",\"targets\":[\"r-3\"],\"context\":null}',NULL);",
        "INSERT INTO audit(evidence) VALUES ",
        "('{\"action\":\"parked-resolve\",\"targets\":[\"1\"],\"time\":500}'),",
        "('{\"action\":\"setup-save\",\"targets\":[\"m-1\"],\"time\":600}');",
        "INSERT INTO machine_orders(destination,order_id,machine,version,state,created_at,updated_at,source_code) VALUES ",
        "('outpatient','o-1','m-1','1','Sent',700,700,'x');",
        "INSERT INTO machine_order_history(machine_order,evidence) VALUES ",
        "(1,'{\"key\":{\"machine\":\"m-1\"},\"state\":\"Sent\",\"updated_at\":700}');",
    ))?;
    Ok(())
}

/// Every row of the tables step 17 touches, in the columns schema 16 has.
fn rows_at_sixteen(db: &Connection) -> Result<Vec<Vec<rusqlite::types::Value>>, Error> {
    let mut rows = Vec::new();
    for sql in [
        "SELECT * FROM receipts ORDER BY rowid",
        "SELECT sequence,id,kind,receipt,version,order_id,destination,ordering_key,payload,digest,contract,state,next_attempt,enqueued_at,enqueue_time_backfilled FROM outbox ORDER BY sequence",
        "SELECT * FROM attempts ORDER BY sequence",
        "SELECT * FROM parked ORDER BY id",
        "SELECT * FROM audit ORDER BY sequence",
        "SELECT * FROM machine_order_history ORDER BY sequence",
        "SELECT * FROM alert_counts ORDER BY kind",
    ] {
        let mut statement = db.prepare(sql)?;
        let columns = statement.column_count();
        let mut read = statement.query([])?;
        while let Some(row) = read.next()? {
            rows.push(
                (0..columns)
                    .map(|column| row.get(column))
                    .collect::<Result<_, _>>()?,
            );
        }
    }
    Ok(rows)
}

/// The schema as stored: every table, index and trigger with its SQL.
fn schema(db: &Connection) -> Result<Vec<(String, Option<String>)>, Error> {
    let mut statement = db.prepare("SELECT name,sql FROM sqlite_master ORDER BY type,name")?;
    let rows = statement
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

/// Each delivery with when and by which attempt it was last accepted.
type Accepted = Vec<(String, Option<i64>, Option<i64>)>;

fn accepted(db: &Connection) -> Result<Accepted, Error> {
    let mut statement =
        db.prepare("SELECT id,delivered_at,delivered_attempt FROM outbox ORDER BY sequence")?;
    let rows = statement
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

// Owner decision 2026-11-20 (admin read speed, items 3 to 5): step 17 adds
// indexes and two helper columns only. It keeps every row and value, fills the
// helpers from the attempts already kept (the latest accepted time and the
// highest accepted sequence, NULL when never accepted), keeps them current on
// later attempts, and opening again changes nothing.
#[test]
fn schema_seventeen_keeps_every_row_and_fills_when_each_delivery_was_accepted() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("sixteen.db");
    store_at_sixteen(&path)?;
    let before = {
        let db = Connection::open(&path)?;
        assert_eq!(
            db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))?,
            16
        );
        rows_at_sixteen(&db)?
    };
    let store = Store::open(&path, limits(), Box::new(Space(u64::MAX)))?;
    assert_eq!(
        store
            .db
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))?,
        SCHEMA_VERSION
    );
    assert_eq!(
        rows_at_sixteen(&store.db)?,
        before,
        "every row is kept as it was"
    );
    let named =
        |id: &str, at: Option<i64>, attempt: Option<i64>| (format!("\"{id}\""), at, attempt);
    assert_eq!(
        accepted(&store.db)?,
        [
            named("d-twice", Some(200), Some(4)),
            named("d-waiting", None, None),
            named("d-once", Some(400), Some(5)),
            named("d-never", None, None),
        ],
        "the latest accepted time and the highest accepted attempt, from the kept attempts"
    );
    let indexes: Vec<String> = store
        .db
        .prepare("SELECT name FROM sqlite_master WHERE type='index' AND sql IS NOT NULL")?
        .query_map([], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    for index in [
        "attempts_accepted_time",
        "attempts_delivery",
        "audit_time",
        "machine_order_history_machine_updated",
        "machine_order_history_updated",
        "outbox_delivered_at",
        "outbox_delivered_attempt",
        "outbox_destination_state",
        "outbox_ordering",
        "outbox_receipt",
        "parked_resolved_time",
        "parked_time",
        "receipts_machine_received",
        "receipts_received",
    ] {
        assert!(indexes.iter().any(|name| name == index), "{index}");
    }
    // An index on stored JSON must never make a write fail: an audit entry
    // that is not JSON is still written (the clinical write beside it too).
    store
        .db
        .execute("INSERT INTO audit(evidence) VALUES ('not json')", [])?;
    // Later attempts keep the helpers current in the same write; a retry or an
    // earlier clock never lowers them.
    store.db.execute_batch(concat!(
        "INSERT INTO attempts(delivery,outcome,time) VALUES ",
        "('\"d-waiting\"','\"Done\"',220),",
        "('\"d-once\"','{\"RetryAt\":999}',410),",
        "('\"d-once\"','\"Done\"',390);",
    ))?;
    assert_eq!(
        accepted(&store.db)?,
        [
            named("d-twice", Some(200), Some(4)),
            named("d-waiting", Some(220), Some(6)),
            named("d-once", Some(400), Some(8)),
            named("d-never", None, None),
        ]
    );
    // A resolution that is not JSON stays writable.
    store.db.execute(
        "UPDATE parked SET resolution='resolved by hand' WHERE id=3",
        [],
    )?;
    let after = (
        schema(&store.db)?,
        rows_at_sixteen(&store.db)?,
        accepted(&store.db)?,
    );
    drop(store);
    let store = Store::open(&path, limits(), Box::new(Space(u64::MAX)))?;
    assert_eq!(
        (
            schema(&store.db)?,
            rows_at_sixteen(&store.db)?,
            accepted(&store.db)?
        ),
        after,
        "opening a version-17 store again changes nothing"
    );
    Ok(())
}

/// A version-17 store with two deliveries of one message and their tries in
/// turn: failed, cut off, accepted and rejected, as `(delivery, outcome, time)`.
fn store_at_seventeen(path: &Path) -> Result<(), Error> {
    let db = Connection::open(path)?;
    db.execute_batch(include_str!("../schema.sql"))?;
    db.execute_batch(include_str!("../parked_at.sql"))?;
    db.execute_batch(include_str!("../read_indexes.sql"))?;
    db.execute_batch(concat!(
        "INSERT INTO receipts VALUES ('\"r-1\"','{\"machine\":\"m-1\",\"received_at\":\"2026-01-01T00:00:00Z\"}',x'00',x'00');",
        "INSERT INTO interpretations VALUES ('\"r-1\"',1,'{}');",
        "INSERT INTO outbox(id,kind,receipt,version,destination,ordering_key,payload,digest,contract,state,next_attempt,enqueued_at) VALUES ",
        "('\"d-a\"','result','\"r-1\"',1,'main-lab','k1',x'00',x'00',1,'pending',0,90),",
        "('\"d-b\"','result','\"r-1\"',1,'outpatient','k2',x'00',x'00',1,'parked',0,90);",
        "INSERT INTO attempts(delivery,outcome,time) VALUES ",
        "('\"d-a\"','{\"RetryAt\":105}',100),",
        "('\"d-a\"','{\"RetryAt\":115}',110),",
        "('\"d-b\"','{\"RetryAt\":110}',105),",
        "('\"d-a\"','{\"RetryAt\":125}',120),",
        "('\"d-a\"','\"Done\"',130),",
        "('\"d-a\"','{\"RetryAt\":145}',140),",
        "('\"d-b\"','\"uncertain (interrupted)\"',150),",
        "('\"d-b\"','\"uncertain (interrupted)\"',160),",
        "('\"d-b\"','{\"Parked\":\"HTTP 422\"}',170),",
        "('\"d-b\"','{\"RetryAt\":185}',180);",
    ))?;
    Ok(())
}

/// Each run of tries: first and newest try, delivery, kind, the newest try's
/// time and how many tries.
type Run = (i64, i64, String, Option<i64>, i64, i64);

fn runs(db: &Connection) -> Result<Vec<Run>, Error> {
    let mut statement =
        db.prepare("SELECT first,last,delivery,kind,time,count FROM attempt_runs ORDER BY first")?;
    let rows = statement
        .query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
            ))
        })?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

// Owner decision 2026-11-21 (Activity groups retries): step 18 adds the runs
// of tries that were not accepted, one row per Activity line, and nothing
// else. It keeps every attempt, fills the runs from the attempts already
// kept, keeps them current in the same write as each later attempt (a try
// joins its delivery's run only after a try that failed the same way;
// accepted, rejected and paused tries end it), drops a delivery's runs with
// its tries, and opening again changes nothing.
#[test]
fn schema_eighteen_keeps_every_attempt_and_fills_the_runs_of_tries() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("seventeen.db");
    store_at_seventeen(&path)?;
    let attempts = |db: &Connection| -> Result<Vec<(i64, String, String, i64)>, Error> {
        let mut statement = db.prepare("SELECT * FROM attempts ORDER BY sequence")?;
        let rows = statement
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    };
    let before = attempts(&Connection::open(&path)?)?;
    let store = Store::open(&path, limits(), Box::new(Space(u64::MAX)))?;
    assert_eq!(
        store
            .db
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))?,
        SCHEMA_VERSION
    );
    assert_eq!(
        attempts(&store.db)?,
        before,
        "every attempt is kept as it was"
    );
    let run = |first: i64, last: i64, delivery: &str, kind: Option<i64>, time: i64, count: i64| {
        (first, last, format!("\"{delivery}\""), kind, time, count)
    };
    assert_eq!(
        runs(&store.db)?,
        [
            run(1, 4, "d-a", Some(2), 120, 3),
            run(3, 3, "d-b", Some(2), 105, 1),
            run(6, 6, "d-a", Some(2), 140, 1),
            run(7, 8, "d-b", Some(1), 160, 2),
            run(9, 9, "d-b", None, 170, 1),
            run(10, 10, "d-b", Some(2), 180, 1),
        ],
        "another delivery's tries in between do not end a run; an accepted try does"
    );
    store.db.execute_batch(concat!(
        "INSERT INTO attempts(delivery,outcome,time) VALUES ",
        "('\"d-a\"','{\"RetryAt\":155}',150),",
        "('\"d-b\"','{\"Paused\":\"keys\"}',190),",
        "('\"d-b\"','{\"Paused\":\"keys\"}',200),",
        "('\"d-b\"','\"uncertain (interrupted)\"',210),",
        "('\"d-b\"','{\"RetryAt\":225}',220),",
        "('\"d-a\"','\"Done\"',230);",
    ))?;
    assert_eq!(
        runs(&store.db)?,
        [
            run(1, 4, "d-a", Some(2), 120, 3),
            run(3, 3, "d-b", Some(2), 105, 1),
            run(6, 11, "d-a", Some(2), 150, 2),
            run(7, 8, "d-b", Some(1), 160, 2),
            run(9, 9, "d-b", None, 170, 1),
            run(10, 10, "d-b", Some(2), 180, 1),
            run(12, 12, "d-b", None, 190, 1),
            run(13, 13, "d-b", None, 200, 1),
            run(14, 14, "d-b", Some(1), 210, 1),
            run(15, 15, "d-b", Some(2), 220, 1),
        ],
        "later tries keep the runs current; a pause never joins one"
    );
    let after = (schema(&store.db)?, attempts(&store.db)?, runs(&store.db)?);
    drop(store);
    let store = Store::open(&path, limits(), Box::new(Space(u64::MAX)))?;
    assert_eq!(
        (schema(&store.db)?, attempts(&store.db)?, runs(&store.db)?),
        after,
        "opening a version-18 store again changes nothing"
    );
    // Data keeping removes all of a delivery's tries in one write (with the
    // immutability trigger lifted); its runs go with them.
    let guard: String = store.db.query_row(
        "SELECT sql FROM sqlite_master WHERE name='immutable_attempts_delete'",
        [],
        |r| r.get(0),
    )?;
    store.db.execute_batch(concat!(
        "DROP TRIGGER immutable_attempts_delete;",
        "DELETE FROM attempts WHERE delivery='\"d-a\"';"
    ))?;
    store.db.execute_batch(&guard)?;
    assert!(
        runs(&store.db)?
            .iter()
            .all(|(.., delivery, _, _, _)| delivery == "\"d-b\""),
        "only the other delivery's runs remain"
    );
    Ok(())
}

// ADR 0033: step 19 keeps every machine order as it was. Nothing kept beside
// an older order says which model its machine had, so its model stays
// unknown; it is never filled from a guess.
#[test]
fn schema_nineteen_keeps_every_machine_order_and_leaves_its_model_unknown() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("eighteen.db");
    {
        let db = Connection::open(&path)?;
        db.execute_batch(include_str!("../schema.sql"))?;
        db.execute_batch(include_str!("../parked_at.sql"))?;
        db.execute_batch(include_str!("../read_indexes.sql"))?;
        db.execute_batch(include_str!("../attempt_runs.sql"))?;
        db.execute_batch(concat!(
            "INSERT INTO machine_orders(destination,order_id,machine,version,state,created_at,updated_at,source_code) VALUES ",
            "('main-lab','o-1','m-a','1','Sending',100,100,'host-reserved'),",
            "('main-lab','o-2','m-b','3','Uncertain',100,120,'host-restart');",
        ))?;
    }
    let orders = |db: &Connection| -> Result<Vec<String>, Error> {
        let mut statement = db.prepare("SELECT json_array(id,destination,order_id,machine,version,state,created_at,updated_at,source_code) FROM machine_orders ORDER BY id")?;
        let rows = statement
            .query_map([], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    };
    let before = orders(&Connection::open(&path)?)?;
    let store = Store::open(&path, limits(), Box::new(Space(u64::MAX)))?;
    assert_eq!(
        store
            .db
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))?,
        SCHEMA_VERSION
    );
    assert_eq!(orders(&store.db)?, before, "every order is kept as it was");
    assert_eq!(
        store.db.query_row(
            "SELECT count(*) FROM machine_orders WHERE model IS NULL",
            [],
            |r| r.get::<_, i64>(0)
        )?,
        2
    );
    Ok(())
}
