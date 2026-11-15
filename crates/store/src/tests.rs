use super::*;
use analyte_model::Reason;
use proptest::prelude::*;

pub(super) type TestResult = Result<(), Box<dyn std::error::Error>>;

pub(super) struct Space(pub(super) u64);

impl FreeSpace for Space {
    fn available(&self, _: &Path) -> std::io::Result<u64> {
        Ok(self.0)
    }
}

pub(super) fn limits() -> Limits {
    Limits {
        reserve_bytes: 1024,
        database_bytes: 100_000_000,
        record_bytes: 65_536,
        page_rows: 100,
        busy_timeout: Duration::from_millis(20),
    }
}

pub(super) fn open(path: &Path) -> Result<Store, Error> {
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
        6
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
    store.db.execute_batch("PRAGMA user_version=7")?;
    drop(store);
    assert!(matches!(open(&path), Err(Error::SchemaTooNew(7))));
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
        _lock: Store::lock(&path, false)?,
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
    let health = store.health(&destination, 15, 30)?;
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
    let health = store.health(&destination, 15, 30)?;
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
    assert!(store.health(&destination, 16, 30)?.cache_trusted);
    store.reset_orders(&destination)?;
    assert!(!store.health(&destination, 16, 30)?.cache_trusted);

    drop(store);
    let mut store = open(&path)?;
    let retry = store.claim(20)?.ok_or(Error::Corruption)?;
    assert_eq!(retry.id, event.id);
    assert_eq!(retry.earlier_attempts, 1);
    store.attempted(retry.id, &Attempt::Paused(Reason::new("credentials")?), 20)?;
    assert!(store.health(&destination, 21, 30)?.paused);
    assert!(store.claim(21)?.is_none());
    // CHECK constraints reject both kinds carrying the other's provenance.
    assert!(store.db.execute("INSERT INTO outbox(id,kind,receipt,version,order_id,destination,ordering_key,payload,digest,contract,state,next_attempt) VALUES ('bad','order_event','receipt',1,'o','app','o',x'00',x'00',1,'pending',0)",[]).is_err());
    assert!(store.db.execute("INSERT INTO outbox(id,kind,receipt,version,order_id,destination,ordering_key,payload,digest,contract,state,next_attempt) VALUES ('bad','result',NULL,NULL,'o','app','o',x'00',x'00',1,'pending',0)",[]).is_err());
    Ok(())
}

fn revised_receipt(
    store: &mut Store,
    sample: &str,
    correction: bool,
    run: &str,
) -> Result<RevisionOutcome, Box<dyn std::error::Error>> {
    let metadata = receipt(b"synthetic")?;
    store.receive(&metadata, b"synthetic", b"config")?;
    let mut result = production_result(metadata.id)?;
    let mut parts = result.parts.as_slice().to_vec();
    for part in &mut parts {
        part.received_at = jiff::Timestamp::now();
    }
    result.parts =
        analyte_model::NonEmptyEvidenceList::new(analyte_model::EvidenceList::new(parts)?)?;
    if let Some(sample_id) = &mut result.sample {
        sample_id.id = SampleId::new(sample)?;
    }
    if let analyte_model::IdentityStatus::Resolved(identity) = &mut result.identity {
        identity.observation.run = analyte_model::SourceText::new(run)?;
    }
    let mut evidence = interpretation()?;
    evidence.outcome = Reading::Results(vec![result]);
    let commit =
        store.interpret_reconciled(metadata.id, evidence, &[correction], &[], |_, _| {
            Ok(Vec::new())
        })?;
    Ok(*commit.outcomes.first().ok_or(Error::Corruption)?)
}

#[test]
fn adr0003_revisions_new_retransmission_correction_conflict_runs_and_restart() -> TestResult {
    use analyte_model::Revision;
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("revisions.db");
    let mut store = open(&path)?;
    assert_eq!(
        revised_receipt(&mut store, "001", false, "run1")?,
        RevisionOutcome::New(Revision(0))
    );
    drop(store);
    let mut store = open(&path)?;
    assert_eq!(
        revised_receipt(&mut store, "001", false, "run1")?,
        RevisionOutcome::Retransmission(Revision(0))
    );
    assert_eq!(
        revised_receipt(&mut store, "002", true, "run1")?,
        RevisionOutcome::Correction(Revision(1))
    );
    assert_eq!(
        revised_receipt(&mut store, "003", false, "run1")?,
        RevisionOutcome::Conflict
    );
    assert_eq!(store.parked(0)?.len(), 1);
    assert_eq!(
        revised_receipt(&mut store, "001", false, "run1")?,
        RevisionOutcome::Retransmission(Revision(0))
    );
    assert_eq!(
        revised_receipt(&mut store, "001", false, "run2")?,
        RevisionOutcome::New(Revision(0))
    );
    assert_eq!(
        revised_receipt(&mut store, "004", true, "correction-without-original")?,
        RevisionOutcome::New(Revision(0))
    );
    assert!(
        store
            .db
            .execute("DELETE FROM observation_revisions", [])
            .is_err()
    );
    assert!(
        store
            .db
            .execute("UPDATE observation_revisions SET revision=10", [])
            .is_err()
    );
    Ok(())
}

#[test]
fn adr0003_revision_builder_failure_rolls_back_interpretation_and_history() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("rollback.db"))?;
    let metadata = receipt(b"raw")?;
    store.receive(&metadata, b"raw", b"config")?;
    let mut evidence = interpretation()?;
    evidence.outcome = Reading::Results(vec![production_result(metadata.id)?]);
    assert!(matches!(
        store.interpret_reconciled(metadata.id, evidence, &[false], &[], |_, _| Err(
            Error::InvalidInput
        )),
        Err(Error::InvalidInput)
    ));
    assert!(store.interpretation(metadata.id, 1)?.is_none());
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM observation_revisions", [], |r| r
                .get::<_, i64>(0))?,
        0
    );
    Ok(())
}

#[test]
fn plan11_v2_to_v3_migration_retains_every_row_and_trigger() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("v2.db");
    let mut db = Connection::open(&path)?;
    db.execute_batch(include_str!("schema.sql"))?;
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
    v2.frame(&Identifier::new("legacy-transfer")?, 1, b"legacy-frame")?;
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
        6
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
fn adr0003_withdrawal_assigns_next_revision_and_retransmission_enqueues_nothing() -> TestResult {
    use analyte_model::*;
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("withdrawal.db"))?;
    for (withdrawn, expected) in [
        (false, RevisionOutcome::New(Revision(0))),
        (true, RevisionOutcome::Correction(Revision(1))),
        (true, RevisionOutcome::Retransmission(Revision(1))),
    ] {
        let metadata = receipt(b"raw")?;
        store.receive(&metadata, b"raw", b"config")?;
        let mut result = production_result(metadata.id)?;
        if withdrawn {
            let mut parts = result.parts.as_slice().to_vec();
            for part in &mut parts {
                part.status = ResultStatus::Withdrawn;
                part.source_status = SourceText::new("D")?;
            }
            result.parts = NonEmptyEvidenceList::new(EvidenceList::new(parts)?)?;
        }
        let mut evidence = interpretation()?;
        evidence.outcome = Reading::Results(vec![result]);
        let commit = store.interpret_reconciled(
            metadata.id,
            evidence,
            &[false],
            &[],
            |evidence, outcomes| {
                let Reading::Results(results) = &evidence.outcome else {
                    return Err(Error::InvalidInput);
                };
                if matches!(outcomes.first(), Some(RevisionOutcome::Retransmission(_))) {
                    return Ok(Vec::new());
                }
                let Some(IdentityStatus::Resolved(identity)) = results.first().map(|r| &r.identity)
                else {
                    return Err(Error::InvalidInput);
                };
                Ok(vec![Delivery {
                    id: DeliveryId::try_from(uuid::Uuid::now_v7())
                        .map_err(|_| Error::InvalidInput)?,
                    destination: Identifier::new("app").map_err(|_| Error::InvalidInput)?,
                    ordering_key: OrderingKey {
                        machine: metadata.machine.clone(),
                        observation: identity.observation.clone(),
                    },
                    payload: format!("revision:{}", identity.revision.0).into_bytes(),
                    contract_version: 1,
                }])
            },
        )?;
        assert_eq!(commit.outcomes, vec![expected]);
    }
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM outbox", [], |r| r.get::<_, i64>(0))?,
        2
    );
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM observation_revisions", [], |r| r
                .get::<_, i64>(0))?,
        2
    );
    Ok(())
}

#[test]
fn adr0008_recovery_audits_interrupted_attempts_and_preserves_blocked_items() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("recover.db");
    let mut store = open(&path)?;
    let metadata = receipt(b"raw")?;
    store.receive(&metadata, b"raw", b"config")?;
    store.interpret(metadata.id, &interpretation()?, &[])?;
    let mut interrupted = None;
    for state in ["in_flight", "parked", "conflict", "paused", "done"] {
        let id = DeliveryId::try_from(uuid::Uuid::now_v7())?;
        seed_delivery(&store, metadata.id, id, state)?;
        store.db.execute(
            "UPDATE outbox SET state=?2 WHERE id=?1",
            params![key(&id)?, state],
        )?;
        if state == "in_flight" {
            interrupted = Some(id);
        }
    }
    drop(store);
    let mut store = open(&path)?;
    let audit = Audit {
        actor: Identifier::new("startup")?,
        action: Identifier::new("recover")?,
        targets: vec![],
        time: 42,
        reason: Reason::new("uncertain (interrupted); ADR 0008 idempotent receiver")?,
    };
    assert_eq!(store.recover_in_flight(42, &audit)?, 1);
    assert_eq!(store.recover_in_flight(42, &audit)?, 0);
    let id = interrupted.ok_or("missing interrupted ID")?;
    let attempts: (String, i64) = store.db.query_row(
        "SELECT outcome,time FROM attempts WHERE delivery=?1",
        [key(&id)?],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    assert_eq!(attempts, (key(&Attempt::Interrupted)?, 42));
    let audits = store.audits(0)?;
    let (_, entry) = audits.first().ok_or("missing audit")?;
    assert_eq!(
        entry.targets,
        vec![Identifier::new(&uuid::Uuid::from(id).to_string())?]
    );
    assert!(entry.reason.as_str().contains("ADR 0008"));
    for state in ["parked", "conflict", "paused", "done"] {
        let count: i64 =
            store
                .db
                .query_row("SELECT count(*) FROM outbox WHERE state=?1", [state], |r| {
                    r.get(0)
                })?;
        assert_eq!(count, 1);
    }
    // A paused destination blocks even a recovered item until operator repair.
    assert!(store.claim(42)?.is_none());
    store
        .db
        .execute("UPDATE outbox SET state='done' WHERE state='paused'", [])?;
    let claim = store.claim(42)?.ok_or("not recovered")?;
    assert_eq!(claim.id, id);
    assert_eq!(claim.payload, [0, 17, 34]);
    assert_eq!(claim.earlier_attempts, 1);
    Ok(())
}

#[test]
fn plan11_restore_refuses_corrupt_and_newer_backup_without_changing_current() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("store.db");
    let source = temp.path().join("backup.db");
    let mut store = open(&path)?;
    let metadata = receipt(b"retained")?;
    store.receive(&metadata, b"retained", b"config")?;
    drop(store);
    let audit = Audit {
        actor: Identifier::new("operator")?,
        action: Identifier::new("store-restore")?,
        targets: vec![],
        time: 0,
        reason: Reason::new("recovery")?,
    };
    std::fs::write(&source, b"not SQLite")?;
    assert!(matches!(
        Store::restore(
            &path,
            &source,
            limits(),
            Box::new(Space(u64::MAX)),
            &audit,
            Instant::now() + Duration::from_secs(5)
        ),
        Err(Error::Corruption)
    ));
    std::fs::remove_file(&source)?;
    Connection::open(&source)?.execute_batch("PRAGMA user_version=999")?;
    assert!(matches!(
        Store::restore(
            &path,
            &source,
            limits(),
            Box::new(Space(u64::MAX)),
            &audit,
            Instant::now() + Duration::from_secs(5)
        ),
        Err(Error::SchemaTooNew(999))
    ));
    assert_eq!(open(&path)?.raw(metadata.id)?, Some(b"retained".to_vec()));
    assert!(!path.with_extension("before-restore").exists());
    Ok(())
}

#[test]
fn plan11_interrupted_restore_staging_refuses_startup_without_deleting_evidence() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("store.db");
    std::fs::write(path.with_extension("restore-pending"), b"interrupted stage")?;
    assert!(matches!(open(&path), Err(Error::Conflict)));
    assert!(!path.exists());
    assert_eq!(
        std::fs::read(path.with_extension("restore-pending"))?,
        b"interrupted stage"
    );
    Ok(())
}

#[test]
fn plan11_adr0008_resend_preserves_identity_bytes_and_atomic_audit() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("resend.db"))?;
    let metadata = receipt(b"raw")?;
    store.receive(&metadata, b"raw", b"config")?;
    let result = production_result(metadata.id)?;
    let observation = match &result.identity {
        analyte_model::IdentityStatus::Resolved(id) => id.observation.clone(),
        analyte_model::IdentityStatus::Unresolved(_) => return Err("fixture identity".into()),
    };
    let mut evidence = interpretation()?;
    evidence.outcome = Reading::Results(vec![result]);
    let delivery = Delivery {
        id: DeliveryId::try_from(uuid::Uuid::now_v7())?,
        destination: Identifier::new("app")?,
        ordering_key: OrderingKey {
            machine: metadata.machine.clone(),
            observation,
        },
        payload: b"immutable synthetic bytes".to_vec(),
        contract_version: 1,
    };
    store.interpret(metadata.id, &evidence, std::slice::from_ref(&delivery))?;
    let mut audit = Audit {
        actor: Identifier::new("operator")?,
        action: Identifier::new("delivery-resend")?,
        targets: vec![Identifier::new(&format!(
            "delivery:{}",
            uuid::Uuid::from(delivery.id)
        ))?],
        time: 50,
        reason: Reason::new("app data recovered; resend requested")?,
    };
    assert!(matches!(
        store.resend_delivery(delivery.id, false, &audit),
        Err(Error::ResendState)
    ));
    let first = store.claim(0)?.ok_or("missing claim")?;
    assert!(matches!(
        store.resend_delivery(delivery.id, false, &audit),
        Err(Error::ResendState)
    ));
    store.attempted(delivery.id, &Attempt::Done, 0)?;
    // An audit write failure must roll back the requeue, not just the audit.
    store.db.execute_batch("CREATE TRIGGER reject_resend_audit BEFORE INSERT ON audit BEGIN SELECT RAISE(ABORT,'synthetic audit failure'); END;")?;
    assert!(store.resend_delivery(delivery.id, false, &audit).is_err());
    assert_eq!(
        store
            .deliveries(0)?
            .first()
            .ok_or("missing delivery")?
            .state,
        DeliveryState::Done
    );
    store
        .db
        .execute_batch("DROP TRIGGER reject_resend_audit;")?;
    for outcome in [
        Attempt::Done,
        Attempt::Parked(Reason::new("rejection")?),
        Attempt::Paused(Reason::new("setup")?),
    ] {
        store.resend_delivery(delivery.id, false, &audit)?;
        let retry = store.claim(50)?.ok_or("missing retry")?;
        assert_eq!(retry.id, first.id);
        assert_eq!(retry.payload, first.payload);
        store.attempted(retry.id, &outcome, 50)?;
    }
    assert_eq!(store.audits(0)?.len(), 3);
    assert_eq!(
        store.audits(0)?.first().ok_or("missing audit")?.1.targets,
        audit.targets
    );
    assert!(
        store
            .deliveries(
                store
                    .deliveries(0)?
                    .first()
                    .ok_or("missing delivery")?
                    .sequence
            )?
            .is_empty()
    );
    let unknown = DeliveryId::try_from(uuid::Uuid::now_v7())?;
    audit.targets = vec![Identifier::new(&format!(
        "delivery:{}",
        uuid::Uuid::from(unknown)
    ))?];
    assert!(matches!(
        store.resend_delivery(unknown, false, &audit),
        Err(Error::UnknownDelivery)
    ));
    assert_eq!(store.audits(0)?.len(), 3);
    Ok(())
}

#[test]
fn plan11_adr0003_superseded_resend_requires_explicit_delivery() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("superseded.db"))?;
    let metadata = receipt(b"raw")?;
    store.receive(&metadata, b"raw", b"config")?;
    let result = production_result(metadata.id)?;
    let observation = match &result.identity {
        analyte_model::IdentityStatus::Resolved(id) => id.observation.clone(),
        analyte_model::IdentityStatus::Unresolved(_) => return Err("fixture identity".into()),
    };
    let mut evidence = interpretation()?;
    evidence.outcome = Reading::Results(vec![result]);
    let delivery = Delivery {
        id: DeliveryId::try_from(uuid::Uuid::now_v7())?,
        destination: Identifier::new("app")?,
        ordering_key: OrderingKey {
            machine: metadata.machine.clone(),
            observation,
        },
        payload: b"old bytes".to_vec(),
        contract_version: 1,
    };
    store.interpret(metadata.id, &evidence, std::slice::from_ref(&delivery))?;
    store.claim(0)?.ok_or("missing claim")?;
    store.attempted(delivery.id, &Attempt::Done, 0)?;
    store.db.execute(
        "INSERT INTO interpretations VALUES (?1,2,?2)",
        params![key(&metadata.id)?, key(&evidence)?],
    )?;
    // A correction need not have an outbox row to supersede the older revision.
    let identity = key(&(
        &delivery.ordering_key.machine,
        &delivery.ordering_key.observation,
    ))?;
    store.db.execute(
        "INSERT INTO observation_revisions VALUES (?1,1,?2,2,?3)",
        params![
            identity,
            key(&metadata.id)?,
            digest(b"corrected").0.as_slice()
        ],
    )?;
    let audit = Audit {
        actor: Identifier::new("operator")?,
        action: Identifier::new("delivery-resend")?,
        targets: vec![Identifier::new(&format!(
            "delivery:{}",
            uuid::Uuid::from(delivery.id)
        ))?],
        time: 50,
        reason: Reason::new("explicit old revision recovery")?,
    };
    assert!(matches!(
        store.resend_delivery(delivery.id, false, &audit),
        Err(Error::SupersededDelivery)
    ));
    assert!(store.audits(0)?.is_empty());
    store.resend_delivery(delivery.id, true, &audit)?;
    assert_eq!(
        store.claim(50)?.ok_or("missing resend")?.payload,
        delivery.payload
    );
    assert_eq!(store.audits(0)?.len(), 1);
    Ok(())
}

#[test]
fn plan11_adr0003_reprocess_provenance_resolution_failure_and_audit_rollback() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("reprocess.db"))?;
    let metadata = receipt(b"original")?;
    store.receive(&metadata, b"original", b"config")?;
    let mut original = interpretation()?;
    original.outcome = Reading::Failed(Reason::new("test mapping missing")?);
    store.interpret(metadata.id, &original, &[])?;
    let before: String = store
        .db
        .query_row("SELECT evidence FROM interpretations", [], |r| r.get(0))?;
    let parked = store.parked(0)?.first().ok_or("missing parked")?.id;
    let mut current = interpretation()?;
    current.config_version = Identifier::new("current-config")?;
    current.driver.value = Identifier::new("current-driver")?;
    current.config = b"current mapping".to_vec();
    current.outcome = Reading::Results(vec![production_result(metadata.id)?]);
    let action = Reprocess {
        receipt: metadata.id,
        parked: Some(parked),
        audit: Audit {
            actor: Identifier::new("operator")?,
            action: Identifier::new("receipt-reprocess")?,
            targets: vec![],
            time: 42,
            reason: Reason::new("test mapping repaired")?,
        },
    };
    store.db.execute_batch("CREATE TRIGGER fail_reprocess_audit BEFORE INSERT ON audit BEGIN SELECT RAISE(ABORT,'synthetic audit failure'); END;")?;
    assert!(
        store
            .reprocess_reconciled(&action, current.clone(), &[false], &[], |_, _| Ok(vec![]))
            .is_err()
    );
    assert!(store.interpretation(metadata.id, 2)?.is_none());
    assert!(
        store
            .parked_item(parked)?
            .ok_or("missing parked")?
            .resolution
            .is_none()
    );
    assert!(store.audits(0)?.is_empty());
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM observation_revisions", [], |r| r
                .get::<_, i64>(0))?,
        0
    );
    store
        .db
        .execute_batch("DROP TRIGGER fail_reprocess_audit;")?;
    let commit =
        store.reprocess_reconciled(&action, current.clone(), &[false], &[], |_, _| Ok(vec![]))?;
    assert_eq!(commit.version, 2);
    assert!(matches!(
        commit.outcomes.as_slice(),
        [RevisionOutcome::New(_)]
    ));
    let retained = store
        .interpretation(metadata.id, 2)?
        .ok_or("missing version")?;
    assert_eq!(retained.driver, current.driver);
    assert_eq!(retained.config_version, current.config_version);
    assert_eq!(retained.config, current.config);
    let wire: serde_json::Value = serde_json::from_str(&store.db.query_row(
        "SELECT evidence FROM interpretations WHERE version=2",
        [],
        |r| r.get::<_, String>(0),
    )?)?;
    assert_eq!(
        wire["config_digest"],
        serde_json::to_value(digest(&current.config))?
    );
    let after: String = store.db.query_row(
        "SELECT evidence FROM interpretations WHERE version=1",
        [],
        |r| r.get(0),
    )?;
    assert_eq!(before, after);
    assert_eq!(
        store.receipt(metadata.id)?.ok_or("missing receipt")?.raw,
        b"original"
    );
    assert_eq!(store.audits(0)?.len(), 1);
    let resolution: Audit = serde_json::from_str(
        &store
            .parked_item(parked)?
            .ok_or("missing parked")?
            .resolution
            .ok_or("not resolved")?,
    )?;
    assert_eq!(
        resolution.targets,
        store.audits(0)?.first().ok_or("missing audit")?.1.targets
    );
    assert!(
        resolution
            .targets
            .iter()
            .any(|t| t.as_str().ends_with(":2"))
    );
    assert!(matches!(
        store.reprocess_reconciled(&action, current.clone(), &[false], &[], |_, _| Ok(vec![])),
        Err(Error::ReprocessParked)
    ));
    let other = store.park(&Parked {
        kind: Identifier::new("unrelated")?,
        reason: Reason::new("other receipt")?,
        targets: vec![],
    })?;
    let mut action = action;
    action.parked = Some(other);
    assert!(matches!(
        store.reprocess_reconciled(&action, current.clone(), &[false], &[], |_, _| Ok(vec![])),
        Err(Error::ReprocessParked)
    ));
    action.parked = None;
    current.outcome = Reading::Results(vec![]);
    store.reprocess_reconciled(&action, current, &[], &[], |_, _| Ok(vec![]))?;
    assert!(
        store
            .parked(0)?
            .iter()
            .any(|p| p.evidence.contains("reprocessing produced no results"))
    );
    assert_eq!(store.audits(0)?.len(), 2);
    Ok(())
}

#[test]
fn rule3_interpretation_digest_verified_legacy_evidence_still_reads() -> TestResult {
    let evidence = interpretation()?;
    let mut encoded = serde_json::to_value(&evidence)?;
    encoded["config_digest"] = serde_json::to_value(digest(b"different"))?;
    assert!(serde_json::from_value::<Interpretation>(encoded.clone()).is_err());
    encoded
        .as_object_mut()
        .ok_or("not object")?
        .remove("config_digest");
    let legacy: Interpretation = serde_json::from_value(encoded)?;
    assert_eq!(legacy.config, evidence.config);
    Ok(())
}

// ADR 0003; POCT01-A2 B 3.2 pp.71-72: provenance offsets must not create results.
#[test]
fn adr0003_legacy_offset_digest_reconciles_after_restart_without_rewriting_evidence() -> TestResult
{
    use analyte_model::{EvidenceList, Identifier, NonEmptyEvidenceList, Revision, SourceNode};
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("legacy-offsets.db");
    let mut store = open(&path)?;
    let first = receipt(b"original")?;
    store.receive(&first, b"original", b"config")?;
    let mut result = production_result(first.id)?;
    let mut parts = result.parts.as_slice().to_vec();
    parts.first_mut().ok_or(Error::Corruption)?.source_path =
        EvidenceList::new(vec![SourceNode {
            occurrence: 5,
            kind: Identifier::new("OBS")?,
            sequence: None,
            status: None,
        }])?;
    result.parts = NonEmptyEvidenceList::new(EvidenceList::new(parts)?)?;
    let mut old_parts = serde_json::to_value(&result.parts)?;
    for part in old_parts.as_array_mut().ok_or(Error::Corruption)? {
        part.as_object_mut()
            .ok_or(Error::Corruption)?
            .remove("received_at");
    }
    // Construct the previous released algorithm's digest, including source offsets.
    let old_digest = digest(&serde_json::to_vec(&(
        &result.sample,
        result.processing,
        old_parts,
    ))?);
    let mut evidence = interpretation()?;
    evidence.outcome = Reading::Results(vec![result.clone()]);
    let analyte_model::IdentityStatus::Resolved(identity) = &result.identity else {
        return Err(Error::Corruption.into());
    };
    let receipt_key = key(&first.id)?;
    let identity_key = key(&(&first.machine, &identity.observation))?;
    store.db.execute(
        "INSERT INTO interpretations(receipt,version,evidence) VALUES (?1,1,?2)",
        params![receipt_key, key(&evidence)?],
    )?;
    store.db.execute(
        "INSERT INTO observation_revisions VALUES (?1,0,?2,1,?3)",
        params![identity_key, receipt_key, old_digest.0.as_slice()],
    )?;
    drop(store);
    let mut store = open(&path)?;
    let second = receipt(b"resent")?;
    store.receive(&second, b"resent", b"config")?;
    result.receipt = second.id;
    let mut parts = result.parts.as_slice().to_vec();
    parts.first_mut().ok_or(Error::Corruption)?.source_path =
        EvidenceList::new(vec![SourceNode {
            occurrence: 6,
            kind: Identifier::new("OBS")?,
            sequence: None,
            status: None,
        }])?;
    result.parts = NonEmptyEvidenceList::new(EvidenceList::new(parts)?)?;
    evidence.outcome = Reading::Results(vec![result]);
    let commit =
        store.interpret_reconciled(second.id, evidence, &[false], &[], |_, outcomes| {
            assert_eq!(outcomes, &[RevisionOutcome::Retransmission(Revision(0))]);
            Ok(vec![])
        })?;
    assert_eq!(
        commit.outcomes,
        vec![RevisionOutcome::Retransmission(Revision(0))]
    );
    assert_eq!(
        store.receipt(first.id)?.ok_or(Error::Corruption)?.raw,
        b"original"
    );
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM observation_revisions", [], |r| r
                .get::<_, i64>(0))?,
        1
    );
    Ok(())
}
