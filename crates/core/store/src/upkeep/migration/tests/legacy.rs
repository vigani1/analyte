//! Tests legacy database migration.
use super::*;

#[test]
fn plan11_empty_database_migrates_and_newer_schema_is_refused() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("store.db");
    let store = open(&path)?;
    assert_eq!(
        store
            .db
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))?,
        SCHEMA_VERSION
    );
    assert_eq!(
        store
            .db
            .query_row("PRAGMA synchronous", [], |r| r.get::<_, i64>(0))?,
        2
    );
    assert_eq!(
        store
            .db
            .query_row("PRAGMA journal_mode", [], |r| r.get::<_, String>(0))?,
        "wal"
    );
    assert_eq!(
        store
            .db
            .query_row("PRAGMA foreign_keys", [], |r| r.get::<_, i64>(0))?,
        1
    );
    store
        .db
        .pragma_update(None, "user_version", SCHEMA_VERSION + 1)?;
    drop(store);
    assert!(
        matches!(open(&path), Err(MaintenanceError::SchemaTooNew(version)) if version == SCHEMA_VERSION + 1)
    );
    Ok(())
}

#[test]
fn plan11_v1_migration_preserves_rows_triggers_and_retry_history() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("v1.db");
    // Exercise the original v1 schema and APIs before opening through migration.
    let db = Connection::open(&path)?;
    db.execute_batch(include_str!("../../schema.sql"))?;
    for table in ["receipts", "frames", "interpretations", "attempts", "audit"] {
        db.execute_batch(&format!("CREATE TRIGGER immutable_{table}_update BEFORE UPDATE ON {table} BEGIN SELECT RAISE(ABORT,'immutable evidence'); END; CREATE TRIGGER immutable_{table}_delete BEFORE DELETE ON {table} BEGIN SELECT RAISE(ABORT,'immutable evidence'); END;"))?;
    }
    let mut v1 = Store {
        retention_key: None,
        _lock: Store::lock(&path, false)?,
        db,
        path: path.clone(),
        limits: limits(),
        space: Box::new(Space(u64::MAX)),
    };
    let metadata = receipt(b"raw")?;
    v1.receive(&metadata, b"raw", b"config")?;
    v1.reconcile(
        ReconcileAction::Interpret(metadata.id),
        interpretation()?,
        ReconcileInput::standard(&[], &[]),
        |_, _| Ok(Vec::new()),
    )?;
    let id = DeliveryId::try_from(uuid::Uuid::now_v7())?;
    seed_delivery(&v1, metadata.id, id, "key")?;
    v1.db.execute("UPDATE outbox SET state='in_flight'", [])?;
    v1.attempted(id, &Attempt::RetryAt(10), 0)?;
    v1.frame(
        &Identifier::new("transfer")?,
        FrameSequence::new(1, 7)?,
        b"frame",
    )?;
    let audit = Audit {
        actor: Identifier::new("operator")?,
        action: Identifier::new("test")?,
        targets: vec![],
        time: 1,
        reason: Reason::new("synthetic")?,
    };
    v1.audit(&audit)?;
    v1.park(&Parked {
        kind: Identifier::new("test")?,
        reason: Reason::new("synthetic")?,
        targets: vec![],
    })?;
    v1.order_page(&OrderPage {
        destination: Identifier::new("app")?,
        cursor: Identifier::new("cursor")?,
        complete: true,
        polled: 1,
        orders: vec![CachedOrder {
            id: Identifier::new("order")?,
            sample: SampleId::new("001")?,
            document: "synthetic".into(),
            tombstone: false,
            expiry: 100,
        }],
    })?;
    let tables = [
        "receipts",
        "transfers",
        "frames",
        "interpretations",
        "outbox",
        "attempts",
        "parked",
        "audit",
        "feed",
        "orders",
    ];
    let rows = tables
        .iter()
        .map(|table| {
            v1.db
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| {
                    r.get::<_, i64>(0)
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let triggers: Vec<String> = v1
        .db
        .prepare("SELECT name FROM sqlite_master WHERE type='trigger' ORDER BY name")?
        .query_map([], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    drop(v1);
    let mut migrated = open(&path)?;
    verify_plan11_v1_evidence(&mut migrated, metadata.id, id, &tables, rows, triggers)
}

#[test]
fn plan11_v2_to_v3_migration_retains_every_row_and_trigger() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("v2.db");
    let mut db = Connection::open(&path)?;
    db.execute_batch(include_str!("../../schema.sql"))?;
    Store::migrate_v2(&mut db)?;
    let mut v2 = Store {
        retention_key: None,
        _lock: Store::lock(&path, false)?,
        db,
        path: path.clone(),
        limits: limits(),
        space: Box::new(Space(u64::MAX)),
    };
    let metadata = receipt(b"legacy")?;
    v2.receive(&metadata, b"legacy", b"config")?;
    let mut evidence = interpretation()?;
    evidence.outcome = Reading::Results(vec![production_result(metadata.id)?]);
    v2.db.execute(
        "INSERT INTO interpretations VALUES (?1,1,?2)",
        params![key(&metadata.id)?, document(&evidence)?],
    )?;
    v2.frame(
        &Identifier::new("legacy-transfer")?,
        FrameSequence::new(1, 7)?,
        b"legacy-frame",
    )?;
    let triggers: Vec<String> = v2
        .db
        .prepare("SELECT name FROM sqlite_master WHERE type='trigger'")?
        .query_map([], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    drop(v2);
    let mut store = open(&path)?;
    assert_eq!(
        revised_receipt(&mut store, "000123", false, "")?,
        RevisionOutcome::Retransmission(analyte_model::Revision(0))
    );
    assert_eq!(store.raw(metadata.id)?, Some(b"legacy".to_vec()));
    assert!(store.interpretation(metadata.id, 1)?.is_some());
    assert_eq!(
        store.frames(&Identifier::new("legacy-transfer")?, 0)?.len(),
        1
    );
    assert_eq!(
        store
            .db
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))?,
        SCHEMA_VERSION
    );
    for trigger in triggers {
        assert_eq!(
            store.db.query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='trigger' AND name=?1",
                [trigger],
                |r| r.get::<_, i64>(0)
            )?,
            1
        );
    }
    Ok(())
}

#[test]
fn mutation_adr0003_migration_bounds_metadata_and_interpretation_independently_and_inclusively()
-> TestResult {
    for case in ["at", "metadata-over", "interpretation-over"] {
        let mut db = Connection::open_in_memory()?;
        db.execute_batch(include_str!("../../schema.sql"))?;
        Store::migrate_v2(&mut db)?;
        let metadata = receipt(b"legacy")?;
        let mut evidence = interpretation()?;
        evidence.outcome = Reading::Results(vec![production_result(metadata.id)?]);
        let pad =
            |mut encoded: String, size: usize| -> Result<String, Box<dyn std::error::Error>> {
                let extra = size
                    .checked_sub(encoded.len())
                    .ok_or("fixture exceeds bound")?;
                encoded.push_str(&" ".repeat(extra));
                Ok(encoded)
            };
        let limit = 4096;
        let metadata_text = pad(
            document(&metadata)?,
            if case == "metadata-over" { 4097 } else { limit },
        )?;
        let evidence_text = pad(
            document(&evidence)?,
            if case == "interpretation-over" {
                4097
            } else {
                limit
            },
        )?;
        db.execute(
            "INSERT INTO receipts VALUES (?1,?2,?3,?4)",
            params![key(&metadata.id)?, metadata_text, b"legacy", b"config"],
        )?;
        db.execute(
            "INSERT INTO interpretations VALUES (?1,1,?2)",
            params![key(&metadata.id)?, evidence_text],
        )?;
        let result = Store::migrate_v3(&mut db, limit);
        if case == "at" {
            result?;
            assert_eq!(
                db.query_row("SELECT count(*) FROM observation_revisions", [], |r| r
                    .get::<_, i64>(0))?,
                1
            );
        } else {
            assert!(
                matches!(
                    result,
                    Err(MaintenanceError::Storage(StorageError::LimitExceeded {
                        what: "record_bytes"
                    }))
                ),
                "case {case}"
            );
            assert_eq!(
                db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))?,
                2
            );
            assert_eq!(
                db.query_row(
                    "SELECT count(*) FROM sqlite_schema WHERE name='observation_revisions'",
                    [],
                    |r| r.get::<_, i64>(0)
                )?,
                0
            );
        }
        assert_eq!(
            db.query_row("SELECT metadata FROM receipts", [], |r| r
                .get::<_, String>(0))?,
            metadata_text
        );
        assert_eq!(
            db.query_row("SELECT evidence FROM interpretations", [], |r| r
                .get::<_, String>(0))?,
            evidence_text
        );
    }
    Ok(())
}

#[test]
fn mutation_adr0003_migration_refuses_conflicting_legacy_revision_and_accepts_repetition()
-> TestResult {
    for conflicting in [false, true] {
        let mut db = Connection::open_in_memory()?;
        db.execute_batch(include_str!("../../schema.sql"))?;
        Store::migrate_v2(&mut db)?;
        let metadata = receipt(b"legacy")?;
        db.execute(
            "INSERT INTO receipts VALUES (?1,?2,?3,?4)",
            params![
                key(&metadata.id)?,
                document(&metadata)?,
                b"legacy",
                b"config"
            ],
        )?;
        for version in 1..=2 {
            let mut result = production_result(metadata.id)?;
            if conflicting && version == 2 {
                result.sample.as_mut().ok_or("missing sample")?.id = SampleId::new("different")?;
            }
            let mut evidence = interpretation()?;
            evidence.outcome = Reading::Results(vec![result]);
            db.execute(
                "INSERT INTO interpretations VALUES (?1,?2,?3)",
                params![key(&metadata.id)?, version, document(&evidence)?],
            )?;
        }
        let result = Store::migrate_v3(&mut db, 65_536);
        if conflicting {
            assert!(matches!(result, Err(MaintenanceError::Conflict)));
            assert_eq!(
                db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))?,
                2
            );
            assert_eq!(
                db.query_row(
                    "SELECT count(*) FROM sqlite_schema WHERE name='observation_revisions'",
                    [],
                    |r| r.get::<_, i64>(0)
                )?,
                0
            );
        } else {
            result?;
            assert_eq!(
                db.query_row("SELECT count(*) FROM observation_revisions", [], |r| r
                    .get::<_, i64>(0))?,
                1
            );
        }
        assert_eq!(
            db.query_row("SELECT count(*) FROM interpretations", [], |r| r
                .get::<_, i64>(0))?,
            2
        );
        assert_eq!(
            db.query_row("SELECT raw FROM receipts", [], |r| r.get::<_, Vec<u8>>(0))?,
            b"legacy"
        );
    }
    Ok(())
}
