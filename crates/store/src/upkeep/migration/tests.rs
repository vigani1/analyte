//! upkeep/migration regression tests kept beside the storage behavior they guard.
use crate::test_support::*;
use crate::*;
use analyte_model::Reason;

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
    store.db.execute_batch("PRAGMA user_version=10")?;
    drop(store);
    assert!(matches!(open(&path), Err(Error::SchemaTooNew(10))));
    Ok(())
}

#[test]
fn plan11_v1_migration_preserves_rows_triggers_and_retry_history() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("v1.db");
    // Exercise the original v1 schema and APIs before opening through migration.
    let db = Connection::open(&path)?;
    db.execute_batch(include_str!("../schema.sql"))?;
    for table in ["receipts", "frames", "interpretations", "attempts", "audit"] {
        db.execute_batch(&format!("CREATE TRIGGER immutable_{table}_update BEFORE UPDATE ON {table} BEGIN SELECT RAISE(ABORT,'immutable evidence'); END; CREATE TRIGGER immutable_{table}_delete BEFORE DELETE ON {table} BEGIN SELECT RAISE(ABORT,'immutable evidence'); END;"))?;
    }
    let mut v1 = Store {
        _lock: Store::lock(&path, false)?,
        db,
        path: path.clone(),
        limits: limits(),
        space: Box::new(Space(u64::MAX)),
    };
    let metadata = receipt(b"raw")?;
    v1.receive(&metadata, b"raw", b"config")?;
    v1.interpret_reconciled(metadata.id, interpretation()?, &[], &[], |_, _| {
        Ok(Vec::new())
    })?;
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

fn verify_plan11_v1_evidence(
    migrated: &mut Store,
    receipt_id: ReceiptId,
    id: DeliveryId,
    tables: &[&str],
    rows: Vec<i64>,
    triggers: Vec<String>,
) -> TestResult {
    for (table, count) in tables.iter().zip(rows) {
        assert_eq!(
            migrated
                .db
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                    .get::<_, i64>(0))?,
            count
        );
    }
    assert_eq!(migrated.raw(receipt_id)?, Some(b"raw".to_vec()));
    assert_eq!(migrated.frames(&Identifier::new("transfer")?, 0)?.len(), 1);
    assert!(migrated.interpretation(receipt_id, 1)?.is_some());
    let claim = migrated.claim(10)?.ok_or(Error::Corruption)?;
    assert_eq!(claim.id, id);
    assert_eq!(claim.payload, vec![0, 17, 34]);
    assert_eq!(claim.earlier_attempts, 1);
    let backfilled: bool =
        migrated
            .db
            .query_row("SELECT enqueue_time_backfilled FROM outbox", [], |r| {
                r.get(0)
            })?;
    assert!(backfilled);
    for trigger in triggers {
        assert!(
            migrated.db.query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='trigger' AND name=?1",
                [trigger],
                |r| r.get::<_, i64>(0)
            )? == 1
        );
    }
    for table in [
        "receipts",
        "frames",
        "interpretations",
        "attempts",
        "audit",
        "outbox",
    ] {
        assert!(
            migrated
                .db
                .execute(&format!("DELETE FROM {table}"), [])
                .is_err()
        );
    }
    assert!(
        migrated
            .db
            .execute("UPDATE outbox SET payload=x'00'", [])
            .is_err()
    );
    Ok(())
}

#[test]
fn plan11_v2_to_v3_migration_retains_every_row_and_trigger() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("v2.db");
    let mut db = Connection::open(&path)?;
    db.execute_batch(include_str!("../schema.sql"))?;
    Store::migrate_v2(&mut db)?;
    let mut v2 = Store {
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
        params![key(&metadata.id)?, key(&evidence)?],
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
        db.execute_batch(include_str!("../schema.sql"))?;
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
            key(&metadata)?,
            if case == "metadata-over" { 4097 } else { limit },
        )?;
        let evidence_text = pad(
            key(&evidence)?,
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
            assert!(matches!(result, Err(Error::InvalidInput)), "case {case}");
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
        db.execute_batch(include_str!("../schema.sql"))?;
        Store::migrate_v2(&mut db)?;
        let metadata = receipt(b"legacy")?;
        db.execute(
            "INSERT INTO receipts VALUES (?1,?2,?3,?4)",
            params![key(&metadata.id)?, key(&metadata)?, b"legacy", b"config"],
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
                params![key(&metadata.id)?, version, key(&evidence)?],
            )?;
        }
        let result = Store::migrate_v3(&mut db, 65_536);
        if conflicting {
            assert!(matches!(result, Err(Error::Conflict)));
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

use crate::sql_types::SqlOrderState;
use crate::test_support::{TestResult, open, receipt};
use analyte_model::{Identifier, OrderState};
use rusqlite::{Connection, params, types::Value};
use std::{collections::BTreeMap, num::NonZeroU64, path::Path};

fn legacy(path: &Path) -> Result<Connection, Error> {
    let mut db = Connection::open(path)?;
    db.execute_batch(include_str!("../schema.sql"))?;
    Store::migrate_v2(&mut db)?;
    Store::migrate_v3(&mut db, 65_536)?;
    db.execute_batch(include_str!("../migration_v4.sql"))?;
    db.execute_batch(include_str!("../migration_v5.sql"))?;
    db.execute_batch(include_str!("../migration_v6.sql"))?;
    Ok(db)
}

#[test]
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
        [key(&provenance)?],
    )?;
    db.execute(
        "INSERT INTO frames(transfer,number,raw) VALUES ('legacy',1,?1),('legacy',7,?2)",
        params![b"first exact frame", b"second exact frame"],
    )?;
    assert!(
        db.execute(
            "INSERT INTO frames(transfer,number,raw) VALUES ('legacy',8,x'00')",
            []
        )
        .is_err()
    );
    db.execute("UPDATE sqlite_sequence SET seq=900 WHERE name='frames'", [])?;
    db.execute("UPDATE machine_activity SET time=123", [])?;
    let before = snapshot(&db)?;
    drop(db);
    let mut store = open(&path)?;
    let mut after = snapshot(&store.db)?;
    assert_eq!(after.remove("poll_intents"), Some(Vec::new()));
    assert_eq!(after.remove("poll_intent_history"), Some(Vec::new()));
    assert_eq!(after, before);
    assert!(store.db.execute("UPDATE frames SET raw=x'00'", []).is_err());
    assert!(store.db.execute("DELETE FROM frames", []).is_err());
    assert!(matches!(
        FrameSequence::new(32, 31),
        Err(Error::InvalidInput)
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
        Err(Error::Conflict)
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

type Snapshot = BTreeMap<String, Vec<Vec<Value>>>;

fn snapshot(db: &Connection) -> Result<Snapshot, Error> {
    let names = db.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    names
        .into_iter()
        .map(|name| {
            let mut statement = db.prepare(&format!(
                "SELECT * FROM \"{}\" ORDER BY rowid",
                name.replace('"', "\"\"")
            ))?;
            let rows = statement
                .query_map([], |row| {
                    (0..row.as_ref().column_count())
                        .map(|column| row.get(column))
                        .collect::<Result<Vec<Value>, _>>()
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok((name, rows))
        })
        .collect()
}

#[test]
fn adr0003_v6_every_state_owner_and_parking_shape_migrates_without_losing_meaning() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("v6.db");
    let db = legacy(&path)?;
    let (expected_orders, owner_count) = seed_adr0003_v6_order_states(&db)?;
    let metadata = receipt(b"original bytes")?;
    db.execute(
        "INSERT INTO receipts VALUES (?1,?2,?3,?4)",
        params![
            key(&metadata.id)?,
            key(&metadata)?,
            b"original bytes",
            b"config"
        ],
    )?;
    db.execute(
        "INSERT INTO interpretations VALUES (?1,1,'original interpretation')",
        [key(&metadata.id)?],
    )?;
    let expected_deliveries = seed_adr0003_v6_deliveries(&db, metadata.id)?;
    let expected_parking = seed_adr0003_v6_parking(&db, metadata.id, &expected_deliveries)?;
    let before = snapshot(&db)?;
    drop(db);
    let mut store = open(&path)?;
    let after = snapshot(&store.db)?;
    verify_adr0003_v6_retained_columns(before, &after)?;
    verify_adr0003_v6_order_states(&mut store, expected_orders)?;
    for (id, state, kind) in expected_deliveries {
        assert_eq!(store.delivery(id)?.ok_or("missing delivery")?.state, state);
        assert_eq!(
            store
                .db
                .query_row("SELECT kind FROM outbox WHERE id=?1", [key(&id)?], |r| {
                    r.get::<_, DeliveryKind>(0)
                })?,
            kind
        );
    }
    for (id, original, resolution, kind, target, reason, context) in expected_parking {
        let row = store.parked_item(id)?.ok_or("missing parked row")?;
        assert_eq!(row.resolution.as_deref(), resolution);
        let details = row.details()?;
        assert_eq!(details.kind.as_str(), kind);
        assert_eq!(details.reason.as_str(), reason);
        assert_eq!(details.targets, vec![Identifier::new(&target)?]);
        let canonical: serde_json::Value = serde_json::from_str(&row.evidence)?;
        assert!(canonical.is_object());
        assert_eq!(canonical.get("context"), Some(&context));
        assert_eq!(
            store.db.query_row(
                "SELECT original_evidence FROM parked WHERE id=?1",
                [id],
                |r| r.get::<_, String>(0)
            )?,
            original
        );
    }
    assert_eq!(
        store.db.query_row(
            "SELECT value FROM alert_counts WHERE kind='uncertain'",
            [],
            |r| r.get::<_, i64>(0)
        )?,
        owner_count
    );
    assert_eq!(
        store.db.query_row(
            "SELECT value FROM alert_counts WHERE kind='parked'",
            [],
            |r| r.get::<_, i64>(0)
        )?,
        8
    );
    store.db.execute("INSERT INTO machine_orders(destination,order_id,machine,version,state,created_at,updated_at,source_code) VALUES ('app','new-order','machine','1',?1,102,102,'new')",[SqlOrderState(OrderState::Sending)])?;
    assert_eq!(store.db.last_insert_rowid(), 1001);
    assert!(
        store
            .db
            .execute("UPDATE machine_order_history SET owner_id='changed'", [])
            .is_err()
    );
    assert!(
        store
            .db
            .execute("UPDATE parked SET original_evidence='changed'", [])
            .is_err()
    );
    assert!(store.db.execute("DELETE FROM parked", []).is_err());
    drop(store);
    assert_eq!(
        open(&path)?
            .db
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))?,
        SCHEMA_VERSION
    );
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
        Err(Error::MigrationEvidence {
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
        Err(Error::MigrationEvidence {
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
    assert!(
        db.query_row("SELECT 'unknown'", [], |row| row.get::<_, DeliveryState>(0))
            .is_err()
    );
    assert!(
        db.query_row("SELECT 'unknown'", [], |row| row.get::<_, DeliveryKind>(0))
            .is_err()
    );
    assert!(
        db.query_row("SELECT 'unknown'", [], |row| row.get::<_, SqlOrderState>(0))
            .is_err()
    );
    Ok(())
}

fn version8(path: &Path) -> Result<Connection, Error> {
    let mut db = legacy(path)?;
    Store::migrate_v7(&mut db, 65_536)?;
    db.execute_batch(include_str!("../migration_v8.sql"))?;
    Ok(db)
}

#[test]
fn adr0003_v8_custody_migration_keeps_every_parked_row_and_reconciles_typed_machine_intents()
-> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("custody-v8.db");
    let mut db = version8(&path)?;
    let tx = db.transaction()?;
    for _ in 0..130 {
        tx.execute("INSERT INTO parked(evidence) VALUES (?1)",[r#"{"kind":"other","reason":"unrelated retained evidence","targets":[],"context":null}"#])?;
    }
    let fixtures = [
        (vec!["autolyser"], "unconfirmed original R", None),
        (
            vec!["autolyser"],
            "original resolved guard",
            Some("exact legacy resolution evidence"),
        ),
        (
            vec!["autolyser", "other", "autolyser"],
            "second unresolved poll",
            None,
        ),
        (
            vec!["resolved-only"],
            "confirmed legacy cursor",
            Some("second exact resolution"),
        ),
    ];
    let mut expected = Vec::new();
    let mut guards = Vec::new();
    for (targets, reason, resolution) in fixtures {
        let evidence = serde_json::json!({"kind":"autolyser_poll_custody","reason":reason,"targets":targets,"context":null}).to_string();
        let original = format!("  {evidence}  ");
        tx.execute(
            "INSERT INTO parked(evidence,resolution,original_evidence) VALUES (?1,?2,?3)",
            params![evidence, resolution, original],
        )?;
        let parked = tx.last_insert_rowid();
        guards.push(parked);
        let state = if resolution.is_some() {
            PollIntentState::Confirmed
        } else {
            PollIntentState::Uncertain
        };
        let mut seen = std::collections::HashSet::new();
        for target in targets {
            if seen.insert(target) {
                expected.push((
                    target.to_owned(),
                    parked,
                    state,
                    reason.to_owned(),
                    resolution.map(str::to_owned),
                ));
            }
        }
    }
    tx.execute("UPDATE sqlite_sequence SET seq=900 WHERE name='parked'", [])?;
    tx.commit()?;
    let before = snapshot(&db)?;
    drop(db);
    let mut limits = limits();
    limits.page_rows = 1; // The first custody row lies beyond the old 128-page scan cap.
    let mut store = Store::open(&path, limits, Box::new(Space(u64::MAX)))?;
    let mut after = snapshot(&store.db)?;
    assert_eq!(
        after
            .remove("poll_intents")
            .ok_or("missing typed intents")?
            .len(),
        5
    );
    assert_eq!(
        after
            .remove("poll_intent_history")
            .ok_or("missing typed history")?
            .len(),
        5
    );
    assert_eq!(after, before);
    let actual = store.db.prepare("SELECT p.machine,p.parked,p.state,p.reason,h.audit FROM poll_intents p JOIN poll_intent_history h ON h.intent=p.id ORDER BY p.id")?
        .query_map([],|row|Ok((row.get::<_,String>(0)?,row.get::<_,i64>(1)?,row.get::<_,PollIntentState>(2)?,row.get::<_,String>(3)?,row.get::<_,Option<String>>(4)?)))?
        .collect::<Result<Vec<_>,_>>()?;
    assert_eq!(actual, expected);
    verify_adr0003_migrated_custody_reconciliation(&mut store, &guards)
}

fn verify_adr0003_migrated_custody_reconciliation(store: &mut Store, guards: &[i64]) -> TestResult {
    let machine = Identifier::new("autolyser")?;
    let intent = store
        .unconfirmed_poll_intent(&machine)?
        .ok_or("missing migrated blocking guard")?;
    assert_eq!(Some(&intent.parked), guards.first());
    assert_eq!(intent.state, PollIntentState::Uncertain);
    let reason = Reason::new("explicit operator cursor reconciliation")?;
    assert!(matches!(
        store.begin_poll_intent(&machine, &reason),
        Err(Error::PollIntentUnresolved)
    ));
    assert!(
        store
            .unconfirmed_poll_intent(&Identifier::new("resolved-only")?)?
            .is_none()
    );
    let audit = Audit {
        actor: Identifier::new("operator")?,
        action: Identifier::new("parked-resolve")?,
        targets: vec![Identifier::new(&intent.parked.to_string())?],
        time: 100,
        reason: reason.clone(),
    };
    store.resolve(intent.parked, &audit)?;
    assert!(store.unconfirmed_poll_intent(&machine)?.is_some());
    let other = Identifier::new("other")?;
    let shared = store
        .unconfirmed_poll_intent(&other)?
        .ok_or("missing shared machine intent")?;
    let individual = Audit {
        actor: Identifier::new("host")?,
        action: Identifier::new("poll-confirmation")?,
        targets: vec![Identifier::new(&format!("poll:{}", shared.id))?],
        time: 101,
        reason: reason.clone(),
    };
    store.record_poll_outcome(shared.id, PollOutcome::Confirmed, &individual)?;
    assert!(store.unconfirmed_poll_intent(&other)?.is_none());
    assert!(store.unconfirmed_poll_intent(&machine)?.is_some());
    assert!(
        store
            .parked_item(shared.parked)?
            .ok_or("lost shared evidence")?
            .resolution
            .is_none()
    );
    store.resolve(*guards.get(2).ok_or("missing shared guard")?, &audit)?;
    assert!(store.unconfirmed_poll_intent(&machine)?.is_none());
    assert!(
        store
            .unconfirmed_poll_intent(&Identifier::new("other")?)?
            .is_none()
    );
    store.begin_poll_intent(&machine, &reason)?;
    assert_eq!(
        store
            .unconfirmed_poll_intent(&machine)?
            .ok_or("missing new poll")?
            .parked,
        901
    );
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM parked WHERE id<901", [], |row| row
                .get::<_, i64>(0))?,
        134
    );
    assert!(store.db.execute("DELETE FROM poll_intents", []).is_err());
    Ok(())
}

#[test]
fn adr0003_v9_missing_custody_machine_refuses_migration_without_guessing_or_dropping_rows()
-> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("unknown-custody.db");
    let db = version8(&path)?;
    db.execute("INSERT INTO parked(evidence) VALUES (?1)",[r#"{"kind":"autolyser_poll_custody","reason":"machine identity missing","targets":[],"context":null}"#])?;
    let before = snapshot(&db)?;
    drop(db);
    assert!(matches!(
        open(&path),
        Err(Error::MigrationEvidence {
            table: "parked",
            row: 1,
            ..
        })
    ));
    let db = Connection::open(&path)?;
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))?,
        8
    );
    assert_eq!(snapshot(&db)?, before);
    Ok(())
}

#[test]
fn mutation_adr0003_migration_canonical_parking_limit_is_inclusive_and_refusal_rolls_back()
-> TestResult {
    for delta in [0, 1] {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("canonical-bound.db");
        let mut db = legacy(&path)?;
        let original = r#"["incomplete_transfer","synthetic-transfer","exact retained reason"]"#;
        db.execute("INSERT INTO parked(evidence) VALUES (?1)", [original])?;
        let expected = serde_json::json!({"kind":"incomplete_transfer","reason":"exact retained reason","targets":["synthetic-transfer"],"context":null});
        let limit = expected
            .to_string()
            .len()
            .checked_sub(delta)
            .ok_or("limit")?;
        assert!(original.len() < limit);
        let before = snapshot(&db)?;
        let result = Store::migrate_v7(&mut db, limit);
        if delta == 0 {
            result?;
            let (canonical, retained): (String, String) =
                db.query_row("SELECT evidence,original_evidence FROM parked", [], |r| {
                    Ok((r.get(0)?, r.get(1)?))
                })?;
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&canonical)?,
                expected
            );
            assert_eq!(retained, original);
        } else {
            assert!(
                matches!(result,Err(Error::MigrationEvidence{table:"parked",row:1,cause}) if matches!(*cause,Error::InvalidInput))
            );
            assert_eq!(
                db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))?,
                6
            );
            assert_eq!(snapshot(&db)?, before);
        }
    }
    Ok(())
}

fn verify_adr0003_v6_retained_columns(before: Snapshot, after: &Snapshot) -> TestResult {
    assert_eq!(
        after
            .keys()
            .filter(|name| !before.contains_key(*name))
            .cloned()
            .collect::<Vec<_>>(),
        vec!["poll_intent_history".to_owned(), "poll_intents".to_owned()]
    );
    for (table, rows) in before {
        let actual = after.get(&table).ok_or("missing retained table")?;
        assert_eq!(rows.len(), actual.len(), "{table} row count");
        for (mut old, new) in rows.into_iter().zip(actual) {
            // Only active lifecycle encodings and canonical parked evidence change.
            let state_column = match table.as_str() {
                "machine_orders" => Some(5),
                "machine_order_items" => Some(6),
                _ => None,
            };
            if let Some(column) = state_column {
                let Value::Text(encoded) = old.get(column).ok_or("missing old state")? else {
                    return Err("state was not text".into());
                };
                let decoded: String = serde_json::from_str(encoded)?;
                *old.get_mut(column).ok_or("missing state")? = Value::Text(decoded);
            }
            if table == "parked" {
                assert_eq!(new.get(3), old.get(1), "original parked JSON is exact");
                *old.get_mut(1).ok_or("missing evidence")? =
                    new.get(1).ok_or("missing canonical evidence")?.clone();
            }
            assert_eq!(
                Some(old.as_slice()),
                new.get(..old.len()),
                "{table} original columns"
            );
        }
    }
    Ok(())
}

type LegacyOrder = (
    i64,
    String,
    OrderState,
    &'static str,
    Option<&'static str>,
    Option<(&'static str, &'static str)>,
);

fn seed_adr0003_v6_order_states(
    db: &Connection,
) -> Result<(Vec<LegacyOrder>, i64), Box<dyn std::error::Error>> {
    let states = [
        OrderState::Sending,
        OrderState::Sent,
        OrderState::Accepted,
        OrderState::Done,
        OrderState::Rejected,
        OrderState::CancelRequested,
        OrderState::CancelConfirmed,
        OrderState::Uncertain,
    ];
    // Include namespace colons and the old cancellation suffix convention. An empty
    // suffix never matched any valid owner, but its version still prevented replay.
    let owners = [
        ("peer-evidence", None, None),
        ("host-reserved", None, None),
        ("host-reserved:connection", Some("connection"), None),
        (
            "host-reserved:connection:part",
            Some("connection:part"),
            None,
        ),
        ("app-cancel:1:connection", None, Some(("connection", "1"))),
        (
            "app-cancel:18446744073709551615:connection:part",
            None,
            Some(("part", "18446744073709551615")),
        ),
        ("app-cancel:7:connection:", None, Some(("", "7"))),
    ];
    let mut expected_orders = Vec::new();
    for state in states {
        for (code, reservation_owner, cancellation) in owners {
            let order = format!("order-{}", expected_orders.len());
            db.execute("INSERT INTO machine_orders(destination,order_id,machine,version,state,created_at,updated_at,source_code) VALUES ('app',?1,'machine','1',?2,100,101,?3)", params![order, key(&state)?, code])?;
            let id = db.last_insert_rowid();
            let history = serde_json::json!({"id":id,"key":{"destination":"app","order":order,"machine":"machine"},"version":1,"state":state,"created_at":100,"updated_at":101,"source_code":code}).to_string();
            db.execute(
                "INSERT INTO machine_order_history(machine_order,evidence) VALUES (?1,?2)",
                params![id, history],
            )?;
            db.execute(
                "INSERT INTO machine_order_items VALUES (?1,1,'[]',0,?2,NULL,?3)",
                params![id, format!("{id}-1"), key(&state)?],
            )?;
            let item = serde_json::json!({"machine_order":id,"index":1,"tests":[],"coverage_known":false,"placer":format!("{id}-1"),"state":state,"updated_at":101,"source_code":code}).to_string();
            db.execute("INSERT INTO machine_order_item_history(machine_order,item_index,evidence) VALUES (?1,1,?2)",params![id,item])?;
            expected_orders.push((id, order, state, code, reservation_owner, cancellation));
        }
    }
    db.execute(
        "UPDATE sqlite_sequence SET seq=1000 WHERE name='machine_orders'",
        [],
    )?;

    Ok((expected_orders, i64::try_from(owners.len())?))
}

fn verify_adr0003_v6_order_states(
    store: &mut Store,
    expected_orders: Vec<LegacyOrder>,
) -> TestResult {
    for (id, order, state, code, reservation_owner, cancellation) in expected_orders {
        let key = MachineOrderKey {
            destination: Identifier::new("app")?,
            order: Identifier::new(&order)?,
            machine: Identifier::new("machine")?,
        };
        let retained = store.machine_order(&key)?.ok_or("missing reservation")?;
        assert_eq!(retained.state, state);
        assert_eq!(retained.source_code.as_str(), code);
        assert_eq!(retained.items.first().ok_or("missing item")?.state, state);
        let sql_state: SqlOrderState =
            store
                .db
                .query_row("SELECT state FROM machine_orders WHERE id=?1", [id], |r| {
                    r.get(0)
                })?;
        assert_eq!(sql_state.0, state);
        let item_state: SqlOrderState = store.db.query_row(
            "SELECT state FROM machine_order_items WHERE machine_order=?1",
            [id],
            |r| r.get(0),
        )?;
        assert_eq!(item_state.0, state);
        for owner in ["connection", "connection:part", "part", "other"] {
            let identifier = Identifier::new(owner)?;
            assert_eq!(
                retained.reserved_by(&identifier),
                state == OrderState::Sending && reservation_owner == Some(owner)
            );
            assert_eq!(
                retained.cancel_requested_by(&identifier),
                state == OrderState::CancelRequested
                    && cancellation.is_some_and(|(connection, _)| connection == owner)
            );
        }
        let version: Option<String> = store.db.query_row(
            "SELECT cancel_version FROM machine_order_history WHERE machine_order=?1",
            [id],
            |r| r.get(0),
        )?;
        assert_eq!(version.as_deref(), cancellation.map(|(_, version)| version));
        assert!(matches!(
            store.reserve_send(
                &OrderReservation {
                    identity: &key,
                    version: NonZeroU64::MIN,
                    now: 102,
                    freshness: 30
                },
                ReserveOptions::default()
            )?,
            Err(SendRefusal::AlreadyReserved)
        ));
    }
    Ok(())
}

type LegacyParking = (
    i64,
    String,
    Option<&'static str>,
    &'static str,
    String,
    &'static str,
    serde_json::Value,
);

fn seed_adr0003_v6_parking(
    db: &Connection,
    receipt_id: ReceiptId,
    expected_deliveries: &[(DeliveryId, DeliveryState, DeliveryKind)],
) -> Result<Vec<LegacyParking>, Box<dyn std::error::Error>> {
    let encoded_target = key(&receipt_id)?;
    let target = serde_json::from_str::<String>(&encoded_target)?;
    let encoded_delivery_target = key(&expected_deliveries
        .first()
        .ok_or("missing delivery fixture")?
        .0)?;
    let delivery_target = serde_json::from_str::<String>(&encoded_delivery_target)?;
    let parked = [
        (
            serde_json::json!([
                "interpretation_failure",
                encoded_target,
                "legacy parse failure"
            ]),
            "interpretation_failure",
            target.as_str(),
            "legacy parse failure",
            serde_json::Value::Null,
        ),
        (
            serde_json::json!(["result_reconciliation", encoded_target, 1, "Unresolved"]),
            "result_reconciliation",
            target.as_str(),
            "result reconciliation: Unresolved",
            serde_json::json!({"Result":{"version":null,"index":1}}),
        ),
        (
            serde_json::json!([
                "interpretation_failure",
                encoded_target,
                "invalid syntax",
                3
            ]),
            "interpretation_failure",
            target.as_str(),
            "invalid syntax",
            serde_json::json!({"Interpretation":{"version":3}}),
        ),
        (
            serde_json::json!(["result_reconciliation", encoded_target, 2, "Conflict", 4]),
            "result_reconciliation",
            target.as_str(),
            "result reconciliation: Conflict",
            serde_json::json!({"Result":{"version":4,"index":2}}),
        ),
        (
            serde_json::json!(["parked", encoded_delivery_target, "receiver refused"]),
            "parked",
            delivery_target.as_str(),
            "receiver refused",
            serde_json::Value::Null,
        ),
        (
            serde_json::json!(["conflict", encoded_delivery_target, "same identity changed"]),
            "conflict",
            delivery_target.as_str(),
            "same identity changed",
            serde_json::Value::Null,
        ),
        (
            serde_json::json!(["incomplete_transfer", "transfer", "incomplete exact reason"]),
            "incomplete_transfer",
            "transfer",
            "incomplete exact reason",
            serde_json::Value::Null,
        ),
        (
            serde_json::json!({"kind":"autolyser_poll_custody","reason":"cursor outcome unknown","targets":["machine"]}),
            "autolyser_poll_custody",
            "machine",
            "cursor outcome unknown",
            serde_json::Value::Null,
        ),
    ];
    let mut expected_parking = Vec::new();
    for (original, kind, target, reason, context) in parked {
        // Both open and already resolved rows must survive in the same format.
        for resolution in [None, Some("unchanged resolution evidence")] {
            let encoded = original.to_string();
            db.execute(
                "INSERT INTO parked(evidence,resolution) VALUES (?1,?2)",
                params![encoded, resolution],
            )?;
            expected_parking.push((
                db.last_insert_rowid(),
                encoded,
                resolution,
                kind,
                target.to_owned(),
                reason,
                context.clone(),
            ));
        }
    }
    Ok(expected_parking)
}

fn seed_adr0003_v6_deliveries(
    db: &Connection,
    receipt_id: ReceiptId,
) -> Result<Vec<(DeliveryId, DeliveryState, DeliveryKind)>, Box<dyn std::error::Error>> {
    let mut expected_deliveries = Vec::new();
    for (name, state) in [
        ("pending", DeliveryState::Pending),
        ("in_flight", DeliveryState::InFlight),
        ("done", DeliveryState::Done),
        ("parked", DeliveryState::Parked),
        ("paused", DeliveryState::Paused),
        ("conflict", DeliveryState::Conflict),
    ] {
        for (kind_name, kind) in [
            ("result", DeliveryKind::Result),
            ("order_event", DeliveryKind::OrderEvent),
        ] {
            let id = DeliveryId::try_from(uuid::Uuid::now_v7())?;
            let (receipt, version, order) = if kind == DeliveryKind::Result {
                (Some(key(&receipt_id)?), Some(1), None)
            } else {
                (None, None, Some("order"))
            };
            db.execute("INSERT INTO outbox(id,kind,receipt,version,order_id,destination,ordering_key,payload,digest,contract,state,next_attempt,enqueued_at) VALUES (?1,?2,?3,?4,?5,'app','order',x'001122',?6,1,?7,99,100)",params![key(&id)?,kind_name,receipt,version,order, digest(&[0,17,34]).0.as_slice(),name])?;
            expected_deliveries.push((id, state, kind));
        }
    }

    Ok(expected_deliveries)
}
