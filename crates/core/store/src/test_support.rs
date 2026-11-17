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

pub(crate) fn open(path: &Path) -> Result<Store, Error> {
    Ok(Store::open(path, limits(), Box::new(Space(u64::MAX)))?)
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
) -> Result<(), Error> {
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
    Ok(*commit.outcomes.first().ok_or(Error::Corruption)?)
}

/// Removes only empty post-v11 notification tables when constructing an older fixture.
pub(crate) fn remove_alert_schema_for_legacy_fixture(db: &Connection) -> TestResult {
    for table in [
        "alert_reminders",
        "alert_progress",
        "alert_events",
        "alert_endpoints",
    ] {
        let count: i64 =
            db.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))?;
        assert_eq!(
            count, 0,
            "legacy fixture must not discard notification evidence"
        );
        db.execute_batch(&format!("DROP TABLE {table}"))?;
    }
    db.execute_batch("DROP INDEX alert_history_episode; DROP TRIGGER alert_installation_retained; DROP TABLE alert_installation;")?;
    Ok(())
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
