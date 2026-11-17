//! Tests retained machine connection ownership.
use super::*;

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "One retained-evidence scenario checks refusals and rollback alongside its successful lifecycle"
)]
fn adr0003_v7_to_v8_keeps_frames_sequence_activity_and_immutability_with_caller_bounds()
-> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("frames-v7.db");
    let mut db = legacy(&path)?;
    Store::migrate_v7(&mut db, 65_536)?;
    // These are the evidence protections installed when schema 7 was opened.
    for operation in ["update", "delete"] {
        db.execute_batch(&format!("CREATE TRIGGER immutable_frames_{operation} BEFORE {operation} ON frames BEGIN SELECT RAISE(ABORT,'immutable evidence'); END;"))?;
    }
    db.execute("INSERT INTO transfers VALUES ('legacy','open',NULL)", [])?;
    let provenance = FrameProvenance {
        machine: Identifier::new("machine")?,
        driver: analyte_model::NamespacedId {
            namespace: Identifier::new("synthetic")?,
            value: Identifier::new("v1")?,
        },
        config: b"original config".to_vec(),
    };
    db.execute(
        "INSERT INTO frame_provenance VALUES ('legacy',?1)",
        [document(&provenance)?],
    )?;
    db.execute(
        "INSERT INTO frames(transfer,number,raw) VALUES ('legacy',1,?1),('legacy',7,?2)",
        params![b"first exact frame", b"second exact frame"],
    )?;
    {
        let refusal = db
            .execute(
                "INSERT INTO frames(transfer,number,raw) VALUES ('legacy',8,x'00')",
                [],
            )
            .map(|_| ())
            .expect_err("input must be refused");
        assert!(
            matches!(
                &refusal,
                rusqlite::Error::SqliteFailure(
                    rusqlite::ffi::Error {
                        code: rusqlite::ErrorCode::ConstraintViolation,
                        ..
                    },
                    _
                )
            ),
            "unexpected refusal: {refusal:?}"
        );
    }
    db.execute("UPDATE sqlite_sequence SET seq=900 WHERE name='frames'", [])?;
    db.execute("UPDATE machine_activity SET time=123", [])?;
    let before = snapshot(&db)?;
    drop(db);
    let mut store = open(&path)?;
    let mut after = snapshot(&store.db)?;
    assert_empty_retention_tables(&mut after);
    assert_eq!(after.remove("poll_intents"), Some(Vec::new()));
    assert_eq!(after.remove("poll_intent_history"), Some(Vec::new()));
    assert_eq!(after, before);
    assert_constraint(store.db.execute("UPDATE frames SET raw=x'00'", []));
    assert_constraint(store.db.execute("DELETE FROM frames", []));
    assert!(matches!(
        FrameSequence::new(32, 31),
        Err(IntakeError::InvalidReceiptInput)
    ));
    let transfer = Identifier::new("legacy")?;
    assert_eq!(
        store.journal_frame(
            &transfer,
            FrameSequence::new(31, 31)?,
            b"new exact frame",
            &provenance
        )?,
        901
    );
    let frames = store.frames(&transfer, 0)?;
    assert_eq!(
        frames.iter().map(|frame| frame.number).collect::<Vec<_>>(),
        vec![1, 7, 31]
    );
    assert_eq!(
        frames.last().ok_or("missing new frame")?.raw,
        b"new exact frame"
    );
    let reason = Reason::new("caller-specific incomplete exchange")?;
    store.finish_transfer(&transfer, TransferOutcome::Incomplete(reason))?;
    assert_eq!(
        store
            .parked(0)?
            .first()
            .ok_or("missing incomplete evidence")?
            .details()?
            .reason
            .as_str(),
        "caller-specific incomplete exchange"
    );
    assert!(matches!(
        store.journal_frame(&transfer, FrameSequence::new(0, 0)?, b"late", &provenance),
        Err(IntakeError::Conflict)
    ));
    let another = Identifier::new("byte-sequence")?;
    store.journal_frame(
        &another,
        FrameSequence::new(u8::MAX, u8::MAX)?,
        b"maximum sequence",
        &provenance,
    )?;
    assert_eq!(
        store
            .frames(&another, 0)?
            .first()
            .ok_or("missing maximum sequence")?
            .number,
        u8::MAX
    );
    drop(store);
    assert_eq!(open(&path)?.frames(&transfer, 0)?.len(), 3);
    Ok(())
}

#[test]
fn adr0003_v7_unrecognized_evidence_rolls_back_without_dropping_any_legacy_row() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("invalid.db");
    let db = legacy(&path)?;
    db.execute(
        "INSERT INTO parked(evidence) VALUES (?1)",
        [r#"["incomplete_transfer","transfer","valid first row"]"#],
    )?;
    db.execute(
        "INSERT INTO parked(evidence) VALUES ('unrecognized exact bytes')",
        [],
    )?;
    let before = snapshot(&db)?;
    drop(db);
    assert!(matches!(
        open(&path),
        Err(MaintenanceError::MigrationEvidence {
            table: "parked",
            row: 2,
            ..
        })
    ));
    let db = Connection::open(&path)?;
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))?,
        6
    );
    assert_eq!(snapshot(&db)?, before);
    Ok(())
}

#[test]
fn adr0003_v7_bad_owner_rolls_back_and_unknown_sql_values_are_never_defaulted() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("bad-owner.db");
    let db = legacy(&path)?;
    db.execute("INSERT INTO machine_orders(destination,order_id,machine,version,state,created_at,updated_at,source_code) VALUES ('app','order','machine','1','\"Sending\"',100,100,'app-cancel:invalid:owner')",[])?;
    db.execute(
        "INSERT INTO machine_order_history(machine_order,evidence) VALUES (1,?1)",
        [r#"{"source_code":"app-cancel:invalid:owner"}"#],
    )?;
    let before = snapshot(&db)?;
    drop(db);
    assert!(matches!(
        open(&path),
        Err(MaintenanceError::MigrationEvidence {
            table: "machine_order_history",
            row: 1,
            ..
        })
    ));
    let db = Connection::open(&path)?;
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))?,
        6
    );
    assert_eq!(snapshot(&db)?, before);
    {
        let refusal = db
            .query_row("SELECT 'unknown'", [], |row| row.get::<_, DeliveryState>(0))
            .map(|_| ())
            .expect_err("input must be refused");
        assert!(
            matches!(&refusal, rusqlite::Error::FromSqlConversionFailure(_, _, _)),
            "unexpected refusal: {refusal:?}"
        );
    }
    {
        let refusal = db
            .query_row("SELECT 'unknown'", [], |row| row.get::<_, DeliveryKind>(0))
            .map(|_| ())
            .expect_err("input must be refused");
        assert!(
            matches!(&refusal, rusqlite::Error::FromSqlConversionFailure(_, _, _)),
            "unexpected refusal: {refusal:?}"
        );
    }
    {
        let refusal = db
            .query_row("SELECT 'unknown'", [], |row| row.get::<_, SqlOrderState>(0))
            .map(|_| ())
            .expect_err("input must be refused");
        assert!(
            matches!(&refusal, rusqlite::Error::FromSqlConversionFailure(_, _, _)),
            "unexpected refusal: {refusal:?}"
        );
    }
    Ok(())
}
