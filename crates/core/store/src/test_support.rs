//! Tests test support behavior with synthetic fixtures.
use crate::*;

/// Upper bound so a broken test cannot hang; never part of the behaviour under test.
pub(crate) const HANG_GUARD: Duration = Duration::from_secs(60);

pub(crate) type TestResult = Result<(), Box<dyn std::error::Error>>;

pub(crate) struct Space(pub(crate) u64);

impl FreeSpace for Space {
    fn available(&self, _: &Path) -> std::io::Result<u64> {
        Ok(self.0)
    }
}

pub(crate) fn limits() -> Limits {
    Limits {
        reserve_bytes: 1024,
        database_bytes: 100_000_000,
        record_bytes: 65_536,
        page_rows: 100,
        busy_timeout: Duration::from_millis(20),
    }
}

/// The synthetic machine model test reservations are made for (ADR 0033).
pub(crate) fn order_model() -> Identifier {
    Identifier::new("example/synthetic").expect("a valid model key")
}

pub(crate) fn open(path: &Path) -> Result<Store, MaintenanceError> {
    Store::open(path, limits(), Box::new(Space(u64::MAX)))
}

pub(crate) fn receipt(raw: &[u8]) -> Result<ReceiptMetadata, Box<dyn std::error::Error>> {
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

pub(crate) fn interpretation() -> Result<Interpretation, Box<dyn std::error::Error>> {
    Ok(Interpretation {
        departures: None,
        driver: receipt(b"")?.driver,
        config_version: Identifier::new("1")?,
        config: b"config".to_vec(),
        outcome: Reading::Results(vec![]),
    })
}

pub(crate) fn seed_delivery(
    store: &Store,
    receipt: ReceiptId,
    id: DeliveryId,
    ordering_key: &str,
) -> Result<(), StorageError> {
    store.db.execute("INSERT INTO outbox(id,receipt,version,destination,ordering_key,payload,digest,contract,state,next_attempt) VALUES (?1,?2,1,'app',?3,x'001122',?4,1,'pending',0)",params![key(&id)?,key(&receipt)?,ordering_key,digest(&[0,17,34]).0.as_slice()])?;
    Ok(())
}

pub(crate) fn production_result(
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
            control: None,
            interference: None,
            raw_measurement: None,
            test: name,
            values: EvidenceList::new(vec![ResultValue::new(
                SourceText::new("positive")?,
                ValueInterpretation::Text,
            )])?,
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

pub(crate) fn revised_receipt(
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
    let commit = store.reconcile(
        ReconcileAction::Interpret(metadata.id),
        evidence,
        ReconcileInput::standard(&[correction], &[]),
        |_, _| Ok(Vec::new()),
    )?;
    Ok(*commit
        .outcomes
        .first()
        .ok_or(ReconciliationError::Corruption)?)
}

pub(crate) fn assert_constraint<T>(result: rusqlite::Result<T>) {
    let Err(error) = result else {
        panic!("SQL mutation must be refused")
    };
    assert!(
        matches!(
            error,
            rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error {
                    code: rusqlite::ErrorCode::ConstraintViolation,
                    ..
                },
                _
            )
        ),
        "unexpected refusal: {error:?}"
    );
}

/// A store with what the admin pages read, mixed so that the edges show:
/// three machines (one quiet), three lab apps (one never accepted anything),
/// lines in the same second, fractions of a second, an item parked before
/// schema 16, an empty target list, a resolution whose audit names a poll,
/// a delivery accepted again under an earlier clock, an order sent to two
/// machines, a repeated order state, and one message purged by data keeping.
/// Rows are written as the store writes them; times are fixed.
pub(crate) fn mixed_store(path: &Path) -> Result<Store, Box<dyn std::error::Error>> {
    let mut store = open(path)?;
    let receipts = mixed_receipts(&mut store)?;
    let deliveries = mixed_deliveries(&store, &receipts)?;
    mixed_orders(&store)?;
    mixed_review(&store, &receipts, &deliveries)?;
    // Data keeping purged the seventh message and its delivery, keeping a
    // tombstone; the item parked about it stays.
    let purged = key(receipts.get(6).ok_or("fixture receipt")?)?;
    let guards: Vec<(String, String)> = store
        .db
        .prepare("SELECT name,sql FROM sqlite_master WHERE type='trigger' AND name IN ('immutable_receipts_delete','immutable_interpretations_delete','immutable_attempts_delete','outbox_retained')")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    for (name, _) in &guards {
        store.db.execute_batch(&format!("DROP TRIGGER {name}"))?;
    }
    store.db.execute(
        "DELETE FROM attempts WHERE delivery IN (SELECT id FROM outbox WHERE receipt=?1)",
        [&purged],
    )?;
    store
        .db
        .execute("DELETE FROM outbox WHERE receipt=?1", [&purged])?;
    store
        .db
        .execute("DELETE FROM interpretations WHERE receipt=?1", [&purged])?;
    store
        .db
        .execute("DELETE FROM receipts WHERE id=?1", [&purged])?;
    for (_, sql) in &guards {
        store.db.execute_batch(sql)?;
    }
    store.db.execute(
        "INSERT INTO retention_tombstones VALUES (?1,'m-a','d','1',zeroblob(32),10200,10210,10600)",
        [&purged],
    )?;
    store.db.execute(
        "INSERT INTO retention_deliveries VALUES (?1,?2,10210)",
        params![deliveries.get(8).ok_or("fixture delivery")?, purged],
    )?;
    Ok(store)
}

/// The mixed store's messages, each with one reading.
fn mixed_receipts(store: &mut Store) -> Result<Vec<ReceiptId>, Box<dyn std::error::Error>> {
    let mut receipts = Vec::new();
    for (machine, second, nanos) in [
        ("m-a", 10_000, 0),
        ("m-b", 10_000, 0),
        ("m-a", 10_050, 700_000_000),
        ("m-a", 10_050, 200_000_000),
        ("m-b", 10_100, 0),
        ("m-quiet", 9_000, 0),
        ("m-a", 10_200, 0),
        ("m-b", 10_300, 0),
    ] {
        let raw = format!("{machine}{second}{nanos}");
        let mut metadata = receipt(raw.as_bytes())?;
        metadata.machine = Identifier::new(machine)?;
        metadata.received_at = jiff::Timestamp::new(second, nanos)?;
        store.receive(&metadata, raw.as_bytes(), b"config")?;
        let mut evidence = interpretation()?;
        evidence.outcome = Reading::Results(vec![production_result(metadata.id)?]);
        store.db.execute(
            "INSERT INTO interpretations(receipt,version,evidence) VALUES (?1,1,?2)",
            params![key(&metadata.id)?, document(&evidence)?],
        )?;
        receipts.push(metadata.id);
    }
    Ok(receipts)
}

/// The mixed store's deliveries and their attempts, in the order written.
fn mixed_deliveries(
    store: &Store,
    receipts: &[ReceiptId],
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let receipt_at = |index: usize| receipts.get(index).copied().ok_or("fixture receipt");
    let mut deliveries = Vec::new();
    for (receipt, destination, state) in [
        (Some(0), "main-lab", "done"),
        (Some(1), "outpatient", "done"),
        (Some(2), "main-lab", "parked"),
        (Some(3), "main-lab", "done"),
        (Some(4), "silent-lab", "pending"),
        (Some(4), "main-lab", "pending"),
        (None, "main-lab", "done"),
        (None, "outpatient", "conflict"),
        (Some(6), "main-lab", "done"),
        (Some(7), "outpatient", "pending"),
    ] {
        let id = key(&DeliveryId::try_from(uuid::Uuid::now_v7())?)?;
        let order = format!("o-{}", deliveries.len().saturating_sub(5));
        match receipt {
            Some(index) => store.db.execute(
                "INSERT INTO outbox(id,receipt,version,destination,ordering_key,payload,digest,contract,state,next_attempt) VALUES (?1,?2,1,?3,?1,x'00',x'00',1,?4,20000)",
                params![id, key(&receipt_at(index)?)?, destination, state],
            )?,
            None => store.db.execute(
                "INSERT INTO outbox(id,kind,order_id,destination,ordering_key,payload,digest,contract,state,next_attempt) VALUES (?1,'order_event',?2,?3,?2,x'00',x'00',1,?4,0)",
                params![id, order, destination, state],
            )?,
        };
        deliveries.push(id);
    }
    let delivery = |index: usize| deliveries.get(index).cloned().ok_or("fixture delivery");
    for (index, outcome, time) in [
        (0, "{\"RetryAt\":10015}", 10_010),
        (0, "\"Done\"", 10_020),
        (1, "\"Done\"", 10_020),
        (2, "{\"Parked\":\"HTTP 422\"}", 10_060),
        (3, "\"Done\"", 10_070),
        (3, "{\"RetryAt\":10090}", 10_080),
        (3, "\"Done\"", 10_065),
        (4, "{\"RetryAt\":10115}", 10_110),
        (4, "{\"RetryAt\":10125}", 10_120),
        (6, "\"Done\"", 10_140),
        (5, "{\"RetryAt\":20000}", 10_130),
        (7, "{\"Conflict\":\"HTTP 409\"}", 10_150),
        (8, "\"Done\"", 10_210),
        (9, "\"Done\"", 10_305),
        (9, "\"uncertain (interrupted)\"", 10_310),
        (9, "{\"RetryAt\":20000}", 10_320),
    ] {
        store.db.execute(
            "INSERT INTO attempts(delivery,outcome,time) VALUES (?1,?2,?3)",
            params![delivery(index)?, outcome, time],
        )?;
    }
    Ok(deliveries)
}

/// The mixed store's machine orders and their snapshots.
fn mixed_orders(store: &Store) -> Result<(), Box<dyn std::error::Error>> {
    store.db.execute_batch(concat!(
        "INSERT INTO feed(destination,cursor,complete,trusted,polled) VALUES ('main-lab','c',1,1,0);",
        "INSERT INTO orders VALUES ('main-lab','o-1','000777','{}',0,0,0);",
        "INSERT INTO machine_orders(destination,order_id,machine,version,state,created_at,updated_at,source_code) VALUES ",
        "('main-lab','o-1','m-a','1','Done',10130,10400,'x'),",
        "('outpatient','o-2','m-a','1','Sent',10140,10140,'x'),",
        "('outpatient','o-2','m-b','1','Sent',10140,10140,'x');",
    ))?;
    let snapshot = |order: &str, destination: &str, machine: &str, state: &str, at: Option<i64>| {
        let updated = at.map_or_else(String::new, |at| format!(",\"updated_at\":{at}"));
        format!(
            "{{\"key\":{{\"destination\":\"{destination}\",\"order\":\"{order}\",\"machine\":\"{machine}\"}},\"state\":\"{state}\"{updated},\"items\":[{{}}]}}"
        )
    };
    for (order, row, destination, machine, state, at) in [
        ("o-1", 1, "main-lab", "m-a", "Sent", Some(10_130)),
        ("o-1", 1, "main-lab", "m-a", "Sent", Some(10_135)),
        ("o-1", 1, "main-lab", "m-a", "Accepted", Some(10_135)),
        ("o-2", 2, "outpatient", "m-a", "Sent", Some(10_140)),
        ("o-2", 3, "outpatient", "m-b", "Sent", Some(10_140)),
        ("o-2", 3, "outpatient", "m-b", "Accepted", None),
        ("o-1", 1, "main-lab", "m-a", "Done", Some(10_400)),
    ] {
        store.db.execute(
            "INSERT INTO machine_order_history(machine_order,evidence) VALUES (?1,?2)",
            params![row, snapshot(order, destination, machine, state, at)],
        )?;
    }
    Ok(())
}

/// The mixed store's parked items, poll intent and audit entries.
fn mixed_review(
    store: &Store,
    receipts: &[ReceiptId],
    deliveries: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    let bare = |id: ReceiptId| uuid::Uuid::from(id).to_string();
    let receipt_at = |index: usize| receipts.get(index).copied().ok_or("fixture receipt");
    let delivery = |index: usize| deliveries.get(index).cloned().ok_or("fixture delivery");
    let item = |kind: &str, targets: &[String]| -> Result<String, serde_json::Error> {
        serde_json::to_string(&serde_json::json!({
            "kind": kind, "reason": "synthetic", "targets": targets, "context": null
        }))
    };
    let audit = |action: &str, target: &str, time: i64| -> Result<String, serde_json::Error> {
        serde_json::to_string(&serde_json::json!({
            "actor": "admin-session-1", "action": action, "targets": [target],
            "time": time, "reason": "synthetic"
        }))
    };
    let delivery_target = serde_json::from_str::<String>(&delivery(2)?)?;
    let resolved = audit("parked-resolve", "2", 10_500)?;
    let confirmed = audit("poll-confirm", "poll:1", 10_500)?;
    for (evidence, parked_at, resolution) in [
        (
            item("delivery_rejected", &[delivery_target])?,
            Some(10_060),
            None,
        ),
        (
            item("mapping_finding", &[bare(receipt_at(0)?)])?,
            Some(10_001),
            Some(&resolved),
        ),
        (
            item("poll", &["m-quiet".to_owned()])?,
            Some(10_002),
            Some(&confirmed),
        ),
        (item("mapping_finding", &[])?, Some(10_001), None),
        (
            item("mapping_finding", &["1".to_owned()])?,
            Some(10_150),
            None,
        ),
        (
            item("incomplete_transfer", &["t-1".to_owned()])?,
            Some(10_150),
            None,
        ),
    ] {
        store.db.execute(
            "INSERT INTO parked(evidence,parked_at,resolution) VALUES (?1,?2,?3)",
            params![evidence, parked_at, resolution],
        )?;
    }
    store.db.execute_batch(concat!(
        "INSERT INTO poll_intents(machine,parked,state,reason) VALUES ('m-quiet',3,'confirmed','synthetic');",
        "INSERT INTO frame_provenance VALUES ('t-1','{\"machine\":\"m-b\"}');",
    ))?;
    // Two items parked before schema 16, with no time.
    let trigger: String = store.db.query_row(
        "SELECT sql FROM sqlite_master WHERE name='parked_time'",
        [],
        |r| r.get(0),
    )?;
    store.db.execute_batch("DROP TRIGGER parked_time")?;
    for target in [bare(receipt_at(6)?), bare(receipt_at(1)?)] {
        store.db.execute(
            "INSERT INTO parked(evidence) VALUES (?1)",
            [item("interpretation_failure", &[target])?],
        )?;
    }
    store.db.execute_batch(&trigger)?;
    for (action, target, time) in [
        ("setup-save", "m-a", 10_200),
        ("setup-save", "m-b", 10_200),
        ("lab-app-save", "main-lab", 10_250),
        ("admin-raw-view", "receipt:x", 10_260),
        ("deliveries-list", "main-lab", 10_260),
    ] {
        store.db.execute(
            "INSERT INTO audit(evidence) VALUES (?1)",
            [audit(action, target, time)?],
        )?;
    }
    for resolution in [&resolved, &confirmed] {
        store
            .db
            .execute("INSERT INTO audit(evidence) VALUES (?1)", [resolution])?;
    }
    Ok(())
}
