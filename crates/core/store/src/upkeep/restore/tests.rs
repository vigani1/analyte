//! Tests offline restore and displaced database retention.
use crate::test_support::*;
use crate::*;
use analyte_model::Reason;

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
            Instant::now() + HANG_GUARD
        ),
        Err(MaintenanceError::StorageCorruption(_))
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
            Instant::now() + HANG_GUARD
        ),
        Err(MaintenanceError::SchemaTooNew(999))
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
    assert!(matches!(open(&path), Err(MaintenanceError::Conflict)));
    assert!(!path.exists());
    assert_eq!(
        std::fs::read(path.with_extension("restore-pending"))?,
        b"interrupted stage"
    );
    Ok(())
}

#[test]
fn plan11_rule7_restore_refuses_zero_page_limit_before_publishing() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("current.db");
    let source = temp.path().join("backup.db");
    drop(open(&source)?);
    let mut store = open(&path)?;
    let metadata = receipt(b"current evidence")?;
    store.receive(&metadata, b"current evidence", b"config")?;
    drop(store);
    let audit = Audit {
        actor: Identifier::new("operator")?,
        action: Identifier::new("store-restore")?,
        targets: vec![],
        time: 0,
        reason: Reason::new("synthetic recovery")?,
    };
    let invalid = {
        let mut policy = limits();
        policy.page_rows = 0;
        policy
    };
    // The same public limits are refused by normal open. Restore must not bypass admission.
    assert!(matches!(
        Store::open(
            &temp.path().join("invalid.db"),
            {
                let mut policy = limits();
                policy.page_rows = 0;
                policy
            },
            Box::new(Space(u64::MAX))
        ),
        Err(MaintenanceError::InvalidLimits)
    ));
    let result = Store::restore(
        &path,
        &source,
        invalid,
        Box::new(Space(u64::MAX)),
        &audit,
        Instant::now() + HANG_GUARD,
    );
    assert!(
        matches!(result, Err(MaintenanceError::InvalidLimits)),
        "restore accepted invalid zero page limit: {result:?}"
    );
    assert_eq!(
        open(&path)?.raw(metadata.id)?,
        Some(b"current evidence".to_vec())
    );
    assert!(!path.with_extension("before-restore").exists());
    Ok(())
}

#[test]
fn mutation_plan11_backup_and_restore_keep_every_page_and_the_displaced_store() -> TestResult {
    let temp = tempfile::tempdir()?;
    let source = temp.path().join("source.db");
    let backup = temp.path().join("backup.db");
    let target = temp.path().join("current.db");
    let transfer = Identifier::new("large-transfer")?;
    let mut store = open(&source)?;
    let raw = vec![b'x'; 60_000];
    for number in 0..8 {
        store.frame(&transfer, FrameSequence::new(number, 7)?, &raw)?;
    }
    let pages: i64 = store.db.query_row("PRAGMA page_count", [], |r| r.get(0))?;
    assert!(pages > 64);
    store.backup(&backup, Instant::now() + HANG_GUARD)?;
    drop(store);
    let mut current = open(&target)?;
    let metadata = receipt(b"displaced original")?;
    current.receive(&metadata, b"displaced original", b"config")?;
    drop(current);
    let audit = Audit {
        actor: Identifier::new("operator")?,
        action: Identifier::new("store-restore")?,
        targets: vec![],
        time: 42,
        reason: Reason::new("synthetic recovery")?,
    };
    let kept = Store::restore(
        &target,
        &backup,
        limits(),
        Box::new(Space(u64::MAX)),
        &audit,
        Instant::now() + HANG_GUARD,
    )?;
    assert_eq!(
        open(&kept)?.raw(metadata.id)?,
        Some(b"displaced original".to_vec())
    );
    let restored = open(&target)?;
    let frames = restored.frames(&transfer, 0)?;
    assert_eq!(frames.len(), 8);
    for (number, frame) in frames.iter().enumerate() {
        assert_eq!(usize::from(frame.number), number);
        assert_eq!(frame.raw, raw);
    }
    assert!(restored.restore_unreconciled()?);
    assert_eq!(restored.audits(0)?.len(), 1);
    Ok(())
}

#[test]
fn mutation_plan11_restore_refuses_each_existing_recovery_file_before_staging() -> TestResult {
    for extension in ["before-restore", "restore-pending"] {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("current.db");
        let source = temp.path().join("source.db");
        drop(open(&source)?);
        drop(open(&path)?);
        let sentinel = path.with_extension(extension);
        std::fs::write(&sentinel, b"retained recovery evidence")?;
        let audit = Audit {
            actor: Identifier::new("operator")?,
            action: Identifier::new("store-restore")?,
            targets: vec![],
            time: 42,
            reason: Reason::new("synthetic recovery")?,
        };
        assert!(matches!(
            Store::restore(
                &path,
                &source,
                limits(),
                Box::new(Space(u64::MAX)),
                &audit,
                Instant::now() + HANG_GUARD
            ),
            Err(MaintenanceError::Conflict)
        ));
        assert_eq!(std::fs::read(&sentinel)?, b"retained recovery evidence");
        if extension == "before-restore" {
            assert!(!path.with_extension("restore-pending").exists());
        }
    }
    Ok(())
}

#[test]
fn mutation_plan11_restore_accepts_schema_one_and_refuses_nonpositive_schema() -> TestResult {
    for version in [-1, 0, 1] {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("current.db");
        let source = temp.path().join("source.db");
        drop(open(&path)?);
        let db = Connection::open(&source)?;
        db.execute_batch(include_str!("../schema.sql"))?;
        db.pragma_update(None, "user_version", version)?;
        drop(db);
        let audit = Audit {
            actor: Identifier::new("operator")?,
            action: Identifier::new("store-restore")?,
            targets: vec![],
            time: 42,
            reason: Reason::new("synthetic recovery")?,
        };
        let result = Store::restore(
            &path,
            &source,
            limits(),
            Box::new(Space(u64::MAX)),
            &audit,
            Instant::now() + HANG_GUARD,
        );
        if version == 1 {
            result?;
            assert!(open(&path)?.restore_unreconciled()?);
        } else {
            assert!(matches!(result, Err(MaintenanceError::Corruption)));
            assert!(!path.with_extension("restore-pending").exists());
        }
    }
    Ok(())
}

#[test]
fn mutation_plan11_rule7_restore_refuses_exhausted_reserve_before_publishing() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("current.db");
    let source = temp.path().join("backup.db");
    drop(open(&source)?);
    let mut current = open(&path)?;
    let metadata = receipt(b"current custody")?;
    current.receive(&metadata, b"current custody", b"config")?;
    drop(current);
    // No available bytes can admit even one byte above the configured disk reserve.
    let probe = Store::open(&temp.path().join("probe.db"), limits(), Box::new(Space(0)))?;
    assert!(matches!(probe.admission(1), Err(IntakeError::Capacity)));
    drop(probe);
    let audit = Audit {
        actor: Identifier::new("operator")?,
        action: Identifier::new("store-restore")?,
        targets: vec![],
        time: 42,
        reason: Reason::new("synthetic recovery")?,
    };
    let result = Store::restore(
        &path,
        &source,
        limits(),
        Box::new(Space(0)),
        &audit,
        Instant::now() + HANG_GUARD,
    );
    assert!(
        matches!(result, Err(MaintenanceError::Capacity)),
        "restore published despite exhausted disk reserve: {result:?}"
    );
    assert_eq!(
        open(&path)?.raw(metadata.id)?,
        Some(b"current custody".to_vec())
    );
    assert!(!path.with_extension("before-restore").exists());
    Ok(())
}

#[test]
fn mutation_plan11_restore_upgrade_refuses_a_database_that_grows_past_its_limit() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("current.db");
    let source = temp.path().join("backup.db");
    let current = receipt(b"current original")?;
    let backup = receipt(b"backup original")?;
    let mut bytes = Vec::new();
    for (file, metadata, raw) in [
        (&path, &current, b"current original".as_slice()),
        (&source, &backup, b"backup original".as_slice()),
    ] {
        let db = Connection::open(file)?;
        db.execute_batch(include_str!("../schema.sql"))?;
        db.execute(
            "INSERT INTO receipts VALUES (?1,?2,?3,?4)",
            params![key(&metadata.id)?, document(metadata)?, raw, b"config"],
        )?;
        let pages: i64 = db.query_row("PRAGMA page_count", [], |r| r.get(0))?;
        let page: i64 = db.query_row("PRAGMA page_size", [], |r| r.get(0))?;
        bytes.push(pages.checked_mul(page).ok_or("database size")?);
    }
    let limit = bytes
        .into_iter()
        .max()
        .ok_or("database sizes")?
        .checked_add(1)
        .ok_or("size limit")?;
    let audit = Audit {
        actor: Identifier::new("operator")?,
        action: Identifier::new("store-restore")?,
        targets: vec![],
        time: 42,
        reason: Reason::new("synthetic old-schema recovery")?,
    };
    let result = Store::restore(
        &path,
        &source,
        {
            let mut policy = limits();
            policy.database_bytes = u64::try_from(limit)?;
            policy
        },
        Box::new(Space(u64::MAX)),
        &audit,
        Instant::now() + HANG_GUARD,
    );
    let db = Connection::open(&path)?;
    let pages: i64 = db.query_row("PRAGMA page_count", [], |r| r.get(0))?;
    let page: i64 = db.query_row("PRAGMA page_size", [], |r| r.get(0))?;
    let actual = pages.checked_mul(page).ok_or("restored size")?;
    assert!(
        matches!(result, Err(MaintenanceError::Capacity)),
        "restore result {result:?}; published size {actual} exceeds configured limit {limit}"
    );
    assert_eq!(
        db.query_row(
            "SELECT raw FROM receipts WHERE id=?1",
            [key(&current.id)?],
            |r| r.get::<_, Vec<u8>>(0)
        )?,
        b"current original"
    );
    assert!(!path.with_extension("before-restore").exists());
    // A refused restore removes only its own staged copy; the backup and current store
    // stay, and ordinary startup is not blocked by a leftover stage.
    assert!(!path.with_extension("restore-pending").exists());
    assert!(source.exists());
    drop(db);
    assert_eq!(
        open(&path)?.raw(current.id)?,
        Some(b"current original".to_vec())
    );
    Ok(())
}

#[test]
fn mutation_plan11_rule7_restore_capacity_includes_the_copy_and_refuses_equality() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("source.db");
    let store = open(&path)?;
    let pages: u64 = store.db.query_row("PRAGMA page_count", [], |r| {
        r.get::<_, u32>(0).map(u64::from)
    })?;
    let page: u64 = store.db.query_row("PRAGMA page_size", [], |r| {
        r.get::<_, u32>(0).map(u64::from)
    })?;
    let size = pages.checked_mul(page).ok_or("copy size overflow")?;
    let needed = 1024u64.checked_add(size).ok_or("reserve overflow")?;
    for (available, free_ok) in [(needed - 1, false), (needed, false), (needed + 1, true)] {
        for (maximum, size_ok) in [(size - 1, false), (size, false), (size + 1, true)] {
            let mut bounds = limits();
            bounds.database_bytes = maximum;
            let result = Store::restore_capacity(&store.db, &path, &bounds, &Space(available));
            if free_ok && size_ok {
                result?;
            } else {
                assert!(matches!(result, Err(StorageError::Capacity)));
            }
        }
    }
    let mut bounds = limits();
    bounds.reserve_bytes = u64::MAX;
    assert!(matches!(
        Store::restore_capacity(&store.db, &path, &bounds, &Space(u64::MAX)),
        Err(StorageError::Capacity)
    ));
    Ok(())
}

#[test]
fn mutation_plan11_restore_busy_copy_refuses_with_timeout_and_preserves_both_stores() -> TestResult
{
    struct LockSource {
        source: PathBuf,
        held: std::sync::Mutex<Option<Connection>>,
    }
    impl FreeSpace for LockSource {
        fn available(&self, _: &Path) -> std::io::Result<u64> {
            let mut held = self
                .held
                .lock()
                .map_err(|_| std::io::Error::other("probe lock poisoned"))?;
            if held.is_none() {
                let db = Connection::open(&self.source).map_err(std::io::Error::other)?;
                db.execute_batch("BEGIN EXCLUSIVE")
                    .map_err(std::io::Error::other)?;
                *held = Some(db);
            }
            Ok(u64::MAX)
        }
    }
    let temp = tempfile::tempdir()?;
    let source = temp.path().join("backup.db");
    let path = temp.path().join("current.db");
    let backup = receipt(b"backup original")?;
    let current = receipt(b"current original")?;
    let mut store = open(&source)?;
    store.receive(&backup, b"backup original", b"config")?;
    drop(store);
    let db = Connection::open(&source)?;
    db.execute_batch("PRAGMA journal_mode=DELETE")?;
    drop(db);
    let mut store = open(&path)?;
    store.receive(&current, b"current original", b"config")?;
    drop(store);
    let original = std::fs::read(&source)?;
    let audit = Audit {
        actor: Identifier::new("operator")?,
        action: Identifier::new("store-restore")?,
        targets: vec![],
        time: 1,
        reason: Reason::new("test locked backup")?,
    };
    let result = Store::restore(
        &path,
        &source,
        limits(),
        Box::new(LockSource {
            source: source.clone(),
            held: std::sync::Mutex::new(None),
        }),
        &audit,
        Instant::now() + HANG_GUARD,
    );
    assert!(
        matches!(result, Err(MaintenanceError::Timeout)),
        "busy copy must be a timeout: {result:?}"
    );
    assert_eq!(std::fs::read(&source)?, original);
    let db = Connection::open(&path)?;
    let raw: Vec<u8> = db.query_row(
        "SELECT raw FROM receipts WHERE id=?1",
        [key(&current.id)?],
        |r| r.get(0),
    )?;
    assert_eq!(raw, b"current original");
    assert!(!path.with_extension("before-restore").exists());
    Ok(())
}
