//! Checks durable custody, immutable evidence and recovery using temporary stores.
use crate::test_support::*;
use crate::*;

#[test]
fn adr0003_rule7_sqlite_lock_failures_remain_specific_busy_failures() -> TestResult {
    // SQLite result codes 5 and 6 are BUSY and LOCKED. No wall-clock wait is needed.
    for code in [5, 6] {
        let error = rusqlite::Error::SqliteFailure(rusqlite::ffi::Error::new(code), None);
        assert!(matches!(StorageError::from(error), StorageError::Busy(_)));
    }
    Ok(())
}

#[test]
fn adr0003_storage_failures_distinguish_full_corrupt_conflict_and_unknown() -> TestResult {
    // SQLite primary result codes; synthetic failures exercise the host recovery boundary.
    for (code, expected) in [
        (13, "full"),
        (11, "corrupt"),
        (26, "corrupt"),
        (19, "conflict"),
    ] {
        let error = StorageError::from(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(code),
            None,
        ));
        match expected {
            "full" => assert!(matches!(error, StorageError::DiskFull(_))),
            "corrupt" => assert!(matches!(error, StorageError::Corruption(_))),
            "conflict" => assert!(matches!(error, StorageError::ConstraintViolation(_))),
            _ => unreachable!("fixed test cases"),
        }
    }
    let original = rusqlite::Error::InvalidQuery;
    assert!(matches!(
        StorageError::from(original),
        StorageError::Sqlite(rusqlite::Error::InvalidQuery)
    ));
    Ok(())
}

#[test]
fn mutation_rule7_lib_limits_validate_each_field_and_include_thirty_seconds() -> TestResult {
    for (rows, record, busy, valid) in [
        (1, 1, Duration::from_secs(30), true),
        (0, 1, Duration::from_secs(1), false),
        (1, 0, Duration::from_secs(1), false),
        (1, 1, Duration::ZERO, false),
        (1, 1, Duration::from_secs(31), false),
    ] {
        let mut bounds = limits();
        bounds.page_rows = rows;
        bounds.record_bytes = record;
        bounds.busy_timeout = busy;
        let result = bounds.validate();
        if valid {
            result?;
        } else {
            assert!(matches!(result, Err(MaintenanceError::InvalidLimits)));
        }
    }
    Ok(())
}

#[test]
fn mutation_adr0003_rule1_lib_sqlite_errors_keep_specific_recovery_actions() -> TestResult {
    use rusqlite::ffi;
    for code in [
        ffi::SQLITE_FULL,
        ffi::SQLITE_CORRUPT,
        ffi::SQLITE_NOTADB,
        ffi::SQLITE_CONSTRAINT,
        ffi::SQLITE_BUSY,
        ffi::SQLITE_LOCKED,
        ffi::SQLITE_IOERR,
    ] {
        let error = StorageError::from(rusqlite::Error::SqliteFailure(
            ffi::Error::new(code),
            Some("synthetic failure".into()),
        ));
        assert!(match code {
            ffi::SQLITE_FULL => matches!(error, StorageError::DiskFull(_)),
            ffi::SQLITE_CORRUPT | ffi::SQLITE_NOTADB =>
                matches!(error, StorageError::Corruption(rusqlite::Error::SqliteFailure(cause, Some(message))) if cause.extended_code == code && message == "synthetic failure"),
            ffi::SQLITE_CONSTRAINT => matches!(error, StorageError::ConstraintViolation(_)),
            ffi::SQLITE_BUSY | ffi::SQLITE_LOCKED => matches!(error, StorageError::Busy(_)),
            ffi::SQLITE_IOERR =>
                matches!(error,StorageError::Sqlite(rusqlite::Error::SqliteFailure(cause,Some(message))) if cause.extended_code==code && message=="synthetic failure"),
            _ => unreachable!(),
        });
    }
    assert!(matches!(
        StorageError::from(rusqlite::Error::QueryReturnedNoRows),
        StorageError::Sqlite(rusqlite::Error::QueryReturnedNoRows)
    ));
    Ok(())
}

#[test]
fn mutation_rule7_lib_encoding_digest_and_trust_keep_their_contracts() -> TestResult {
    use std::io::Write;
    assert_eq!(
        document(&serde_json::json!({"sample":"001"}))?,
        r#"{"sample":"001"}"#
    );
    assert_eq!(
        digest(b"abc").0,
        [
            0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea, 0x41, 0x41, 0x40, 0xde, 0x5d, 0xae,
            0x22, 0x23, 0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c, 0xb4, 0x10, 0xff, 0x61,
            0xf2, 0x00, 0x15, 0xad
        ]
    );
    let mut writer = BoundedJson {
        bytes: vec![],
        limit: 3,
    };
    assert_eq!(writer.write(b"ab")?, 2);
    assert_eq!(writer.write(b"c")?, 1);
    {
        let refusal = writer
            .write(b"d")
            .map(|_| ())
            .expect_err("input must be refused");
        assert!(
            refusal.kind() == std::io::ErrorKind::FileTooLarge,
            "unexpected refusal: {refusal:?}"
        );
    }
    assert_eq!(writer.bytes, b"abc");
    writer.flush()?;
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    store.limits.record_bytes = 5;
    assert_eq!(store.encode(&"001")?, r#""001""#);
    store.limits.record_bytes = 4;
    {
        let refusal = store
            .encode(&"001")
            .map(|_| ())
            .expect_err("input must be refused");
        assert!(
            matches!(
                &refusal,
                StorageError::LimitExceeded {
                    what: "record_bytes"
                }
            ),
            "unexpected refusal: {refusal:?}"
        );
    }
    assert!(OrderTrust::Trusted { age: 0 }.is_trusted());
    for trust in [
        OrderTrust::Stale { age: 1 },
        OrderTrust::Incomplete,
        OrderTrust::RestorePaused,
        OrderTrust::Unknown,
    ] {
        assert!(!trust.is_trusted());
    }
    Ok(())
}

#[test]
fn sqlite_result_codes_preserve_lock_and_constraint_causes() {
    use std::error::Error as _;
    for code in [
        rusqlite::ffi::SQLITE_BUSY,
        rusqlite::ffi::SQLITE_LOCKED,
        rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE,
    ] {
        let error = StorageError::from(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(code),
            Some("synthetic diagnostic".into()),
        ));
        let source = error
            .source()
            .and_then(|source| source.downcast_ref::<rusqlite::Error>())
            .expect("original SQLite source");
        assert!(
            matches!(source, rusqlite::Error::SqliteFailure(cause, Some(message)) if cause.extended_code == code && message == "synthetic diagnostic")
        );
    }
}

// SQLite sqlite3_busy_handler: a transaction that has already read and then
// asks to write while another connection writes gets SQLITE_BUSY at once, with
// no busy-handler wait. An operator command committing on the same store must
// only delay a service write within busy_timeout (AGENTS rule 7), never fail it.
#[test]
fn rule7_read_then_write_waits_for_another_writer_within_busy_timeout() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("store.db");
    let mut bounds = limits();
    bounds.busy_timeout = Duration::from_secs(10);
    let mut store = Store::open(&path, bounds, Box::new(Space(u64::MAX)))?;
    let metadata = receipt(b"raw")?;
    store.receive(&metadata, b"raw", b"config")?;
    store.reconcile(
        ReconcileAction::Interpret(metadata.id),
        interpretation()?,
        ReconcileInput::standard(&[], &[]),
        |_, _| Ok(Vec::new()),
    )?;
    let id = DeliveryId::try_from(uuid::Uuid::now_v7())?;
    seed_delivery(&store, metadata.id, id, "machine/observation")?;
    // Another connection, like an operator command, holds the write lock briefly.
    let operator = Connection::open(&path)?;
    operator.execute_batch("BEGIN IMMEDIATE")?;
    let release = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(200));
        operator.execute_batch("COMMIT")
    });
    // The claim reads the queue head before marking it in flight.
    let claim = store.claim_destination(0, &Identifier::new("app")?);
    release.join().map_err(|_| "operator thread panicked")??;
    assert_eq!(claim?.ok_or("pending delivery not claimed")?.id, id);
    Ok(())
}
