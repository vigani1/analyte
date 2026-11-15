use super::*;
use analyte_model::Reason;
use proptest::prelude::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

struct Space(u64);

impl FreeSpace for Space {
    fn available(&self, _: &Path) -> std::io::Result<u64> {
        Ok(self.0)
    }
}

fn limits() -> Limits {
    Limits {
        reserve_bytes: 1024,
        database_bytes: 100_000_000,
        record_bytes: 65_536,
        page_rows: 100,
        busy_timeout: Duration::from_millis(20),
    }
}

fn open(path: &Path) -> Result<Store, Error> {
    Store::open(path, limits(), Box::new(Space(u64::MAX)))
}

fn receipt(raw: &[u8]) -> Result<ReceiptMetadata, Box<dyn std::error::Error>> {
    Ok(ReceiptMetadata {
        id: ReceiptId::try_from(uuid::Uuid::now_v7())?,
        raw_digest: digest(raw),
        connection: Identifier::new("synthetic-connection")?,
        machine: Identifier::new("synthetic-machine")?,
        driver: analyte_model::NamespacedId {
            namespace: Identifier::new("synthetic-driver")?,
            value: Identifier::new("1")?,
        },
        config_version: Identifier::new("1")?,
        config_digest: digest(b"config"),
        received_at: jiff::Timestamp::UNIX_EPOCH,
    })
}

fn interpretation() -> Result<Interpretation, Box<dyn std::error::Error>> {
    Ok(Interpretation {
        driver: receipt(b"")?.driver,
        config_version: Identifier::new("1")?,
        config: b"config".to_vec(),
        outcome: Reading::Results(vec![]),
    })
}

#[test]
fn adr0003_frame_committed_before_ack_survives_restart() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("store.db");
    let transfer = Identifier::new("transfer-1")?;
    {
        let mut store = open(&path)?;
        let first = store.frame(&transfer, 1, b"frame-one")?;
        assert!(store.frame(&transfer, 2, b"frame-two")? > first);
    }
    let store = open(&path)?;
    assert_eq!(store.incomplete("")?, vec!["transfer-1"]);
    let bytes: Vec<u8> = store.db.query_row(
        "SELECT raw FROM frames ORDER BY sequence LIMIT 1",
        [],
        |r| r.get(0),
    )?;
    assert_eq!(bytes, b"frame-one");
    Ok(())
}

#[test]
fn adr0003_admission_check_matches_the_write_guard_without_writing() -> TestResult {
    let temp = tempfile::tempdir()?;
    let full = Store::open(&temp.path().join("full.db"), limits(), Box::new(Space(0)))?;
    assert!(matches!(full.admission(1), Err(Error::Capacity)));
    let roomy = open(&temp.path().join("roomy.db"))?;
    roomy.admission(1)?;
    assert_eq!(roomy.incomplete("")?, Vec::<String>::new());
    Ok(())
}

#[test]
fn adr0003_capacity_guard_refuses_receipts_and_frames() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = Store::open(&temp.path().join("store.db"), limits(), Box::new(Space(0)))?;
    let metadata = receipt(b"raw")?;
    assert!(matches!(
        store.receive(&metadata, b"raw", b"config"),
        Err(Error::Capacity)
    ));
    assert!(matches!(
        store.frame(&Identifier::new("transfer")?, 0, b"frame"),
        Err(Error::Capacity)
    ));
    assert_eq!(store.raw(metadata.id)?, None);
    Ok(())
}

#[test]
fn plan11_empty_database_migrates_and_newer_schema_is_refused() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("store.db");
    let store = open(&path)?;
    assert_eq!(
        store
            .db
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))?,
        2
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
    store.db.execute_batch("PRAGMA user_version=3")?;
    drop(store);
    assert!(matches!(open(&path), Err(Error::SchemaTooNew(3))));
    Ok(())
}

#[test]
fn adr0002_0003_receipts_and_reinterpretations_are_immutable() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    let metadata = receipt(b"raw\0exact")?;
    store.receive(&metadata, b"raw\0exact", b"config")?;
    assert!(matches!(
        store.receive(&metadata, b"raw\0exact", b"config"),
        Err(Error::Conflict)
    ));
    let reading = interpretation()?;
    assert_eq!(store.interpret(metadata.id, &reading, &[])?, 1);
    assert_eq!(store.interpret(metadata.id, &reading, &[])?, 2);
    assert!(
        store
            .db
            .execute("UPDATE receipts SET raw=x'00'", [])
            .is_err()
    );
    assert!(store.db.execute("DELETE FROM interpretations", []).is_err());
    assert_eq!(store.raw(metadata.id)?, Some(b"raw\0exact".to_vec()));
    Ok(())
}

#[test]
fn adr0008_outbox_and_interpretation_rollback_together() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    let metadata = receipt(b"raw")?;
    store.receive(&metadata, b"raw", b"config")?;
    // A deliberately failing trigger simulates an outbox constraint failure after
    // interpretation insertion, proving that the whole transaction rolls back.
    store.db.execute_batch("CREATE TRIGGER reject_interpretation AFTER INSERT ON interpretations BEGIN INSERT INTO outbox(id,receipt,version,destination,ordering_key,payload,digest,contract,state,next_attempt) VALUES ('x',NEW.receipt,NEW.version,'d','k',x'00',x'00',1,'invalid',0); END;")?;
    assert!(
        store
            .interpret(metadata.id, &interpretation()?, &[])
            .is_err()
    );
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM interpretations", [], |r| r
                .get::<_, i64>(0))?,
        0
    );
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM outbox", [], |r| r.get::<_, i64>(0))?,
        0
    );
    Ok(())
}

fn seed_delivery(
    store: &Store,
    receipt: ReceiptId,
    id: DeliveryId,
    ordering_key: &str,
) -> Result<(), Error> {
    store.db.execute("INSERT INTO outbox(id,receipt,version,destination,ordering_key,payload,digest,contract,state,next_attempt) VALUES (?1,?2,1,'app',?3,x'001122',?4,1,'pending',0)",params![key(&id)?,key(&receipt)?,ordering_key,digest(&[0,17,34]).0.as_slice()])?;
    Ok(())
}

#[test]
fn adr0008_parked_head_blocks_correction_but_not_other_machine() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    let metadata = receipt(b"raw")?;
    store.receive(&metadata, b"raw", b"config")?;
    store.interpret(metadata.id, &interpretation()?, &[])?;
    let first = DeliveryId::try_from(uuid::Uuid::now_v7())?;
    let correction = DeliveryId::try_from(uuid::Uuid::now_v7())?;
    let other = DeliveryId::try_from(uuid::Uuid::now_v7())?;
    seed_delivery(&store, metadata.id, first, "machine-a/observation")?;
    seed_delivery(&store, metadata.id, correction, "machine-a/observation")?;
    seed_delivery(&store, metadata.id, other, "machine-b/observation")?;
    let claim = store.claim(0)?.ok_or(Error::Corruption)?;
    assert_eq!(claim.id, first);
    assert_eq!(claim.payload, vec![0, 17, 34]);
    store.attempted(
        first,
        &Attempt::Parked(Reason::new("synthetic rejection")?),
        0,
    )?;
    assert_eq!(store.claim(0)?.ok_or(Error::Corruption)?.id, other);
    assert!(store.claim(0)?.is_none());
    Ok(())
}

#[test]
fn adr0004_order_cache_distinguishes_empty_stale_reset_and_tombstones() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    let destination = Identifier::new("app")?;
    let sample = SampleId::new("00012")?;
    assert!(matches!(
        store.orders(&destination, &sample, 10, 5)?,
        Lookup::NotTrusted
    ));
    let mut page = OrderPage {
        destination: destination.clone(),
        cursor: Identifier::new("cursor")?,
        complete: false,
        polled: 10,
        orders: vec![CachedOrder {
            id: Identifier::new("order")?,
            sample: sample.clone(),
            document: "synthetic-v1".into(),
            tombstone: false,
            expiry: 30,
        }],
    };
    store.order_page(&page)?;
    assert!(matches!(
        store.orders(&destination, &sample, 10, 5)?,
        Lookup::NotTrusted
    ));
    page.complete = true;
    store.order_page(&page)?;
    assert!(matches!(
        store.orders(&destination, &sample, 10, 5)?,
        Lookup::Orders(_)
    ));
    assert!(matches!(
        store.orders(&destination, &sample, 15, 5)?,
        Lookup::NotTrusted
    ));
    assert!(matches!(
        store.orders(&destination, &sample, 9, 5)?,
        Lookup::NotTrusted
    ));
    for order in &mut page.orders {
        order.tombstone = true;
    }
    store.order_page(&page)?;
    assert!(matches!(
        store.orders(&destination, &sample, 10, 5)?,
        Lookup::NoOrders
    ));
    store.reset_orders(&destination)?;
    assert!(matches!(
        store.orders(&destination, &sample, 10, 5)?,
        Lookup::NotTrusted
    ));
    Ok(())
}

#[test]
fn rule2_parse_failure_parks_and_resolution_is_audited_once() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    let metadata = receipt(b"invalid")?;
    store.receive(&metadata, b"invalid", b"config")?;
    let mut reading = interpretation()?;
    reading.outcome = Reading::Failed(Reason::new("invalid syntax")?);
    store.interpret(metadata.id, &reading, &[])?;
    let audit = Audit {
        actor: Identifier::new("operator")?,
        action: Identifier::new("resolve")?,
        targets: vec![Identifier::new("parked:1")?],
        time: 1,
        reason: Reason::new("reconciled using synthetic evidence")?,
    };
    store.resolve(1, &audit)?;
    assert!(matches!(store.resolve(1, &audit), Err(Error::Conflict)));
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM audit", [], |r| r.get::<_, i64>(0))?,
        1
    );
    assert!(store.db.execute("DELETE FROM audit", []).is_err());
    Ok(())
}

#[test]
fn plan11_online_backup_restores_exact_receipts() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    let metadata = receipt(b"protected\0bytes")?;
    store.receive(&metadata, b"protected\0bytes", b"config")?;
    let target = temp.path().join("backup.db");
    store.backup(
        &target,
        Instant::now()
            .checked_add(Duration::from_secs(2))
            .ok_or(Error::Timeout)?,
    )?;
    let restored = open(&target)?;
    assert_eq!(
        restored.raw(metadata.id)?,
        Some(b"protected\0bytes".to_vec())
    );
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]
    #[test]
    fn rule3_random_operations_never_lose_or_change_receipt_bytes(operations in prop::collection::vec(prop::collection::vec(any::<u8>(),0..128),1..20)) {
        let temp=tempfile::tempdir()?; let path=temp.path().join("store.db"); let mut store=open(&path)?;
        let mut saved=Vec::new();
        for raw in operations {
            let metadata=receipt(&raw).map_err(|e| TestCaseError::fail(e.to_string()))?; store.receive(&metadata,&raw,b"config")?;
            prop_assert!(store.receive(&metadata,&raw,b"config").is_err());
            saved.push((metadata.id,raw));
            drop(store); store=open(&path)?;
            for (id,raw) in &saved { let stored=store.raw(*id)?; prop_assert_eq!(stored.as_ref(),Some(raw)); }
        }
    }
}

#[test]
fn adr0004_reset_snapshot_excludes_orders_absent_from_replacement() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    let destination = Identifier::new("app")?;
    let sample = SampleId::new("00012")?;
    let mut page = OrderPage {
        destination: destination.clone(),
        cursor: Identifier::new("first")?,
        complete: true,
        polled: 10,
        orders: vec![CachedOrder {
            id: Identifier::new("old")?,
            sample: sample.clone(),
            document: "old".into(),
            tombstone: false,
            expiry: 100,
        }],
    };
    store.order_page(&page)?;
    store.reset_orders(&destination)?;
    page.orders.clear();
    page.cursor = Identifier::new("replacement")?;
    store.order_page(&page)?;
    assert!(matches!(
        store.orders(&destination, &sample, 10, 5)?,
        Lookup::NoOrders
    ));
    assert_eq!(store.cursor(&destination)?, Some("replacement".into()));
    Ok(())
}

fn production_result(
    receipt: ReceiptId,
) -> Result<analyte_model::ResultContent, Box<dyn std::error::Error>> {
    use analyte_model::*;
    let name = NamespacedId {
        namespace: Identifier::new("synthetic")?,
        value: Identifier::new("test")?,
    };
    Ok(ResultContent {
        receipt,
        sample: Some(SampleIdentity {
            id: SampleId::new("000123")?,
            source: SampleSource::ScannedByMachine,
            position: None,
        }),
        processing: ProcessingMode::Production,
        identity: IdentityStatus::Resolved(ResultIdentity {
            observation: ObservationIdentity {
                code: name.clone(),
                sub_id: SourceText::new("1")?,
                run: SourceText::new("")?,
                order: name.clone(),
            },
            revision: Revision(0),
        }),
        parts: NonEmptyEvidenceList::new(EvidenceList::new(vec![ResultPart {
            test: name,
            values: EvidenceList::new(vec![ResultValue {
                original: SourceText::new("positive")?,
                parsed: ValueInterpretation::Text,
            }])?,
            units: None,
            dilution: None,
            flags: EvidenceList::new(vec![])?,
            reference_range: None,
            comments: EvidenceList::new(vec![])?,
            qc_class: QcClass::Patient,
            source_status: SourceText::new("F")?,
            status: ResultStatus::Final,
            source_path: EvidenceList::new(vec![])?,
            times: EvidenceList::new(vec![])?,
            received_at: jiff::Timestamp::UNIX_EPOCH,
        }])?)?,
    })
}

#[test]
fn adr0008_public_enqueue_is_atomic_and_retry_keeps_id_and_bytes() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    let metadata = receipt(b"raw")?;
    store.receive(&metadata, b"raw", b"config")?;
    let mut evidence = interpretation()?;
    evidence.outcome = Reading::Results(vec![production_result(metadata.id)?]);
    let delivery = Delivery {
        id: DeliveryId::try_from(uuid::Uuid::now_v7())?,
        destination: Identifier::new("app")?,
        ordering_key: OrderingKey {
            machine: Identifier::new("synthetic-machine")?,
            observation: match production_result(metadata.id)?.identity {
                analyte_model::IdentityStatus::Resolved(id) => id.observation,
                analyte_model::IdentityStatus::Unresolved(_) => {
                    return Err(Error::InvalidInput.into());
                }
            },
        },
        payload: b"exact wire bytes".to_vec(),
        contract_version: 1,
    };
    let id = delivery.id;
    assert_eq!(
        store.interpret(metadata.id, &evidence, std::slice::from_ref(&delivery))?,
        1
    );
    assert!(
        store
            .interpret(metadata.id, &evidence, std::slice::from_ref(&delivery))
            .is_err()
    );
    assert!(store.interpretation(metadata.id, 2)?.is_none());
    let first = store.claim(0)?.ok_or(Error::Corruption)?;
    store.attempted(id, &Attempt::RetryAt(10), 0)?;
    assert!(store.claim(9)?.is_none());
    let retry = store.claim(10)?.ok_or(Error::Corruption)?;
    assert_eq!(first.id, retry.id);
    assert_eq!(first.payload, retry.payload);
    assert!(
        store
            .db
            .execute("UPDATE outbox SET payload=x'00'", [])
            .is_err()
    );
    store.attempted(id, &Attempt::Done, 10)?;
    assert!(store.claim(20)?.is_none());
    Ok(())
}

#[test]
fn rule7_worker_roundtrip_expired_requests_and_shutdown_joins() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("store.db");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    runtime.block_on(async {
        let deadline = Instant::now()
            .checked_add(Duration::from_secs(2))
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
        .checked_add(Duration::from_secs(2))
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
fn adr0003_complete_and_aborted_transfers_keep_all_evidence() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    let complete = Identifier::new("complete")?;
    let aborted = Identifier::new("aborted")?;
    store.frame(&complete, 1, b"first")?;
    store.frame(&aborted, 1, b"partial")?;
    let metadata = receipt(b"first")?;
    store.receive(&metadata, b"first", b"config")?;
    store.finish_transfer(&complete, Some(metadata.id))?;
    store.finish_transfer(&aborted, None)?;
    assert_eq!(store.incomplete("")?, vec!["aborted"]);
    assert_eq!(
        store
            .frames(&complete, 0)?
            .first()
            .map(|f| f.raw.as_slice()),
        Some(b"first".as_slice())
    );
    assert_eq!(
        store.frames(&aborted, 0)?.first().map(|f| f.raw.as_slice()),
        Some(b"partial".as_slice())
    );
    assert!(matches!(
        store.frame(&complete, 2, b"late"),
        Err(Error::Conflict)
    ));
    assert_eq!(
        store.receipt(metadata.id)?.ok_or(Error::Corruption)?.config,
        b"config"
    );
    Ok(())
}

#[test]
fn adr0004_failed_page_never_advances_its_cursor() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    let destination = Identifier::new("app")?;
    let mut page = OrderPage {
        destination: destination.clone(),
        cursor: Identifier::new("first")?,
        complete: true,
        polled: 1,
        orders: vec![],
    };
    store.order_page(&page)?;
    store.db.execute_batch("CREATE TRIGGER reject_order BEFORE INSERT ON orders BEGIN SELECT RAISE(ABORT,'synthetic failure'); END;")?;
    page.cursor = Identifier::new("must-not-commit")?;
    page.orders.push(CachedOrder {
        id: Identifier::new("order")?,
        sample: SampleId::new("0001")?,
        document: "synthetic".into(),
        tombstone: false,
        expiry: 100,
    });
    assert!(store.order_page(&page).is_err());
    assert_eq!(store.cursor(&destination)?, Some("first".into()));
    Ok(())
}

#[test]
fn plan11_corruption_and_database_size_limit_refuse_visibly() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("invalid.db");
    std::fs::write(&path, b"this is not SQLite")?;
    assert!(matches!(open(&path), Err(Error::Corruption)));
    let mut small = limits();
    small.database_bytes = 1;
    let mut store = Store::open(
        &temp.path().join("small.db"),
        small,
        Box::new(Space(u64::MAX)),
    )?;
    assert!(matches!(
        store.receive(&receipt(b"raw")?, b"raw", b"config"),
        Err(Error::Capacity)
    ));
    Ok(())
}

#[test]
fn plan11_v1_migration_preserves_rows_triggers_and_retry_history() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("v1.db");
    // Exercise the original v1 schema and APIs before opening through migration.
    let db = Connection::open(&path)?;
    db.execute_batch(include_str!("schema.sql"))?;
    for table in ["receipts", "frames", "interpretations", "attempts", "audit"] {
        db.execute_batch(&format!("CREATE TRIGGER immutable_{table}_update BEFORE UPDATE ON {table} BEGIN SELECT RAISE(ABORT,'immutable evidence'); END; CREATE TRIGGER immutable_{table}_delete BEFORE DELETE ON {table} BEGIN SELECT RAISE(ABORT,'immutable evidence'); END;"))?;
    }
    let mut v1 = Store {
        db,
        path: path.clone(),
        limits: limits(),
        space: Box::new(Space(u64::MAX)),
    };
    let metadata = receipt(b"raw")?;
    v1.receive(&metadata, b"raw", b"config")?;
    v1.interpret(metadata.id, &interpretation()?, &[])?;
    let id = DeliveryId::try_from(uuid::Uuid::now_v7())?;
    seed_delivery(&v1, metadata.id, id, "key")?;
    v1.db.execute("UPDATE outbox SET state='in_flight'", [])?;
    v1.attempted(id, &Attempt::RetryAt(10), 0)?;
    v1.frame(&Identifier::new("transfer")?, 1, b"frame")?;
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
    for (table, count) in tables.iter().zip(rows) {
        assert_eq!(
            migrated
                .db
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                    .get::<_, i64>(0))?,
            count
        );
    }
    assert_eq!(migrated.raw(metadata.id)?, Some(b"raw".to_vec()));
    assert_eq!(migrated.frames(&Identifier::new("transfer")?, 0)?.len(), 1);
    assert!(migrated.interpretation(metadata.id, 1)?.is_some());
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
fn adr0008_event_provenance_health_ordering_and_restart_attempts() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("events.db");
    let mut store = open(&path)?;
    let destination = Identifier::new("app")?;
    let event = OrderEventDelivery {
        id: DeliveryId::try_from(uuid::Uuid::now_v7())?,
        destination: destination.clone(),
        order_id: Identifier::new("order-a")?,
        payload: b"event".to_vec(),
        contract_version: 1,
    };
    store.enqueue_order_event(&event, 10)?;
    assert!(store.enqueue_order_event(&event, 10).is_err());
    let second = OrderEventDelivery {
        id: DeliveryId::try_from(uuid::Uuid::now_v7())?,
        ..event
    };
    store.enqueue_order_event(&second, 11)?;
    let other = OrderEventDelivery {
        id: DeliveryId::try_from(uuid::Uuid::now_v7())?,
        order_id: Identifier::new("order-b")?,
        ..second
    };
    store.enqueue_order_event(&other, 12)?;
    let claim = store.claim(12)?.ok_or(Error::Corruption)?;
    assert_eq!(claim.kind, DeliveryKind::OrderEvent);
    assert_eq!(claim.earlier_attempts, 0);
    store.attempted(claim.id, &Attempt::RetryAt(20), 12)?;
    assert_eq!(store.claim(12)?.ok_or(Error::Corruption)?.id, other.id);
    let health = store.health(&destination, 15)?;
    assert_eq!(
        (
            health.pending_count,
            health.pending_bytes,
            health.oldest_pending_age
        ),
        (3, 15, Some(5))
    );
    assert!(!health.paused);
    assert!(!health.cache_trusted);
    store.order_page(&OrderPage {
        destination: destination.clone(),
        cursor: Identifier::new("cursor")?,
        complete: false,
        polled: 15,
        orders: vec![],
    })?;
    let health = store.health(&destination, 15)?;
    assert_eq!(health.last_successful_poll, Some(15));
    assert!(!health.snapshot_complete);
    assert!(!health.cache_trusted);
    store.order_page(&OrderPage {
        destination: destination.clone(),
        cursor: Identifier::new("next")?,
        complete: true,
        polled: 16,
        orders: vec![],
    })?;
    assert!(store.health(&destination, 16)?.cache_trusted);
    store.reset_orders(&destination)?;
    assert!(!store.health(&destination, 16)?.cache_trusted);

    drop(store);
    let mut store = open(&path)?;
    let retry = store.claim(20)?.ok_or(Error::Corruption)?;
    assert_eq!(retry.id, event.id);
    assert_eq!(retry.earlier_attempts, 1);
    store.attempted(retry.id, &Attempt::Paused(Reason::new("credentials")?), 20)?;
    assert!(store.health(&destination, 21)?.paused);
    assert!(store.claim(21)?.is_none());
    // CHECK constraints reject both kinds carrying the other's provenance.
    assert!(store.db.execute("INSERT INTO outbox(id,kind,receipt,version,order_id,destination,ordering_key,payload,digest,contract,state,next_attempt) VALUES ('bad','order_event','receipt',1,'o','app','o',x'00',x'00',1,'pending',0)",[]).is_err());
    assert!(store.db.execute("INSERT INTO outbox(id,kind,receipt,version,order_id,destination,ordering_key,payload,digest,contract,state,next_attempt) VALUES ('bad','result',NULL,NULL,'o','app','o',x'00',x'00',1,'pending',0)",[]).is_err());
    Ok(())
}
